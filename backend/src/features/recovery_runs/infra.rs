use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{
    ClosingRecord, NewRunEvent, RecoveryRun, RecoveryRunRepository, RunEvent, RunMode, RunStepState,
};
use crate::features::action_items::infra::insert_action_item;
use crate::features::dr_tests::infra::replace_results_in;
use crate::shared::infra::{Db, Tx, db_enum, db_enum_opt, db_minutes, opt_minutes, write_err};
use crate::shared::kernel::{AppError, AppResult, TenantContext, TenantId};

#[derive(FromRow)]
struct RunRow {
    id: Uuid,
    tenant_id: Uuid,
    service_id: Option<Uuid>,
    plan_version_id: Uuid,
    scenario_id: Option<Uuid>,
    dr_test_id: Option<Uuid>,
    mode: String,
    status: String,
    microservice_ids: Vec<Uuid>,
    declared_by: String,
    declared_at: DateTime<Utc>,
    recovered_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
    outcome: Option<String>,
    summary: Option<String>,
    note: Option<String>,
    service_rto_minutes: Option<i32>,
    mtpd_minutes: Option<i32>,
    status_update_frequency_minutes: Option<i32>,
}

impl TryFrom<RunRow> for RecoveryRun {
    type Error = AppError;

    fn try_from(r: RunRow) -> AppResult<Self> {
        Ok(RecoveryRun {
            id: r.id,
            tenant_id: TenantId(r.tenant_id),
            service_id: r.service_id,
            plan_version_id: r.plan_version_id,
            scenario_id: r.scenario_id,
            dr_test_id: r.dr_test_id,
            mode: db_enum(&r.mode)?,
            status: db_enum(&r.status)?,
            microservice_ids: r.microservice_ids,
            declared_by: r.declared_by,
            declared_at: r.declared_at,
            recovered_at: r.recovered_at,
            closed_at: r.closed_at,
            outcome: db_enum_opt(r.outcome.as_deref())?,
            summary: r.summary,
            note: r.note,
            service_rto: db_minutes(r.service_rto_minutes)?,
            mtpd: db_minutes(r.mtpd_minutes)?,
            status_update_frequency: db_minutes(r.status_update_frequency_minutes)?,
        })
    }
}

#[derive(FromRow)]
struct StepRow {
    runbook_step_id: Uuid,
    runbook_id: Uuid,
    microservice_id: Uuid,
    ord: i32,
    seq: i32,
    phase: String,
    title: String,
    instructions: Option<String>,
    verification: Option<String>,
    owner_role_id: Option<Uuid>,
    expected_duration_minutes: Option<i32>,
    depends_on: Vec<Uuid>,
    is_decision_point: bool,
    requires_authorization_role_id: Option<Uuid>,
    status: String,
    assignee_person_id: Option<Uuid>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    note: Option<String>,
}

impl TryFrom<StepRow> for RunStepState {
    type Error = AppError;

    fn try_from(r: StepRow) -> AppResult<Self> {
        Ok(RunStepState {
            runbook_step_id: r.runbook_step_id,
            runbook_id: r.runbook_id,
            microservice_id: r.microservice_id,
            ord: r.ord.max(0) as u32,
            seq: r.seq.max(0) as u32,
            phase: db_enum(&r.phase)?,
            title: r.title,
            instructions: r.instructions,
            verification: r.verification,
            owner_role_id: r.owner_role_id,
            expected_duration: db_minutes(r.expected_duration_minutes)?,
            depends_on: r.depends_on,
            is_decision_point: r.is_decision_point,
            requires_authorization_role_id: r.requires_authorization_role_id,
            status: db_enum(&r.status)?,
            assignee_person_id: r.assignee_person_id,
            started_at: r.started_at,
            finished_at: r.finished_at,
            note: r.note,
        })
    }
}

#[derive(FromRow)]
struct EventRow {
    id: i64,
    run_id: Uuid,
    at: DateTime<Utc>,
    actor: String,
    #[sqlx(rename = "type")]
    event_type: String,
    message: Option<String>,
    payload: Option<String>,
}

impl TryFrom<EventRow> for RunEvent {
    type Error = AppError;

