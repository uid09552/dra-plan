use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::domain::{
    GateContext, STEP_DEFINITIONS, StepDefinition, StepProgress, WorkflowRepository,
    WorkflowStepKey, WorkflowStepStatus, design_issues, evaluate,
};
use crate::features::action_items::domain::{ActionItemFilter, ActionItemRepository};
use crate::features::dr_tests::domain::{DrTest, DrTestRepository, DrTestResult};
use crate::features::plans::application::PlanUseCases;
use crate::features::plans::domain::{PlanAggregate, PlanVersion};
use crate::shared::kernel::{AppError, AppResult, Issue, TenantContext};

#[derive(Debug, Clone)]
pub struct StepState {
    pub definition: &'static StepDefinition,
    pub status: WorkflowStepStatus,
    pub gate_passed: bool,
    pub issues: Vec<Issue>,
    pub completed_by: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct WorkflowState {
    pub service_id: Uuid,
    pub completion_percent: u8,
    pub current_step: Option<WorkflowStepKey>,
    pub steps: Vec<StepState>,
}

/// Gate evaluation of one service plus the inputs it was computed from.
pub struct Evaluation {
    pub state: WorkflowState,
    pub aggregate: PlanAggregate,
    pub dr_tests: Vec<(DrTest, Vec<DrTestResult>)>,
    pub approved_plan: Option<PlanVersion>,
}

pub struct WorkflowUseCases {
    repo: Arc<dyn WorkflowRepository>,
    plans: Arc<PlanUseCases>,
    dr_tests: Arc<dyn DrTestRepository>,
    action_items: Arc<dyn ActionItemRepository>,
}

impl WorkflowUseCases {
    pub fn new(
        repo: Arc<dyn WorkflowRepository>,
        plans: Arc<PlanUseCases>,
        dr_tests: Arc<dyn DrTestRepository>,
        action_items: Arc<dyn ActionItemRepository>,
    ) -> Self {
        Self {
            repo,
            plans,
            dr_tests,
            action_items,
        }
    }

    /// All 15 steps with stored status and live gate evaluation.
    pub async fn state(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<WorkflowState> {
        Ok(self.evaluate(ctx, service_id).await?.state)
    }

    /// Evaluates all gates and returns the state together with the data it was computed from
    /// (used by readiness and compliance).
    pub async fn evaluate(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Evaluation> {
        let aggregate = self.plans.loader().load(ctx, service_id).await?;
        let approved_plan = self.plans.current_approved(ctx, service_id).await?;
        let has_approved_plan = approved_plan.is_some();
        let tests = self.dr_tests.list_by_service(ctx, service_id).await?;
        let ids: Vec<Uuid> = tests.iter().map(|t| t.meta.id).collect();
        let mut results: HashMap<Uuid, Vec<_>> = HashMap::new();
        for (test_id, r) in self.dr_tests.results(ctx, &ids).await? {
            results.entry(test_id).or_default().push(r);
        }
        let dr_tests: Vec<_> = tests
            .into_iter()
            .map(|t| {
                let r = results.remove(&t.meta.id).unwrap_or_default();
                (t, r)
            })
            .collect();
        let action_items = self
            .action_items
            .list_by_service(ctx, service_id, &ActionItemFilter::default())
            .await?;
        let gate = GateContext {
            aggregate: &aggregate,
            has_approved_plan,
            dr_tests: &dr_tests,
            action_items: &action_items,
        };

        let stored: HashMap<WorkflowStepKey, StepProgress> = self
            .repo
            .list(ctx, service_id)
            .await?
            .into_iter()
            .map(|p| (p.key, p))
            .collect();
        let steps: Vec<StepState> = STEP_DEFINITIONS
            .iter()
            .map(|d| {
                let issues = evaluate(d.key, &gate);
                let progress = stored
                    .get(&d.key)
                    .cloned()
                    .unwrap_or_else(|| StepProgress::new(d.key));
                StepState {
                    definition: d,
                    status: progress.status,
                    gate_passed: !issues.iter().any(Issue::is_blocking),
                    issues,
                    completed_by: progress.completed_by,
                    completed_at: progress.completed_at,
                }
            })
            .collect();
        let complete = steps
            .iter()
            .filter(|s| s.status == WorkflowStepStatus::Complete)
            .count();
        let state = WorkflowState {
            service_id,
            completion_percent: (complete * 100 / steps.len()) as u8,
            current_step: steps
                .iter()
                .find(|s| s.status != WorkflowStepStatus::Complete)
                .map(|s| s.definition.key),
            steps,
        };
        Ok(Evaluation {
            state,
            aggregate,
            dr_tests,
            approved_plan,
        })
    }

    /// Completes a step: earlier steps must be complete (scenario selection before recovery
    /// design), the gate must have no blocking issues, and warnings must be acknowledged.
    pub async fn complete(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        key: WorkflowStepKey,
        comment: Option<String>,
        acknowledge_warnings: bool,
    ) -> AppResult<StepState> {
        let state = self.state(ctx, service_id).await?;
        let index = state
            .steps
            .iter()
            .position(|s| s.definition.key == key)
            .ok_or(AppError::NotFound("workflow step"))?;
        let mut issues: Vec<Issue> = state.steps[..index]
            .iter()
            .filter(|s| s.status != WorkflowStepStatus::Complete)
            .map(|s| {
                let mut i = Issue::blocking(
                    "WORKFLOW_ORDER",
                    format!(
                        "step {} ({}) must be completed first",
                        s.definition.number, s.definition.key
                    ),
                );
                i.workflow_step = Some(s.definition.key.to_string());
                i
            })
            .collect();
        let step = &state.steps[index];
        issues.extend(step.issues.iter().filter(|i| i.is_blocking()).cloned());
        if issues.is_empty() && !acknowledge_warnings && !step.issues.is_empty() {
            issues.extend(step.issues.iter().cloned());
            issues.push(Issue::blocking(
                "WARNINGS_NOT_ACKNOWLEDGED",
                "set acknowledgeWarnings to complete the step despite warnings",
            ));
        }
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        let progress = StepProgress::completed(key, ctx.actor(), comment);
        self.repo
            .save(ctx, service_id, std::slice::from_ref(&progress))
            .await?;
        Ok(StepState {
            status: WorkflowStepStatus::Complete,
            completed_by: progress.completed_by,
            completed_at: progress.completed_at,
            ..step.clone()
        })
    }

    /// Reopens a step; completed later steps need review again.
    pub async fn reopen(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        key: WorkflowStepKey,
    ) -> AppResult<WorkflowState> {
        let state = self.state(ctx, service_id).await?;
        let mut changes = vec![StepProgress {
            status: WorkflowStepStatus::InProgress,
            ..StepProgress::new(key)
        }];
        let number = super::domain::definition(key).number;
        for s in state
            .steps
            .iter()
            .filter(|s| s.definition.number > number && s.status == WorkflowStepStatus::Complete)
        {
            changes.push(StepProgress {
                status: WorkflowStepStatus::NeedsReview,
                completed_by: s.completed_by.clone(),
                completed_at: s.completed_at,
                ..StepProgress::new(s.definition.key)
            });
        }
        self.repo.save(ctx, service_id, &changes).await?;
        self.state(ctx, service_id).await
    }

    /// Integrity rules and gap checks of the design steps (validation report).
    pub async fn validate(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Vec<Issue>> {
        let aggregate = self.plans.loader().load(ctx, service_id).await?;
        Ok(design_issues(&aggregate))
    }
}
