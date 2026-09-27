use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{ItService, ItServiceRepository, ServiceFilter, ServiceSummary};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, db_enum, db_enum_opt, like_pattern, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, Page, PageRequest, TenantContext};

#[derive(FromRow)]
struct ItServiceRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    name: String,
    description: Option<String>,
    business_owner_id: Option<Uuid>,
    technical_owner_id: Option<Uuid>,
    consumers: Vec<String>,
    protection_requirement_availability: Option<String>,
    impact_level: Option<String>,
    lifecycle_status: String,
}

impl TryFrom<ItServiceRow> for ItService {
    type Error = AppError;

    fn try_from(r: ItServiceRow) -> AppResult<Self> {
        Ok(ItService {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            name: r.name,
            description: r.description,
            business_owner_id: r.business_owner_id,
            technical_owner_id: r.technical_owner_id,
            consumers: r.consumers,
            protection_requirement_availability: db_enum_opt(
                r.protection_requirement_availability.as_deref(),
            )?,
            impact_level: db_enum_opt(r.impact_level.as_deref())?,
            lifecycle_status: db_enum(&r.lifecycle_status)?,
        })
    }
}

#[derive(FromRow)]
struct SummaryRow {
    id: Uuid,
    microservice_count: i64,
    selected_scenario_count: i64,
    completed_workflow_steps: i64,
    current_plan_version_id: Option<Uuid>,
    current_plan_status: Option<String>,
    next_review_due: Option<NaiveDate>,
    last_tested_at: Option<DateTime<Utc>>,
    open_action_item_count: i64,
    active_recovery_run_id: Option<Uuid>,
}

pub struct PgItServiceRepository(pub Db);

#[async_trait]
impl ItServiceRepository for PgItServiceRepository {
    async fn list(
        &self,
        ctx: &TenantContext,
        f: &ServiceFilter,
        page: PageRequest,
    ) -> AppResult<Page<ItService>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ItServiceRow> = sqlx::query_as(
            "select s.* from it_service s
             where s.tenant_id = $1
               and ($2::text is null or s.name ilike $2 or s.description ilike $2)
               and ($3::text is null or s.lifecycle_status = $3)
               and ($4::boolean is null or $4 = exists(
                     select 1 from plan_version p
                     where p.service_id = s.id and p.status = 'approved' and p.next_review_due < current_date))
             order by s.name, s.id limit $5 offset $6",
        )
        .bind(ctx.tenant_id.0)
        .bind(f.query.as_deref().map(like_pattern))
        .bind(f.lifecycle_status.map(|s| s.as_str()))
        .bind(f.review_overdue)
        .bind(page.fetch_limit())
        .bind(page.offset)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let items = rows
            .into_iter()
            .map(ItService::try_from)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Page::from_overfetch(items, page))
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<ItService>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ItServiceRow> =
            sqlx::query_as("select * from it_service where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(ItService::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, s: &ItService) -> AppResult<ItService> {
        let mut tx = self.0.begin(ctx).await?;
        let row: ItServiceRow = sqlx::query_as(
            "insert into it_service (id, tenant_id, name, description, business_owner_id, technical_owner_id, consumers,
                                     protection_requirement_availability, impact_level, lifecycle_status,
                                     origin, ai_suggestion_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13) returning *",
        )
        .bind(s.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(&s.name)
        .bind(&s.description)
        .bind(s.business_owner_id)
        .bind(s.technical_owner_id)
        .bind(&s.consumers)
        .bind(s.protection_requirement_availability.map(|v| v.as_str()))
        .bind(s.impact_level.map(|v| v.as_str()))
        .bind(s.lifecycle_status.as_str())
        .bind(s.provenance.origin.as_str())
        .bind(s.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, s: &ItService) -> AppResult<ItService> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ItServiceRow> = sqlx::query_as(
            "update it_service set name = $4, description = $5, business_owner_id = $6, technical_owner_id = $7,
                    consumers = $8, protection_requirement_availability = $9, impact_level = $10,
                    lifecycle_status = $11, version = version + 1, updated_at = now(), updated_by = $12
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(s.meta.id)
        .bind(s.meta.version)
        .bind(&s.name)
        .bind(&s.description)
        .bind(s.business_owner_id)
        .bind(s.technical_owner_id)
        .bind(&s.consumers)
        .bind(s.protection_requirement_availability.map(|v| v.as_str()))
        .bind(s.impact_level.map(|v| v.as_str()))
        .bind(s.lifecycle_status.as_str())
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from it_service where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn summaries(
        &self,
        ctx: &TenantContext,
        ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, ServiceSummary>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<SummaryRow> = sqlx::query_as(
            "select s.id,
                (select count(*) from microservice m where m.service_id = s.id) as microservice_count,
                (select count(*) from scenario c where c.service_id = s.id and c.status = 'selected') as selected_scenario_count,
                (select count(*) from workflow_progress w where w.service_id = s.id and w.status = 'complete') as completed_workflow_steps,
                pv.id as current_plan_version_id,
                pv.status as current_plan_status,
                (select p.next_review_due from plan_version p where p.service_id = s.id and p.status = 'approved') as next_review_due,
                (select max(t.executed_at) from dr_test t where t.service_id = s.id) as last_tested_at,
                (select count(*) from action_item a where a.service_id = s.id and a.status in ('open', 'in_progress')) as open_action_item_count,
                (select r.id from recovery_run r where r.service_id = s.id
                    and r.status in ('declared', 'in_progress', 'recovered', 'reconstituting') limit 1) as active_recovery_run_id
             from it_service s
             left join lateral (
                select p.id, p.status from plan_version p where p.service_id = s.id order by p.plan_number desc limit 1
             ) pv on true
             where s.tenant_id = $1 and s.id = any($2)",
        )
        .bind(ctx.tenant_id.0)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.id,
                    ServiceSummary {
                        microservice_count: r.microservice_count,
                        selected_scenario_count: r.selected_scenario_count,
                        completed_workflow_steps: r.completed_workflow_steps,
                        current_plan_version_id: r.current_plan_version_id,
                        current_plan_status: r.current_plan_status,
                        next_review_due: r.next_review_due,
                        last_tested_at: r.last_tested_at,
                        open_action_item_count: r.open_action_item_count,
                        active_recovery_run_id: r.active_recovery_run_id,
                    },
                )
            })
            .collect())
    }
}