    fn try_from(r: EventRow) -> AppResult<Self> {
        Ok(RunEvent {
            id: r.id,
            run_id: r.run_id,
            at: r.at,
            actor: r.actor,
            event_type: db_enum(&r.event_type)?,
            message: r.message,
            payload: r.payload,
        })
    }
}

async fn append_event(
    tx: &mut Tx,
    ctx: &TenantContext,
    run_id: Uuid,
    e: &NewRunEvent,
) -> AppResult<RunEvent> {
    let row: EventRow = sqlx::query_as(
        "insert into run_event (tenant_id, run_id, actor, type, message, payload)
         values ($1, $2, $3, $4, $5, $6::jsonb)
         returning id, run_id, at, actor, type, message, payload::text as payload",
    )
    .bind(ctx.tenant_id.0)
    .bind(run_id)
    .bind(ctx.actor())
    .bind(e.event_type.as_str())
    .bind(&e.message)
    .bind(&e.payload)
    .fetch_one(&mut **tx)
    .await
    .map_err(write_err)?;
    row.try_into()
}

async fn update_run(tx: &mut Tx, ctx: &TenantContext, r: &RecoveryRun) -> AppResult<()> {
    sqlx::query(
        "update recovery_run set status = $3, recovered_at = $4, closed_at = $5, outcome = $6, summary = $7
         where tenant_id = $1 and id = $2",
    )
    .bind(ctx.tenant_id.0)
    .bind(r.id)
    .bind(r.status.as_str())
    .bind(r.recovered_at)
    .bind(r.closed_at)
    .bind(r.outcome.map(|o| o.as_str()))
    .bind(&r.summary)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_step(
    tx: &mut Tx,
    ctx: &TenantContext,
    run_id: Uuid,
    s: &RunStepState,
) -> AppResult<()> {
    sqlx::query(
        "insert into run_step_state (tenant_id, run_id, runbook_step_id, runbook_id, microservice_id, ord, seq, phase, title,
                instructions, verification, owner_role_id, expected_duration_minutes, depends_on, is_decision_point,
                requires_authorization_role_id, status, assignee_person_id, started_at, finished_at, note)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
         on conflict (run_id, runbook_step_id) do update set status = excluded.status,
            assignee_person_id = excluded.assignee_person_id, started_at = excluded.started_at,
            finished_at = excluded.finished_at, note = excluded.note",
    )
    .bind(ctx.tenant_id.0)
    .bind(run_id)
    .bind(s.runbook_step_id)
    .bind(s.runbook_id)
    .bind(s.microservice_id)
    .bind(s.ord as i32)
    .bind(s.seq as i32)
    .bind(s.phase.as_str())
    .bind(&s.title)
    .bind(&s.instructions)
    .bind(&s.verification)
    .bind(s.owner_role_id)
    .bind(opt_minutes(s.expected_duration))
    .bind(&s.depends_on)
    .bind(s.is_decision_point)
    .bind(s.requires_authorization_role_id)
    .bind(s.status.as_str())
    .bind(s.assignee_person_id)
    .bind(s.started_at)
    .bind(s.finished_at)
    .bind(&s.note)
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

pub struct PgRecoveryRunRepository(pub Db);

#[async_trait]
impl RecoveryRunRepository for PgRecoveryRunRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        active: Option<bool>,
        mode: Option<RunMode>,
    ) -> AppResult<Vec<RecoveryRun>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<RunRow> = sqlx::query_as(
            "select * from recovery_run where tenant_id = $1 and service_id = $2
               and ($3::boolean is null or $3 = (status in ('declared', 'in_progress', 'recovered', 'reconstituting')))
               and ($4::text is null or mode = $4)
             order by declared_at desc",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .bind(active)
        .bind(mode.map(|m| m.as_str()))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(RecoveryRun::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryRun>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RunRow> =
            sqlx::query_as("select * from recovery_run where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(RecoveryRun::try_from).transpose()
    }

    async fn insert(
        &self,
        ctx: &TenantContext,
        r: &RecoveryRun,
        steps: &[RunStepState],
        event: &NewRunEvent,
    ) -> AppResult<RunEvent> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query(
            "insert into recovery_run (id, tenant_id, service_id, plan_version_id, scenario_id, dr_test_id, mode, status,
                    microservice_ids, declared_by, declared_at, note, service_rto_minutes, mtpd_minutes,
                    status_update_frequency_minutes)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(r.id)
        .bind(ctx.tenant_id.0)
        .bind(r.service_id)
        .bind(r.plan_version_id)
        .bind(r.scenario_id)
        .bind(r.dr_test_id)
        .bind(r.mode.as_str())
        .bind(r.status.as_str())
        .bind(&r.microservice_ids)
        .bind(&r.declared_by)
        .bind(r.declared_at)
        .bind(&r.note)
        .bind(opt_minutes(r.service_rto))
        .bind(opt_minutes(r.mtpd))
        .bind(opt_minutes(r.status_update_frequency))
        .execute(&mut *tx)
        .await
        .map_err(write_err)?;
        for s in steps {
            upsert_step(&mut tx, ctx, r.id, s).await?;
        }
        let stored = append_event(&mut tx, ctx, r.id, event).await?;
        tx.commit().await?;
        Ok(stored)
    }

    async fn steps(&self, ctx: &TenantContext, run_id: Uuid) -> AppResult<Vec<RunStepState>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<StepRow> = sqlx::query_as(
            "select * from run_step_state where tenant_id = $1 and run_id = $2 order by ord",
        )
        .bind(ctx.tenant_id.0)
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(RunStepState::try_from).collect()
    }

    async fn save(
        &self,
        ctx: &TenantContext,
        r: &RecoveryRun,
        steps: &[RunStepState],
        events: &[NewRunEvent],
    ) -> AppResult<Vec<RunEvent>> {
        let mut tx = self.0.begin(ctx).await?;
        update_run(&mut tx, ctx, r).await?;
        for s in steps {
            upsert_step(&mut tx, ctx, r.id, s).await?;
        }
        let mut stored = Vec::with_capacity(events.len());
        for e in events {
            stored.push(append_event(&mut tx, ctx, r.id, e).await?);
        }
        tx.commit().await?;
        Ok(stored)
    }

    async fn events(
        &self,
        ctx: &TenantContext,
        run_id: Uuid,
        after_id: Option<i64>,
        since: Option<DateTime<Utc>>,
    ) -> AppResult<Vec<RunEvent>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<EventRow> = sqlx::query_as(
            "select id, run_id, at, actor, type, message, payload::text as payload from run_event
             where tenant_id = $1 and run_id = $2 and ($3::bigint is null or id > $3) and ($4::timestamptz is null or at >= $4)
             order by id",
        )
        .bind(ctx.tenant_id.0)
        .bind(run_id)
        .bind(after_id)
        .bind(since)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(RunEvent::try_from).collect()
    }

    async fn close(
        &self,
        ctx: &TenantContext,
        r: &RecoveryRun,
        event: &NewRunEvent,
        record: &ClosingRecord,
    ) -> AppResult<RunEvent> {
        let mut tx = self.0.begin(ctx).await?;
        update_run(&mut tx, ctx, r).await?;
        for item in &record.action_items {
            insert_action_item(&mut tx, ctx, item).await?;
        }
        if let Some((test, results)) = &record.test {
            replace_results_in(&mut tx, ctx, test.meta.id, results).await?;
            sqlx::query(
                "update dr_test set executed_at = $3, outcome = $4, recovery_run_id = $5,
                        version = version + 1, updated_at = now(), updated_by = $6
                 where tenant_id = $1 and id = $2",
            )
            .bind(ctx.tenant_id.0)
            .bind(test.meta.id)
            .bind(test.executed_at)
            .bind(test.outcome.as_str())
            .bind(test.recovery_run_id)
            .bind(ctx.actor())
            .execute(&mut *tx)
            .await?;
        }
        let stored = append_event(&mut tx, ctx, r.id, event).await?;
        tx.commit().await?;
        Ok(stored)
    }

    async fn last_communication_at(
        &self,
        ctx: &TenantContext,
        run_id: Uuid,
    ) -> AppResult<Option<DateTime<Utc>>> {
        let mut tx = self.0.begin(ctx).await?;
        let at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "select max(at) from run_event where tenant_id = $1 and run_id = $2 and type = 'communication_sent'",
        )
        .bind(ctx.tenant_id.0)
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(at)
    }
}
