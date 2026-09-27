use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{ObjectiveRepository, RecoveryObjective};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, db_minutes, opt_minutes, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

#[derive(FromRow)]
struct ObjectiveRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    microservice_id: Uuid,
    scenario_id: Option<Uuid>,
    rto_minutes: i32,
    rpo_minutes: i32,
    mttr_target_minutes: Option<i32>,
    restore_priority: Option<i32>,
    first_functions: Vec<String>,
}

impl TryFrom<ObjectiveRow> for RecoveryObjective {
    type Error = AppError;

    fn try_from(r: ObjectiveRow) -> AppResult<Self> {
        Ok(RecoveryObjective {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            microservice_id: r.microservice_id,
            scenario_id: r.scenario_id,
            rto: Minutes::from_db(r.rto_minutes)?,
            rpo: Minutes::from_db(r.rpo_minutes)?,
            mttr_target: db_minutes(r.mttr_target_minutes)?,
            restore_priority: r.restore_priority.map(|v| v.max(1) as u32),
            first_functions: r.first_functions,
        })
    }
}

pub struct PgObjectiveRepository(pub Db);

#[async_trait]
impl ObjectiveRepository for PgObjectiveRepository {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<RecoveryObjective>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ObjectiveRow> = sqlx::query_as(
            "select * from recovery_objective where tenant_id = $1 and microservice_id = $2
             order by scenario_id nulls first, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(microservice_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(RecoveryObjective::try_from).collect()
    }

    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RecoveryObjective>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ObjectiveRow> = sqlx::query_as(
            "select o.* from recovery_objective o join microservice m on m.id = o.microservice_id
             where o.tenant_id = $1 and m.service_id = $2 order by o.microservice_id, o.scenario_id nulls first",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(RecoveryObjective::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryObjective>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ObjectiveRow> =
            sqlx::query_as("select * from recovery_objective where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(RecoveryObjective::try_from).transpose()
    }

    async fn insert(
        &self,
        ctx: &TenantContext,
        o: &RecoveryObjective,
    ) -> AppResult<RecoveryObjective> {
        let mut tx = self.0.begin(ctx).await?;
        let row: ObjectiveRow = sqlx::query_as(
            "insert into recovery_objective (id, tenant_id, microservice_id, scenario_id, rto_minutes, rpo_minutes,
                    mttr_target_minutes, restore_priority, first_functions, origin, ai_suggestion_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12) returning *",
        )
        .bind(o.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(o.microservice_id)
        .bind(o.scenario_id)
        .bind(o.rto.to_db())
        .bind(o.rpo.to_db())
        .bind(opt_minutes(o.mttr_target))
        .bind(o.restore_priority.map(|v| v as i32))
        .bind(&o.first_functions)
        .bind(o.provenance.origin.as_str())
        .bind(o.provenance.ai_suggestion_id)
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
        o: &RecoveryObjective,
    ) -> AppResult<RecoveryObjective> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ObjectiveRow> = sqlx::query_as(
            "update recovery_objective set scenario_id = $4, rto_minutes = $5, rpo_minutes = $6, mttr_target_minutes = $7,
                    restore_priority = $8, first_functions = $9, version = version + 1, updated_at = now(), updated_by = $10
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(o.meta.id)
        .bind(o.meta.version)
        .bind(o.scenario_id)
        .bind(o.rto.to_db())
        .bind(o.rpo.to_db())
        .bind(opt_minutes(o.mttr_target))
        .bind(o.restore_priority.map(|v| v as i32))
        .bind(&o.first_functions)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from recovery_objective where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
