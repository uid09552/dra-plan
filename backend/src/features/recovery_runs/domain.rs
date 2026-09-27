//! Recovery runs: executing an approved, pinned plan version during a disaster or a test.
//! Phases follow NIST SP 800-34 (Activation → Recovery → Reconstitution) and BSI 200-4
//! (Alarmierung → Notbetrieb/Wiederanlauf → Wiederherstellung).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::features::action_items::domain::ActionItem;
use crate::features::dr_tests::domain::{DrTest, DrTestResult};
use crate::features::plans::domain::PlanAggregate;
use crate::features::runbooks::domain::{RunbookPhase, effective_dependencies};
use crate::features::scenarios::domain::ScenarioStatus;
use crate::shared::kernel::{
    AppError, AppResult, Issues, Minutes, TenantContext, TenantId, non_blank, str_enum,
};

str_enum! {
    pub enum RunMode { Real = "real", Test = "test" }
}

str_enum! {
    pub enum RunStatus {
        Declared = "declared",
        InProgress = "in_progress",
        Recovered = "recovered",
        Reconstituting = "reconstituting",
        Closed = "closed",
        Aborted = "aborted",
    }
}

str_enum! {
    pub enum RunStepStatus {
        Pending = "pending",
        Blocked = "blocked",
        InProgress = "in_progress",
        Done = "done",
        Skipped = "skipped",
        Failed = "failed",
    }
}

str_enum! {
    pub enum RunOutcome { Success = "success", Partial = "partial", Failed = "failed" }
}

