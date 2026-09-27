use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{
    CommunicationRule, CommunicationRuleRepository, RoleAssignment, RoleAssignmentRepository,
};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, db_enum, db_minutes, opt_minutes, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct AssignmentRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    service_id: Uuid,
    role_id: Uuid,
    person_id: Uuid,
    is_deputy: bool,
    escalation_order: Option<i32>,
}

impl From<AssignmentRow> for RoleAssignment {
    fn from(r: AssignmentRow) -> Self {
        RoleAssignment {
            meta: r.meta.into(),
            service_id: r.service_id,
            role_id: r.role_id,
            person_id: r.person_id,
            is_deputy: r.is_deputy,
            escalation_order: r.escalation_order.map(|v| v.max(1) as u32),
        }
    }
}

pub struct PgRoleAssignmentRepository(pub Db);

#[async_trait]
impl RoleAssignmentRepository for PgRoleAssignmentRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RoleAssignment>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<AssignmentRow> = sqlx::query_as(
            "select * from role_assignment where tenant_id = $1 and service_id = $2
             order by role_id, is_deputy, escalation_order nulls last, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(RoleAssignment::from).collect())
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RoleAssignment>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<AssignmentRow> =
            sqlx::query_as("select * from role_assignment where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(row.map(RoleAssignment::from))
    }

    async fn insert(&self, ctx: &TenantContext, a: &RoleAssignment) -> AppResult<RoleAssignment> {
        let mut tx = self.0.begin(ctx).await?;
        let row: AssignmentRow = sqlx::query_as(
            "insert into role_assignment (id, tenant_id, service_id, role_id, person_id, is_deputy, escalation_order,
                                          created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $8) returning *",
        )
        .bind(a.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(a.service_id)
        .bind(a.role_id)
        .bind(a.person_id)
        .bind(a.is_deputy)
        .bind(a.escalation_order.map(|v| v as i32))
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(row.into())
    }

    async fn update(&self, ctx: &TenantContext, a: &RoleAssignment) -> AppResult<RoleAssignment> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<AssignmentRow> = sqlx::query_as(
            "update role_assignment set role_id = $4, person_id = $5, is_deputy = $6, escalation_order = $7,
                    version = version + 1, updated_at = now(), updated_by = $8
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(a.meta.id)
        .bind(a.meta.version)
        .bind(a.role_id)
        .bind(a.person_id)
        .bind(a.is_deputy)
        .bind(a.escalation_order.map(|v| v as i32))
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(require_updated(row)?.into())
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool> {
        let mut tx = self.0.begin(ctx).await?;
        let r = sqlx::query("delete from role_assignment where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(r.rows_affected() > 0)
    }
}

#[derive(FromRow)]
struct RuleRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    service_id: Uuid,
    trigger: String,
    audience: String,
    channel: String,
    frequency_minutes: Option<i32>,
    responsible_role_id: Uuid,
    authorizer_role_id: Option<Uuid>,
    template: Option<String>,
}

impl TryFrom<RuleRow> for CommunicationRule {
    type Error = AppError;

    fn try_from(r: RuleRow) -> AppResult<Self> {
        Ok(CommunicationRule {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            service_id: r.service_id,
            trigger: db_enum(&r.trigger)?,
            audience: db_enum(&r.audience)?,
            channel: r.channel,
            frequency: db_minutes(r.frequency_minutes)?,
            responsible_role_id: r.responsible_role_id,
            authorizer_role_id: r.authorizer_role_id,
            template: r.template,
        })
    }
}

pub struct PgCommunicationRuleRepository(pub Db);

#[async_trait]
impl CommunicationRuleRepository for PgCommunicationRuleRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<CommunicationRule>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<RuleRow> = sqlx::query_as(
            "select * from communication_rule where tenant_id = $1 and service_id = $2 order by trigger, audience, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(CommunicationRule::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<CommunicationRule>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RuleRow> =
            sqlx::query_as("select * from communication_rule where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(CommunicationRule::try_from).transpose()
    }

    async fn insert(
        &self,
        ctx: &TenantContext,
        r: &CommunicationRule,
    ) -> AppResult<CommunicationRule> {
        let mut tx = self.0.begin(ctx).await?;
        let row: RuleRow = sqlx::query_as(
            "insert into communication_rule (id, tenant_id, service_id, trigger, audience, channel, frequency_minutes,
                    responsible_role_id, authorizer_role_id, template, origin, ai_suggestion_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13) returning *",
        )
        .bind(r.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(r.service_id)
        .bind(r.trigger.as_str())
        .bind(r.audience.as_str())
        .bind(&r.channel)
        .bind(opt_minutes(r.frequency))
        .bind(r.responsible_role_id)
        .bind(r.authorizer_role_id)
        .bind(&r.template)
        .bind(r.provenance.origin.as_str())
        .bind(r.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(
        &self,
        ctx: &TenantContext,
        r: &CommunicationRule,
    ) -> AppResult<CommunicationRule> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RuleRow> = sqlx::query_as(
            "update communication_rule set trigger = $4, audience = $5, channel = $6, frequency_minutes = $7,
                    responsible_role_id = $8, authorizer_role_id = $9, template = $10,
                    version = version + 1, updated_at = now(), updated_by = $11
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(r.meta.id)
        .bind(r.meta.version)
        .bind(r.trigger.as_str())
        .bind(r.audience.as_str())
        .bind(&r.channel)
        .bind(opt_minutes(r.frequency))
        .bind(r.responsible_role_id)
        .bind(r.authorizer_role_id)
        .bind(&r.template)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool> {
        let mut tx = self.0.begin(ctx).await?;
        let r = sqlx::query("delete from communication_rule where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(r.rows_affected() > 0)
    }
}
