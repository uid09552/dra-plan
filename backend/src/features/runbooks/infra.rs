use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Runbook, RunbookRepository, RunbookStep};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, Tx, db_enum, db_minutes, opt_minutes, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct RunbookRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    microservice_id: Uuid,
    scenario_id: Uuid,
    strategy_id: Option<Uuid>,
    title: String,
    description: Option<String>,
}

impl TryFrom<RunbookRow> for Runbook {
    type Error = AppError;

    fn try_from(r: RunbookRow) -> AppResult<Self> {
        Ok(Runbook {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            microservice_id: r.microservice_id,
            scenario_id: r.scenario_id,
            strategy_id: r.strategy_id,
            title: r.title,
            description: r.description,
        })
    }
}

#[derive(FromRow)]
struct StepRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    runbook_id: Uuid,
    seq: i32,
    phase: String,
    title: String,
    instructions: Option<String>,
    owner_role_id: Option<Uuid>,
    expected_duration_minutes: Option<i32>,
    verification: Option<String>,
    depends_on: Vec<Uuid>,
    is_decision_point: bool,
    requires_authorization_role_id: Option<Uuid>,
}

impl TryFrom<StepRow> for RunbookStep {
    type Error = AppError;

    fn try_from(r: StepRow) -> AppResult<Self> {
        Ok(RunbookStep {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            runbook_id: r.runbook_id,
            seq: r.seq.max(1) as u32,
            phase: db_enum(&r.phase)?,
            title: r.title,
            instructions: r.instructions,
            owner_role_id: r.owner_role_id,
            expected_duration: db_minutes(r.expected_duration_minutes)?,
            verification: r.verification,
            depends_on: r.depends_on,
            is_decision_point: r.is_decision_point,
            requires_authorization_role_id: r.requires_authorization_role_id,
        })
    }
}

