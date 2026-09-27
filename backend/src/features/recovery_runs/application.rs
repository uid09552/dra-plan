use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::domain::{
    ClosingRecord, NewRunEvent, RecoveryRun, RecoveryRunRepository, RunEvent, RunEventType,
    RunMode, RunOutcome, RunStatus, RunStepState, RunStepStatus, Transition, blocked_by,
    display_status, instantiate, remaining_critical_path, transition,
};
use crate::features::action_items::domain::{ActionItem, ActionItemInput, ActionItemSource};
use crate::features::dr_tests::application::{DrTestUseCases, ResultInput, to_result};
use crate::features::dr_tests::domain::{DrTestOutcome, DrTestResult};
use crate::features::plans::application::PlanUseCases;
use crate::features::plans::domain::PlanStatus;
use crate::features::roles_comm::domain::CommunicationTrigger;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext, TenantId};

/// In-process fan-out of run events to live subscribers (SSE). Single-instance only; a
/// multi-instance deployment needs PostgreSQL LISTEN/NOTIFY or a message broker.
pub struct EventBus {
    sender: broadcast::Sender<(TenantId, RunEvent)>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
        }
    }

    pub fn publish(&self, tenant: TenantId, events: &[RunEvent]) {
        for e in events {
            // No subscribers is fine.
            let _ = self.sender.send((tenant, e.clone()));
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(TenantId, RunEvent)> {
        self.sender.subscribe()
    }
}

#[derive(Debug, Clone)]
pub struct RunClock {
    pub elapsed_minutes: i64,
    pub service_rto: Option<Minutes>,
    pub mtpd: Option<Minutes>,
    pub remaining_critical_path: Minutes,
    pub rto_at_risk: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RunProgress {
    pub total: usize,
    pub pending: usize,
    pub blocked: usize,
    pub in_progress: usize,
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub struct RunView {
    pub run: RecoveryRun,
    pub clock: RunClock,
    pub progress: RunProgress,
    pub next_status_update_due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct StepView {
    pub state: RunStepState,
    pub display_status: RunStepStatus,
    pub blocked_by: Vec<Uuid>,
}

pub struct Declaration {
    pub scenario_id: Uuid,
    pub mode: RunMode,
    pub plan_version_id: Option<Uuid>,
    pub dr_test_id: Option<Uuid>,
    pub microservice_ids: Option<Vec<Uuid>>,
    pub note: Option<String>,
}

pub struct Closing {
    pub outcome: RunOutcome,
    pub summary: Option<String>,
    pub achieved: Vec<ResultInput>,
    pub lessons_learned: Vec<String>,
}

pub struct RecoveryRunUseCases {
    repo: Arc<dyn RecoveryRunRepository>,
    plans: Arc<PlanUseCases>,
    dr_tests: Arc<DrTestUseCases>,
    bus: Arc<EventBus>,
}

impl RecoveryRunUseCases {
    pub fn new(
        repo: Arc<dyn RecoveryRunRepository>,
        plans: Arc<PlanUseCases>,
        dr_tests: Arc<DrTestUseCases>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            repo,
            plans,
            dr_tests,
            bus,
        }
    }

    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Declares DR (NIST Activation & Notification / BSI Alarmierung): pins the approved plan
    /// version and instantiates the runbook steps of the scenario.
    pub async fn declare(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        d: Declaration,
    ) -> AppResult<RunView> {
        self.plans.loader().require_service(ctx, service_id).await?;
        let version = match d.plan_version_id {
            Some(id) => self.plans.get(ctx, id).await?,
            None => self
                .plans
                .current_approved(ctx, service_id)
                .await?
                .ok_or_else(|| AppError::conflict("the service has no approved plan version"))?,
        };
        if version.service_id != Some(service_id) || version.status != PlanStatus::Approved {
            return Err(AppError::invalid(
                "PLAN_NOT_APPROVED",
                Some("/planVersionId"),
                "recovery runs require an approved plan version of this service",
            ));
        }
        if let Some(test_id) = d.dr_test_id {
            let test = self.dr_tests.get(ctx, test_id).await?;
            if d.mode != RunMode::Test || test.service_id != service_id {
                return Err(AppError::invalid(
                    "INVALID_VALUE",
                    Some("/drTestId"),
                    "a DR test can only be linked to a test-mode run of the same service",
                ));
            }
        }
        if !self
            .repo
            .list_by_service(ctx, service_id, Some(true), None)
            .await?
            .is_empty()
        {
            return Err(AppError::conflict(
                "the service already has an active recovery run",
            ));
        }
        let aggregate = self.plans.decode(&version)?;
        let microservice_ids = match d.microservice_ids {
            Some(ids) if !ids.is_empty() => ids,
            _ => aggregate
                .scenarios
                .iter()
                .find(|s| s.meta.id == d.scenario_id)
                .map(|s| aggregate.affected_components(s))
                .unwrap_or_default(),
        };
        let steps = instantiate(&aggregate, d.scenario_id, &microservice_ids)?;
        let status_update_frequency = aggregate
            .communication_rules
            .iter()
            .filter(|r| r.trigger == CommunicationTrigger::StatusUpdate)
            .filter_map(|r| r.frequency)
            .min();
        let run = RecoveryRun {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            service_id: Some(service_id),
            plan_version_id: version.id,
            scenario_id: Some(d.scenario_id),
            dr_test_id: d.dr_test_id,
            mode: d.mode,
            status: RunStatus::Declared,
            microservice_ids,
            declared_by: ctx.actor().to_owned(),
            declared_at: Utc::now(),
            recovered_at: None,
            closed_at: None,
            outcome: None,
            summary: None,
            note: crate::shared::kernel::non_blank(d.note),
            service_rto: aggregate.bia.as_ref().map(|b| b.service_rto),
            mtpd: aggregate.bia.as_ref().map(|b| b.mtpd),
            status_update_frequency,
        };
        let event = NewRunEvent {
            event_type: RunEventType::Declared,
            message: Some(format!(
                "DR declared ({}) for scenario `{}`",
                run.mode,
                scenario_title(&aggregate, d.scenario_id)
            )),
            payload: Some(serde_json_object(&[(
                "planVersion",
                version.plan_number.to_string(),
            )])),
        };
        let stored = self.repo.insert(ctx, &run, &steps, &event).await?;
        self.bus.publish(ctx.tenant_id, &[stored]);
        self.get(ctx, run.id).await
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        active: Option<bool>,
        mode: Option<RunMode>,
    ) -> AppResult<Vec<RunView>> {
        self.plans.loader().require_service(ctx, service_id).await?;
        let runs = self
            .repo
            .list_by_service(ctx, service_id, active, mode)
            .await?;
        let mut views = Vec::with_capacity(runs.len());
        for run in runs {
            views.push(self.view(ctx, run).await?);
        }
        Ok(views)
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<RunView> {
        let run = self.load(ctx, id).await?;
        self.view(ctx, run).await
    }

    pub async fn steps(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        microservice_id: Option<Uuid>,
        status: Option<RunStepStatus>,
    ) -> AppResult<Vec<StepView>> {
        self.load(ctx, id).await?;
        let all = self.repo.steps(ctx, id).await?;
        Ok(all
            .iter()
            .filter(|s| microservice_id.is_none_or(|m| s.microservice_id == m))
            .map(|s| StepView {
                display_status: display_status(s, &all),
                blocked_by: blocked_by(s, &all),
                state: s.clone(),
            })
            .filter(|v| status.is_none_or(|st| v.display_status == st))
            .collect())
    }

    pub async fn transition(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        step_id: Uuid,
        t: Transition,
    ) -> AppResult<StepView> {
        let mut run = self.load(ctx, id).await?;
        run.ensure_active()?;
        let mut all = self.repo.steps(ctx, id).await?;
        let to = t.to;
        let updated = transition(step_id, &mut all, t)?;
        let mut events = Vec::new();
        if run.status == RunStatus::Declared {
            run.status = RunStatus::InProgress;
            events.push(NewRunEvent {
                event_type: RunEventType::StatusChanged,
                message: Some("recovery in progress".into()),
                payload: None,
            });
        }
        events.push(NewRunEvent {
            event_type: RunEventType::StepTransition,
            message: Some(format!("{}: {to}", updated.title)),
            payload: Some(serde_json_object(&[
                ("stepId", step_id.to_string()),
                ("to", to.to_string()),
            ])),
        });
        let stored = self
            .repo
            .save(ctx, &run, std::slice::from_ref(&updated), &events)
            .await?;
        self.bus.publish(ctx.tenant_id, &stored);
        Ok(StepView {
            display_status: display_status(&updated, &all),
            blocked_by: blocked_by(&updated, &all),
            state: updated,
        })
    }

    pub async fn change_status(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        to: RunStatus,
        note: Option<String>,
    ) -> AppResult<RunView> {
        let mut run = self.load(ctx, id).await?;
        run.change_status(to)?;
        let event = NewRunEvent {
            event_type: RunEventType::StatusChanged,
            message: Some(note.unwrap_or_else(|| format!("status: {to}"))),
            payload: None,
        };
        let stored = self.repo.save(ctx, &run, &[], &[event]).await?;
        self.bus.publish(ctx.tenant_id, &stored);
        self.view(ctx, run).await
    }

    /// Closes the run: records achieved RTO/RPO, creates action items from lessons learned and
    /// (test mode) stores the results of the linked DR test.
    pub async fn close(&self, ctx: &TenantContext, id: Uuid, c: Closing) -> AppResult<RunView> {
        let mut run = self.load(ctx, id).await?;
        run.close(c.outcome, c.summary)?;
        let mut action_items = Vec::new();
        if let Some(service_id) = run.service_id {
            for lesson in c
                .lessons_learned
                .into_iter()
                .filter(|l| !l.trim().is_empty())
            {
                action_items.push(ActionItem::create(
                    ctx,
                    service_id,
                    ActionItemSource::Run,
                    Some(run.id),
                    ActionItemInput {
                        title: Some(lesson.chars().take(300).collect()),
                        description: Some(lesson),
                        ..Default::default()
                    },
                )?);
            }
        }
        let test = match run.dr_test_id {
            Some(test_id) => {
                let mut test = self.dr_tests.get(ctx, test_id).await?;
                let objectives = self.dr_tests.target_objectives(ctx, &test).await?;
                let results: Vec<DrTestResult> = c
                    .achieved
                    .into_iter()
                    .map(|i| to_result(i, &objectives, test.scenario_id))
                    .collect();
                test.executed_at = run.closed_at;
                test.recovery_run_id = Some(run.id);
                test.outcome = match c.outcome {
                    RunOutcome::Success => DrTestOutcome::Passed,
                    RunOutcome::Partial => DrTestOutcome::PartiallyPassed,
                    RunOutcome::Failed => DrTestOutcome::Failed,
                };
                Some((test, results))
            }
            None => None,
        };
        let event = NewRunEvent {
            event_type: RunEventType::Closed,
            message: Some(format!("run closed: {}", c.outcome)),
            payload: Some(serde_json_object(&[(
                "actionItems",
                action_items.len().to_string(),
            )])),
        };
        let stored = self
            .repo
            .close(ctx, &run, &event, &ClosingRecord { action_items, test })
            .await?;
        self.bus.publish(ctx.tenant_id, &[stored]);
        self.view(ctx, run).await
    }

    pub async fn events(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        after_id: Option<i64>,
        since: Option<DateTime<Utc>>,
    ) -> AppResult<Vec<RunEvent>> {
        self.load(ctx, id).await?;
        self.repo.events(ctx, id, after_id, since).await
    }

    pub async fn add_event(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        event: NewRunEvent,
    ) -> AppResult<RunEvent> {
        let run = self.load(ctx, id).await?;
        run.ensure_active()?;
        let mut stored = self.repo.save(ctx, &run, &[], &[event]).await?;
        self.bus.publish(ctx.tenant_id, &stored);
        stored
            .pop()
            .ok_or_else(|| AppError::internal("event not stored"))
    }

    async fn load(&self, ctx: &TenantContext, id: Uuid) -> AppResult<RecoveryRun> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery run"))
    }

    async fn view(&self, ctx: &TenantContext, run: RecoveryRun) -> AppResult<RunView> {
        let steps = self.repo.steps(ctx, run.id).await?;
        let mut progress = RunProgress {
            total: steps.len(),
            ..Default::default()
        };
        for s in &steps {
            match display_status(s, &steps) {
                RunStepStatus::Pending => progress.pending += 1,
                RunStepStatus::Blocked => progress.blocked += 1,
                RunStepStatus::InProgress => progress.in_progress += 1,
                RunStepStatus::Done => progress.done += 1,
                RunStepStatus::Skipped => progress.skipped += 1,
                RunStepStatus::Failed => progress.failed += 1,
            }
        }
        let now = Utc::now();
        let elapsed = run.elapsed_minutes(now);
        let remaining = remaining_critical_path(&steps);
        let rto_at_risk = run.is_active()
            && run
                .service_rto
                .is_some_and(|rto| elapsed + i64::from(remaining.get()) > i64::from(rto.get()));
        let next_status_update_due_at = match (run.is_active(), run.status_update_frequency) {
            (true, Some(freq)) => {
                let last = self
                    .repo
                    .last_communication_at(ctx, run.id)
                    .await?
                    .unwrap_or(run.declared_at);
                Some(last + Duration::minutes(i64::from(freq.get())))
            }
            _ => None,
        };
        Ok(RunView {
            clock: RunClock {
                elapsed_minutes: elapsed,
                service_rto: run.service_rto,
                mtpd: run.mtpd,
                remaining_critical_path: remaining,
                rto_at_risk,
            },
            progress,
            next_status_update_due_at,
            run,
        })
    }
}

fn scenario_title(a: &crate::features::plans::domain::PlanAggregate, id: Uuid) -> String {
    a.scenarios
        .iter()
        .find(|s| s.meta.id == id)
        .map(|s| s.title.clone())
        .unwrap_or_default()
}

/// Builds a small JSON object of string values (event payloads).
fn serde_json_object(pairs: &[(&str, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String(v.clone())))
        .collect();
    serde_json::Value::Object(map).to_string()
}