str_enum! {
    pub enum RunEventType {
        Declared = "declared",
        StepTransition = "step_transition",
        StatusChanged = "status_changed",
        Note = "note",
        Decision = "decision",
        CommunicationSent = "communication_sent",
        AiSuggestionAccepted = "ai_suggestion_accepted",
        Closed = "closed",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryRun {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub service_id: Option<Uuid>,
    pub plan_version_id: Uuid,
    pub scenario_id: Option<Uuid>,
    pub dr_test_id: Option<Uuid>,
    pub mode: RunMode,
    pub status: RunStatus,
    pub microservice_ids: Vec<Uuid>,
    pub declared_by: String,
    pub declared_at: DateTime<Utc>,
    pub recovered_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub outcome: Option<RunOutcome>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub service_rto: Option<Minutes>,
    pub mtpd: Option<Minutes>,
    pub status_update_frequency: Option<Minutes>,
}

impl RecoveryRun {
    pub fn is_active(&self) -> bool {
        !matches!(self.status, RunStatus::Closed | RunStatus::Aborted)
    }

    pub fn ensure_active(&self) -> AppResult<()> {
        if self.is_active() {
            Ok(())
        } else {
            Err(AppError::conflict("the recovery run is closed"))
        }
    }

    /// Phase transitions: declared/in_progress → recovered → reconstituting; abort from any active state.
    pub fn change_status(&mut self, to: RunStatus) -> AppResult<()> {
        self.ensure_active()?;
        let allowed = match to {
            RunStatus::Recovered => {
                matches!(self.status, RunStatus::Declared | RunStatus::InProgress)
            }
            RunStatus::Reconstituting => self.status == RunStatus::Recovered,
            RunStatus::Aborted => true,
            _ => false,
        };
        if !allowed {
            return Err(AppError::conflict(format!(
                "cannot change the run status from {} to {to}",
                self.status
            )));
        }
        if to == RunStatus::Recovered {
            self.recovered_at = Some(Utc::now());
        }
        if to == RunStatus::Aborted {
            self.closed_at = Some(Utc::now());
        }
        self.status = to;
        Ok(())
    }

    pub fn close(&mut self, outcome: RunOutcome, summary: Option<String>) -> AppResult<()> {
        if !matches!(
            self.status,
            RunStatus::Recovered | RunStatus::Reconstituting
        ) {
            return Err(AppError::conflict(
                "only recovered or reconstituting runs can be closed",
            ));
        }
        self.status = RunStatus::Closed;
        self.outcome = Some(outcome);
        self.summary = non_blank(summary);
        self.closed_at = Some(Utc::now());
        Ok(())
    }

    pub fn elapsed_minutes(&self, now: DateTime<Utc>) -> i64 {
        let end = self
            .closed_at
            .or(if self.status == RunStatus::Recovered {
                self.recovered_at
            } else {
                None
            })
            .unwrap_or(now);
        (end - self.declared_at).num_minutes().max(0)
    }
}

/// A runbook step copied from the pinned snapshot, with its execution state.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStepState {
    pub runbook_step_id: Uuid,
    pub runbook_id: Uuid,
    pub microservice_id: Uuid,
    /// Global execution order within the run.
    pub ord: u32,
    pub seq: u32,
    pub phase: RunbookPhase,
    pub title: String,
    pub instructions: Option<String>,
    pub verification: Option<String>,
    pub owner_role_id: Option<Uuid>,
    pub expected_duration: Option<Minutes>,
    /// Effective predecessors (explicit or the previous step of the runbook).
    pub depends_on: Vec<Uuid>,
    pub is_decision_point: bool,
    pub requires_authorization_role_id: Option<Uuid>,
    pub status: RunStepStatus,
    pub assignee_person_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}

impl RunStepState {
    fn finished(&self) -> bool {
        matches!(self.status, RunStepStatus::Done | RunStepStatus::Skipped)
    }
}

/// Instantiates the step states of a run from the pinned snapshot: runbooks of the scenario for
/// the selected microservices, in restore order.
pub fn instantiate(
    aggregate: &PlanAggregate,
    scenario_id: Uuid,
    microservice_ids: &[Uuid],
) -> AppResult<Vec<RunStepState>> {
    let scenario = aggregate
        .scenarios
        .iter()
        .find(|s| s.meta.id == scenario_id)
        .ok_or_else(|| {
            AppError::invalid(
                "REFERENCE_NOT_FOUND",
                Some("/scenarioId"),
                "the scenario is not part of the plan version",
            )
        })?;
    if scenario.status != ScenarioStatus::Selected {
        return Err(AppError::invalid(
            "SCENARIO_NOT_SELECTED",
            Some("/scenarioId"),
            "the scenario was not selected in the plan version",
        ));
    }
    let mut states = Vec::new();
    for bundle in &aggregate.microservices {
        let mid = bundle.microservice.meta.id;
        if !microservice_ids.contains(&mid) {
            continue;
        }
        for rb in bundle
            .runbooks
            .iter()
            .filter(|r| r.runbook.scenario_id == scenario_id)
        {
            let deps = effective_dependencies(&rb.steps);
            let mut steps: Vec<_> = rb.steps.iter().collect();
            steps.sort_by_key(|s| s.seq);
            for s in steps {
                states.push(RunStepState {
                    runbook_step_id: s.meta.id,
                    runbook_id: rb.runbook.meta.id,
                    microservice_id: mid,
                    ord: states.len() as u32 + 1,
                    seq: s.seq,
                    phase: s.phase,
                    title: s.title.clone(),
                    instructions: s.instructions.clone(),
                    verification: s.verification.clone(),
                    owner_role_id: s.owner_role_id,
                    expected_duration: s.expected_duration,
                    depends_on: deps.get(&s.meta.id).cloned().unwrap_or_default(),
                    is_decision_point: s.is_decision_point,
                    requires_authorization_role_id: s.requires_authorization_role_id,
                    status: RunStepStatus::Pending,
                    assignee_person_id: None,
                    started_at: None,
                    finished_at: None,
                    note: None,
                });
            }
        }
    }
    if states.is_empty() {
        return Err(AppError::invalid(
            "NO_RUNBOOK_STEPS",
            Some("/scenarioId"),
            "the plan version has no runbook steps for this scenario and these microservices",
        ));
    }
    Ok(states)
}

/// Unfinished predecessors of a step.
pub fn blocked_by(step: &RunStepState, all: &[RunStepState]) -> Vec<Uuid> {
    let finished: HashMap<Uuid, bool> = all
        .iter()
        .map(|s| (s.runbook_step_id, s.finished()))
        .collect();
    step.depends_on
        .iter()
        .filter(|d| !finished.get(d).copied().unwrap_or(true))
        .copied()
        .collect()
}

/// Status as shown to users: pending steps with unfinished predecessors are `blocked`.
pub fn display_status(step: &RunStepState, all: &[RunStepState]) -> RunStepStatus {
    if step.status == RunStepStatus::Pending && !blocked_by(step, all).is_empty() {
        RunStepStatus::Blocked
    } else {
        step.status
    }
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub to: RunStepStatus,
    pub note: Option<String>,
    pub verification_passed: Option<bool>,
    pub assignee_person_id: Option<Uuid>,
}

/// Applies a step transition, enforcing dependencies, verification and notes.
pub fn transition(
    step_id: Uuid,
    all: &mut [RunStepState],
    t: Transition,
) -> AppResult<RunStepState> {
    let blockers = {
        let step = all
            .iter()
            .find(|s| s.runbook_step_id == step_id)
            .ok_or(AppError::NotFound("run step"))?;
        blocked_by(step, all)
    };
    let step = all
        .iter_mut()
        .find(|s| s.runbook_step_id == step_id)
        .ok_or(AppError::NotFound("run step"))?;
    let note = non_blank(t.note);
    let mut issues = Issues::new();
    match t.to {
        RunStepStatus::InProgress => {
            if !matches!(step.status, RunStepStatus::Pending | RunStepStatus::Failed) {
                return Err(AppError::conflict(format!(
                    "cannot start a step that is {}",
                    step.status
                )));
            }
        }
        RunStepStatus::Done => {
            if step.finished() {
                return Err(AppError::conflict("the step is already finished"));
            }
            issues.check(
                t.verification_passed == Some(true),
                "VERIFICATION_REQUIRED",
                "/verificationPassed",
                "completing a step requires a passed verification",
            );
        }
        RunStepStatus::Skipped | RunStepStatus::Failed => {
            if step.finished() {
                return Err(AppError::conflict("the step is already finished"));
            }
            issues.check(
                note.is_some(),
                "REQUIRED",
                "/note",
                "a note is required to skip or fail a step",
            );
        }
        RunStepStatus::Pending | RunStepStatus::Blocked => {
            return Err(AppError::invalid(
                "INVALID_VALUE",
                Some("/to"),
                "steps can only be moved to in_progress, done, skipped or failed",
            ));
        }
    }
    issues.into_result()?;
    if t.to != RunStepStatus::Skipped && !blockers.is_empty() {
        return Err(AppError::conflict(
            "the step depends on steps that are not finished yet",
        ));
    }
    let now = Utc::now();
    if t.to == RunStepStatus::InProgress || step.started_at.is_none() {
        step.started_at = Some(now);
    }
    step.finished_at = matches!(
        t.to,
        RunStepStatus::Done | RunStepStatus::Skipped | RunStepStatus::Failed
    )
    .then_some(now);
    step.status = t.to;
    if note.is_some() {
        step.note = note;
    }
    if t.assignee_person_id.is_some() {
        step.assignee_person_id = t.assignee_person_id;
    }
    Ok(step.clone())
}

/// Longest remaining chain of expected durations (finished steps count as 0).
pub fn remaining_critical_path(all: &[RunStepState]) -> Minutes {
    let mut ordered: Vec<&RunStepState> = all.iter().collect();
    ordered.sort_by_key(|s| s.ord);
    let mut finish: HashMap<Uuid, u64> = HashMap::new();
    for s in ordered {
        let start = s
            .depends_on
            .iter()
            .filter_map(|d| finish.get(d))
            .copied()
            .max()
            .unwrap_or(0);
        let own = if s.finished() {
            0
        } else {
            s.expected_duration.map_or(0, |m| u64::from(m.get()))
        };
        finish.insert(s.runbook_step_id, start + own);
    }
    Minutes::saturating(finish.values().copied().max().unwrap_or(0))
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunEvent {
    pub id: i64,
    pub run_id: Uuid,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub event_type: RunEventType,
    pub message: Option<String>,
    /// JSON object, stored as-is.
    pub payload: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewRunEvent {
    pub event_type: RunEventType,
    pub message: Option<String>,
    pub payload: Option<String>,
}

/// Everything persisted atomically when a run is closed.
pub struct ClosingRecord {
    pub action_items: Vec<ActionItem>,
    pub test: Option<(DrTest, Vec<DrTestResult>)>,
}

#[async_trait]
pub trait RecoveryRunRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        active: Option<bool>,
        mode: Option<RunMode>,
    ) -> AppResult<Vec<RecoveryRun>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryRun>>;
    async fn insert(
        &self,
        ctx: &TenantContext,
        run: &RecoveryRun,
        steps: &[RunStepState],
        event: &NewRunEvent,
    ) -> AppResult<RunEvent>;
    async fn steps(&self, ctx: &TenantContext, run_id: Uuid) -> AppResult<Vec<RunStepState>>;
    /// Persists run + changed steps + events atomically; returns the stored events.
    async fn save(
        &self,
        ctx: &TenantContext,
        run: &RecoveryRun,
        steps: &[RunStepState],
        events: &[NewRunEvent],
    ) -> AppResult<Vec<RunEvent>>;
    async fn events(
        &self,
        ctx: &TenantContext,
        run_id: Uuid,
        after_id: Option<i64>,
        since: Option<DateTime<Utc>>,
    ) -> AppResult<Vec<RunEvent>>;
    /// Closes the run atomically with its action items and DR test results.
    async fn close(
        &self,
        ctx: &TenantContext,
        run: &RecoveryRun,
        event: &NewRunEvent,
        record: &ClosingRecord,
    ) -> AppResult<RunEvent>;
    async fn last_communication_at(
        &self,
        ctx: &TenantContext,
        run_id: Uuid,
    ) -> AppResult<Option<DateTime<Utc>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(ord: u32, minutes: u32, depends_on: Vec<Uuid>) -> RunStepState {
        RunStepState {
            runbook_step_id: Uuid::new_v4(),
            runbook_id: Uuid::nil(),
            microservice_id: Uuid::nil(),
            ord,
            seq: ord,
            phase: RunbookPhase::Recovery,
            title: format!("s{ord}"),
            instructions: None,
            verification: None,
            owner_role_id: None,
            expected_duration: Some(Minutes::new(minutes).expect("valid")),
            depends_on,
            is_decision_point: false,
            requires_authorization_role_id: None,
            status: RunStepStatus::Pending,
            assignee_person_id: None,
            started_at: None,
            finished_at: None,
            note: None,
        }
    }

    fn t(to: RunStepStatus) -> Transition {
        Transition {
            to,
            note: None,
            verification_passed: None,
            assignee_person_id: None,
        }
    }

    #[test]
    fn enforces_dependencies_and_verification() {
        let a = state(1, 10, vec![]);
        let b = state(2, 20, vec![a.runbook_step_id]);
        let (a_id, b_id) = (a.runbook_step_id, b.runbook_step_id);
        let mut all = vec![a, b];
        assert_eq!(display_status(&all[1], &all), RunStepStatus::Blocked);
        assert!(transition(b_id, &mut all, t(RunStepStatus::InProgress)).is_err());
        transition(a_id, &mut all, t(RunStepStatus::InProgress)).expect("start");
        assert!(
            transition(a_id, &mut all, t(RunStepStatus::Done)).is_err(),
            "verification required"
        );
        transition(
            a_id,
            &mut all,
            Transition {
                verification_passed: Some(true),
                ..t(RunStepStatus::Done)
            },
        )
        .expect("done");
        assert_eq!(remaining_critical_path(&all).get(), 20);
        assert!(
            transition(b_id, &mut all, t(RunStepStatus::Skipped)).is_err(),
            "note required"
        );
        transition(
            b_id,
            &mut all,
            Transition {
                note: Some("not needed".into()),
                ..t(RunStepStatus::Skipped)
            },
        )
        .expect("skip");
        assert_eq!(remaining_critical_path(&all).get(), 0);
    }

    #[test]
    fn run_status_follows_phases() {
        let mut run = RecoveryRun {
            id: Uuid::nil(),
            tenant_id: TenantId(Uuid::nil()),
            service_id: None,
            plan_version_id: Uuid::nil(),
            scenario_id: None,
            dr_test_id: None,
            mode: RunMode::Test,
            status: RunStatus::Declared,
            microservice_ids: vec![],
            declared_by: "u".into(),
            declared_at: Utc::now(),
            recovered_at: None,
            closed_at: None,
            outcome: None,
            summary: None,
            note: None,
            service_rto: None,
            mtpd: None,
            status_update_frequency: None,
        };
        assert!(run.change_status(RunStatus::Reconstituting).is_err());
        assert!(run.close(RunOutcome::Success, None).is_err());
        run.change_status(RunStatus::Recovered).expect("recovered");
        run.change_status(RunStatus::Reconstituting)
            .expect("reconstituting");
        run.close(RunOutcome::Success, Some("ok".into()))
            .expect("closed");
        assert!(run.change_status(RunStatus::Aborted).is_err());
    }
}