/// Inserts or updates a step. Updates only happen when something changed, so unchanged steps
/// keep their version and produce no audit entry.
async fn upsert_step(tx: &mut Tx, ctx: &TenantContext, s: &RunbookStep) -> AppResult<()> {
    sqlx::query(
        "insert into runbook_step (id, tenant_id, runbook_id, seq, phase, title, instructions, owner_role_id,
                expected_duration_minutes, verification, depends_on, is_decision_point, requires_authorization_role_id,
                origin, ai_suggestion_id, created_by, updated_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
         on conflict (id) do update set
            seq = excluded.seq, phase = excluded.phase, title = excluded.title, instructions = excluded.instructions,
            owner_role_id = excluded.owner_role_id, expected_duration_minutes = excluded.expected_duration_minutes,
            verification = excluded.verification, depends_on = excluded.depends_on,
            is_decision_point = excluded.is_decision_point,
            requires_authorization_role_id = excluded.requires_authorization_role_id,
            version = runbook_step.version + 1, updated_at = now(), updated_by = excluded.updated_by
         where runbook_step.tenant_id = excluded.tenant_id
           and (runbook_step.seq, runbook_step.phase, runbook_step.title, runbook_step.instructions,
                runbook_step.owner_role_id, runbook_step.expected_duration_minutes, runbook_step.verification,
                runbook_step.depends_on, runbook_step.is_decision_point, runbook_step.requires_authorization_role_id)
               is distinct from
               (excluded.seq, excluded.phase, excluded.title, excluded.instructions, excluded.owner_role_id,
                excluded.expected_duration_minutes, excluded.verification, excluded.depends_on,
                excluded.is_decision_point, excluded.requires_authorization_role_id)",
    )
    .bind(s.meta.id)
    .bind(ctx.tenant_id.0)
    .bind(s.runbook_id)
    .bind(s.seq as i32)
    .bind(s.phase.as_str())
    .bind(&s.title)
    .bind(&s.instructions)
    .bind(s.owner_role_id)
    .bind(opt_minutes(s.expected_duration))
    .bind(&s.verification)
    .bind(&s.depends_on)
    .bind(s.is_decision_point)
    .bind(s.requires_authorization_role_id)
    .bind(s.provenance.origin.as_str())
    .bind(s.provenance.ai_suggestion_id)
    .bind(ctx.actor())
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

async fn load_steps(
    tx: &mut Tx,
    ctx: &TenantContext,
    runbook_ids: &[Uuid],
) -> AppResult<Vec<RunbookStep>> {
    let rows: Vec<StepRow> = sqlx::query_as(
        "select * from runbook_step where tenant_id = $1 and runbook_id = any($2) order by runbook_id, seq",
    )
    .bind(ctx.tenant_id.0)
    .bind(runbook_ids)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter().map(RunbookStep::try_from).collect()
}

pub struct PgRunbookRepository(pub Db);

#[async_trait]
impl RunbookRepository for PgRunbookRepository {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<Runbook>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<RunbookRow> = sqlx::query_as(
            "select * from runbook where tenant_id = $1 and microservice_id = $2 and ($3::uuid is null or scenario_id = $3)
             order by scenario_id, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(microservice_id)
        .bind(scenario_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(Runbook::try_from).collect()
    }

    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<Runbook>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<RunbookRow> = sqlx::query_as(
            "select r.* from runbook r join microservice m on m.id = r.microservice_id
             where r.tenant_id = $1 and m.service_id = $2 and ($3::uuid is null or r.scenario_id = $3)
             order by m.restore_order nulls last, m.name, r.created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .bind(scenario_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(Runbook::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Runbook>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RunbookRow> =
            sqlx::query_as("select * from runbook where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(Runbook::try_from).transpose()
    }

    async fn insert(
        &self,
        ctx: &TenantContext,
        r: &Runbook,
        steps: &[RunbookStep],
    ) -> AppResult<Runbook> {
        let mut tx = self.0.begin(ctx).await?;
        let row: RunbookRow = sqlx::query_as(
            "insert into runbook (id, tenant_id, microservice_id, scenario_id, strategy_id, title, description, origin,
                                  ai_suggestion_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10) returning *",
        )
        .bind(r.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(r.microservice_id)
        .bind(r.scenario_id)
        .bind(r.strategy_id)
        .bind(&r.title)
        .bind(&r.description)
        .bind(r.provenance.origin.as_str())
        .bind(r.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        for s in steps {
            upsert_step(&mut tx, ctx, s).await?;
        }
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, r: &Runbook) -> AppResult<Runbook> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RunbookRow> = sqlx::query_as(
            "update runbook set scenario_id = $4, strategy_id = $5, title = $6, description = $7,
                    version = version + 1, updated_at = now(), updated_by = $8
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(r.meta.id)
        .bind(r.meta.version)
        .bind(r.scenario_id)
        .bind(r.strategy_id)
        .bind(&r.title)
        .bind(&r.description)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from runbook where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn steps(
        &self,
        ctx: &TenantContext,
        runbook_ids: &[Uuid],
    ) -> AppResult<Vec<RunbookStep>> {
        let mut tx = self.0.begin(ctx).await?;
        let steps = load_steps(&mut tx, ctx, runbook_ids).await?;
        tx.commit().await?;
        Ok(steps)
    }

    async fn get_step(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RunbookStep>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<StepRow> =
            sqlx::query_as("select * from runbook_step where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(RunbookStep::try_from).transpose()
    }

    async fn save_steps(
        &self,
        ctx: &TenantContext,
        runbook_id: Uuid,
        steps: &[RunbookStep],
        deleted: &[Uuid],
    ) -> AppResult<Vec<RunbookStep>> {
        let mut tx = self.0.begin(ctx).await?;
        if !deleted.is_empty() {
            sqlx::query("delete from runbook_step where tenant_id = $1 and runbook_id = $2 and id = any($3)")
                .bind(ctx.tenant_id.0)
                .bind(runbook_id)
                .bind(deleted)
                .execute(&mut *tx)
                .await?;
        }
        for s in steps {
            upsert_step(&mut tx, ctx, s).await?;
        }
        let saved = load_steps(&mut tx, ctx, &[runbook_id]).await?;
        tx.commit().await?;
        Ok(saved)
    }
}
