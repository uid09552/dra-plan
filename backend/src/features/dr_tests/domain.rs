//! DR tests and exercises (NIST SP 800-34 step 6 TT&E / BSI 200-4 Tests und Übungen) and their
//! measured results (workflow steps 13–14).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Meta, Minutes, TenantContext, non_blank, str_enum,
};

str_enum! {
    pub enum DrTestType {
        PlanReview = "plan_review",
        Tabletop = "tabletop",
        Simulation = "simulation",
        TechnicalRecovery = "technical_recovery",
        FullDr = "full_dr",
    }
}

str_enum! {
    pub enum DrTestOutcome {
        Planned = "planned",
        Passed = "passed",
        PartiallyPassed = "partially_passed",
        Failed = "failed",
        Cancelled = "cancelled",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrTest {
    pub meta: Meta,
    pub service_id: Uuid,
    pub test_type: DrTestType,
    pub scenario_id: Uuid,
    pub plan_version_id: Option<Uuid>,
    pub planned_at: DateTime<Utc>,
    pub executed_at: Option<DateTime<Utc>>,
    pub participant_ids: Vec<Uuid>,
    pub outcome: DrTestOutcome,
    pub report: Option<String>,
    pub recovery_run_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct DrTestInput {
    pub test_type: Option<DrTestType>,
    pub scenario_id: Option<Uuid>,
    pub plan_version_id: Option<Uuid>,
    pub planned_at: Option<DateTime<Utc>>,
    pub executed_at: Option<Option<DateTime<Utc>>>,
    pub participant_ids: Option<Vec<Uuid>>,
    pub outcome: Option<DrTestOutcome>,
    pub report: Option<String>,
}

impl DrTest {
    pub fn create(
        ctx: &TenantContext,
        service_id: Uuid,
        test_type: DrTestType,
        scenario_id: Uuid,
        planned_at: DateTime<Utc>,
        input: DrTestInput,
    ) -> AppResult<Self> {
        let mut t = DrTest {
            meta: Meta::new(ctx),
            service_id,
            test_type,
            scenario_id,
            plan_version_id: None,
            planned_at,
            executed_at: None,
            participant_ids: Vec::new(),
            outcome: DrTestOutcome::Planned,
            report: None,
            recovery_run_id: None,
        };
        t.apply(input)?;
        Ok(t)
    }

    pub fn apply(&mut self, p: DrTestInput) -> AppResult<()> {
        if let Some(v) = p.test_type {
            self.test_type = v;
        }
        if let Some(v) = p.scenario_id {
            self.scenario_id = v;
        }
        if p.plan_version_id.is_some() {
            self.plan_version_id = p.plan_version_id;
        }
        if let Some(v) = p.planned_at {
            self.planned_at = v;
        }
        if let Some(v) = p.executed_at {
            self.executed_at = v;
        }
        if let Some(mut v) = p.participant_ids {
            v.sort();
            v.dedup();
            self.participant_ids = v;
        }
        if let Some(v) = p.outcome {
            self.outcome = v;
        }
        if p.report.is_some() {
            self.report = non_blank(p.report);
        }
        let executed_outcome = !matches!(
            self.outcome,
            DrTestOutcome::Planned | DrTestOutcome::Cancelled
        );
        if executed_outcome && self.executed_at.is_none() {
            self.executed_at = Some(Utc::now());
        }
        Ok(())
    }

    pub fn is_executed(&self) -> bool {
        self.executed_at.is_some() && self.outcome != DrTestOutcome::Cancelled
    }

    pub fn ensure_deletable(&self) -> AppResult<()> {
        if self.is_executed() {
            return Err(AppError::conflict(
                "executed tests are evidence and cannot be deleted",
            ));
        }
        Ok(())
    }
}

/// Measured recovery time/point of one microservice in a test, against the targets of the tested plan.
#[derive(Debug, Clone, PartialEq)]
pub struct DrTestResult {
    pub microservice_id: Uuid,
    pub target_rto: Option<Minutes>,
    pub target_rpo: Option<Minutes>,
    pub achieved_rto: Option<Minutes>,
    pub achieved_rpo: Option<Minutes>,
    pub notes: Option<String>,
}

impl DrTestResult {
    pub fn rto_met(&self) -> Option<bool> {
        Some(self.achieved_rto? <= self.target_rto?)
    }

    pub fn rpo_met(&self) -> Option<bool> {
        Some(self.achieved_rpo? <= self.target_rpo?)
    }

    /// Passed when every measured value meets its target (and at least one was measured).
    pub fn passed(&self) -> bool {
        let checks: Vec<bool> = [self.rto_met(), self.rpo_met()]
            .into_iter()
            .flatten()
            .collect();
        !checks.is_empty() && checks.into_iter().all(|ok| ok)
    }
}

#[async_trait]
pub trait DrTestRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<DrTest>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<DrTest>>;
    async fn insert(&self, ctx: &TenantContext, t: &DrTest) -> AppResult<DrTest>;
    async fn update(&self, ctx: &TenantContext, t: &DrTest) -> AppResult<DrTest>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
    async fn results(
        &self,
        ctx: &TenantContext,
        test_ids: &[Uuid],
    ) -> AppResult<Vec<(Uuid, DrTestResult)>>;
    async fn replace_results(
        &self,
        ctx: &TenantContext,
        test_id: Uuid,
        results: &[DrTestResult],
    ) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(v: u32) -> Option<Minutes> {
        Some(Minutes::new(v).expect("valid"))
    }

    #[test]
    fn result_passes_only_when_measured_targets_are_met() {
        let base = DrTestResult {
            microservice_id: Uuid::nil(),
            target_rto: m(120),
            target_rpo: m(15),
            achieved_rto: m(95),
            achieved_rpo: m(8),
            notes: None,
        };
        assert!(base.passed());
        assert!(
            !DrTestResult {
                achieved_rpo: m(30),
                ..base.clone()
            }
            .passed()
        );
        assert!(
            !DrTestResult {
                achieved_rto: None,
                achieved_rpo: None,
                ..base
            }
            .passed()
        );
    }
}
