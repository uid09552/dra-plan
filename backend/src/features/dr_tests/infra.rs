use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{DrTest, DrTestRepository, DrTestResult};
use crate::shared::infra::{
    Db, MetaRow, Tx, db_enum, db_minutes, opt_minutes, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct DrTestRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    service_id: Uuid,
    #[sqlx(rename = "type")]
    test_type: String,
    scenario_id: Uuid,
    plan_version_id: Option<Uuid>,
    planned_at: DateTime<Utc>,
    executed_at: Option<DateTime<Utc>>,
    participant_ids: Vec<Uuid>,
    outcome: String,
    report: Option<String>,
    recovery_run_id: Option<Uuid>,
}

impl TryFrom<DrTestRow> for DrTest {
    type Error = AppError;

    fn try_from(r: DrTestRow) -> AppResult<Self> {
        Ok(DrTest {
            meta: r.meta.into(),
            service_id: r.service_id,
            test_type: db_enum(&r.test_type)?,
            scenario_id: r.scenario_id,
            plan_version_id: r.plan_version_id,
            planned_at: r.planned_at,
            executed_at: r.executed_at,
            participant_ids: r.participant_ids,
            outcome: db_enum(&r.outcome)?,
            report: r.report,
            recovery_run_id: r.recovery_run_id,
        })
    }
}

#[derive(FromRow)]
struct ResultRow {
    dr_test_id: Uuid,
    microservice_id: Uuid,
    target_rto_minutes: Option<i32>,
    target_rpo_minutes: Option<i32>,
    achieved_rto_minutes: Option<i32>,
    achieved_rpo_minutes: Option<i32>,
    notes: Option<String>,
}

/// Replaces the results of a test inside an existing transaction (also used by recovery runs).
pub async fn replace_results_in(
    tx: &mut Tx,
    ctx: &TenantContext,
    test_id: Uuid,
    results: &[DrTestResult],
) -> AppResult<()> {
    sqlx::query("delete from dr_test_result where tenant_id = $1 and dr_test_id = $2")
        .bind(ctx.tenant_id.0)
        .bind(test_id)
        .execute(&mut **tx)
        .await?;
    for r in results {
        sqlx::query(
            "insert into dr_test_result (tenant_id, dr_test_id, microservice_id, target_rto_minutes, target_rpo_minutes,
                                         achieved_rto_minutes, achieved_rpo_minutes, notes)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(ctx.tenant_id.0)
        .bind(test_id)
        .bind(r.microservice_id)
        .bind(opt_minutes(r.target_rto))
        .bind(opt_minutes(r.target_rpo))
        .bind(opt_minutes(r.achieved_rto))
        .bind(opt_minutes(r.achieved_rpo))
        .bind(&r.notes)
        .execute(&mut **tx)
        .await
        .map_err(write_err)?;
    }
    Ok(())
}

pub struct PgDrTestRepository(pub Db);

#[async_trait]
impl DrTestRepository for PgDrTestRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<DrTest>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<DrTestRow> =
            sqlx::query_as("select * from dr_test where tenant_id = $1 and service_id = $2 order by planned_at desc")
                .bind(ctx.tenant_id.0)
                .bind(service_id)
                .fetch_all(&mut *tx)
                .await?;
        tx.commit().await?;
        rows.into_iter().map(DrTest::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<DrTest>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DrTestRow> =
            sqlx::query_as("select * from dr_test where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(DrTest::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, t: &DrTest) -> AppResult<DrTest> {
        let mut tx = self.0.begin(ctx).await?;
        let row: DrTestRow = sqlx::query_as(
            "insert into dr_test (id, tenant_id, service_id, type, scenario_id, plan_version_id, planned_at, executed_at,
                                  participant_ids, outcome, report, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12) returning *",
        )
        .bind(t.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(t.service_id)
        .bind(t.test_type.as_str())
        .bind(t.scenario_id)
        .bind(t.plan_version_id)
        .bind(t.planned_at)
        .bind(t.executed_at)
        .bind(&t.participant_ids)
        .bind(t.outcome.as_str())
        .bind(&t.report)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, t: &DrTest) -> AppResult<DrTest> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DrTestRow> = sqlx::query_as(
            "update dr_test set type = $4, scenario_id = $5, plan_version_id = $6, planned_at = $7, executed_at = $8,
                    participant_ids = $9, outcome = $10, report = $11, recovery_run_id = $12,
                    version = version + 1, updated_at = now(), updated_by = $13
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(t.meta.id)
        .bind(t.meta.version)
        .bind(t.test_type.as_str())
        .bind(t.scenario_id)
        .bind(t.plan_version_id)
        .bind(t.planned_at)
        .bind(t.executed_at)
        .bind(&t.participant_ids)
        .bind(t.outcome.as_str())
        .bind(&t.report)
        .bind(t.recovery_run_id)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from dr_test where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn results(
        &self,
        ctx: &TenantContext,
        test_ids: &[Uuid],
    ) -> AppResult<Vec<(Uuid, DrTestResult)>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ResultRow> = sqlx::query_as(
            "select * from dr_test_result where tenant_id = $1 and dr_test_id = any($2) order by dr_test_id, microservice_id",
        )
        .bind(ctx.tenant_id.0)
        .bind(test_ids)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.dr_test_id,
                    DrTestResult {
                        microservice_id: r.microservice_id,
                        target_rto: db_minutes(r.target_rto_minutes)?,
                        target_rpo: db_minutes(r.target_rpo_minutes)?,
                        achieved_rto: db_minutes(r.achieved_rto_minutes)?,
                        achieved_rpo: db_minutes(r.achieved_rpo_minutes)?,
                        notes: r.notes,
                    },
                ))
            })
            .collect()
    }

    async fn replace_results(
        &self,
        ctx: &TenantContext,
        test_id: Uuid,
        results: &[DrTestResult],
    ) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        replace_results_in(&mut tx, ctx, test_id, results).await?;
        tx.commit().await?;
        Ok(())
    }
}
