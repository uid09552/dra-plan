//! DR plan versions: immutable snapshots of an IT service subtree, reviewed and approved
//! (workflow step 12), exported as an emergency handbook, and pinned by recovery runs.

use async_trait::async_trait;
use chrono::{DateTime, Months, NaiveDate, Utc};
use uuid::Uuid;

use crate::features::bia::domain::Bia;
use crate::features::data_protection::domain::DataProtection;
use crate::features::dependencies::domain::Dependency;
use crate::features::directory::domain::{Person, Role};
use crate::features::it_services::domain::ItService;
use crate::features::microservices::domain::Microservice;
use crate::features::objectives::domain::RecoveryObjective;
use crate::features::roles_comm::domain::{CommunicationRule, RoleAssignment};
use crate::features::runbooks::domain::{Runbook, RunbookStep};
use crate::features::scenarios::domain::{Scenario, ScenarioStatus};
use crate::features::strategies::domain::RecoveryStrategy;
use crate::shared::kernel::{
    AppError, AppResult, Language, TenantContext, TenantId, non_blank, str_enum,
};

// ───────────────────────────── Aggregate ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct RunbookWithSteps {
    pub runbook: Runbook,
    pub steps: Vec<RunbookStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MicroserviceBundle {
    pub microservice: Microservice,
    pub dependencies: Vec<Dependency>,
    pub objectives: Vec<RecoveryObjective>,
    pub strategies: Vec<RecoveryStrategy>,
    pub data_protection: Vec<DataProtection>,
    pub runbooks: Vec<RunbookWithSteps>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentEntry {
    pub assignment: RoleAssignment,
    pub role_name: String,
    pub person: Option<Person>,
}

/// The full IT service subtree: what a DR plan consists of.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanAggregate {
    pub service: ItService,
    pub bia: Option<Bia>,
    pub scenarios: Vec<Scenario>,
    /// Ordered by restore order, then name.
    pub microservices: Vec<MicroserviceBundle>,
    pub role_assignments: Vec<AssignmentEntry>,
    pub communication_rules: Vec<CommunicationRule>,
    pub roles: Vec<Role>,
    /// Persons referenced by the plan (owners, role assignments).
    pub persons: Vec<Person>,
}

impl PlanAggregate {
    pub fn objectives(&self) -> Vec<RecoveryObjective> {
        self.microservices
            .iter()
            .flat_map(|m| m.objectives.iter().cloned())
            .collect()
    }

    pub fn active_scenarios(&self) -> impl Iterator<Item = &Scenario> {
        self.scenarios
            .iter()
            .filter(|s| s.status != ScenarioStatus::Merged)
    }

    pub fn dr_scenarios(&self) -> impl Iterator<Item = &Scenario> {
        self.scenarios.iter().filter(|s| s.requires_dr())
    }

    /// Components a scenario affects: the explicitly chosen ones, otherwise the default component
    /// that stands for the whole service.
    pub fn affected_components(&self, scenario: &Scenario) -> Vec<Uuid> {
        if !scenario.affected_microservice_ids.is_empty() {
            return scenario.affected_microservice_ids.clone();
        }
        self.microservices
            .iter()
            .filter(|b| b.microservice.is_default)
            .map(|b| b.microservice.meta.id)
            .collect()
    }

    pub fn bundle(&self, microservice_id: Uuid) -> Option<&MicroserviceBundle> {
        self.microservices
            .iter()
            .find(|b| b.microservice.meta.id == microservice_id)
    }

    pub fn role_name(&self, role_id: Uuid) -> Option<&str> {
        self.roles
            .iter()
            .find(|r| r.meta.id == role_id)
            .map(|r| r.name.as_str())
    }

    pub fn person(&self, person_id: Uuid) -> Option<&Person> {
        self.persons.iter().find(|p| p.meta.id == person_id)
    }
}

// ───────────────────────────── Plan version ─────────────────────────────

str_enum! {
    pub enum PlanStatus { Draft = "draft", InReview = "in_review", Approved = "approved", Retired = "retired" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanVersion {
    pub id: Uuid,
    pub tenant_id: TenantId,
    /// `None` after the service was deleted (approved versions are kept for audit).
    pub service_id: Option<Uuid>,
    pub plan_number: u32,
    pub status: PlanStatus,
    /// Encoded snapshot (JSON), loaded only for detail views.
    pub snapshot: Option<String>,
    pub submitted_by: String,
    pub submitted_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub next_review_due: Option<NaiveDate>,
    pub comment: Option<String>,
    pub review_comment: Option<String>,
}

impl PlanVersion {
    pub fn submit(
        ctx: &TenantContext,
        service_id: Uuid,
        plan_number: u32,
        snapshot: String,
        comment: Option<String>,
    ) -> Self {
        PlanVersion {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            service_id: Some(service_id),
            plan_number,
            status: PlanStatus::InReview,
            snapshot: Some(snapshot),
            submitted_by: ctx.actor().to_owned(),
            submitted_at: Utc::now(),
            approved_by: None,
            approved_at: None,
            next_review_due: None,
            comment: non_blank(comment),
            review_comment: None,
        }
    }

    pub fn approve(
        &mut self,
        actor: &str,
        comment: Option<String>,
        next_review_due: Option<NaiveDate>,
        review_interval_months: u32,
    ) -> AppResult<()> {
        if self.status != PlanStatus::InReview {
            return Err(AppError::conflict(
                "only plan versions in review can be approved",
            ));
        }
        let now = Utc::now();
        let default_due = now
            .date_naive()
            .checked_add_months(Months::new(review_interval_months));
        let due = next_review_due.or(default_due);
        if due.is_some_and(|d| d <= now.date_naive()) {
            return Err(AppError::invalid(
                "INVALID_VALUE",
                Some("/nextReviewDue"),
                "the next review must be in the future",
            ));
        }
        self.status = PlanStatus::Approved;
        self.approved_by = Some(actor.to_owned());
        self.approved_at = Some(now);
        self.next_review_due = due;
        self.review_comment = non_blank(comment);
        Ok(())
    }

    pub fn reject(&mut self, comment: String) -> AppResult<()> {
        if self.status != PlanStatus::InReview {
            return Err(AppError::conflict(
                "only plan versions in review can be rejected",
            ));
        }
        let comment = non_blank(Some(comment)).ok_or_else(|| {
            AppError::invalid(
                "REQUIRED",
                Some("/comment"),
                "a comment is required when rejecting",
            )
        })?;
        self.status = PlanStatus::Draft;
        self.review_comment = Some(comment);
        Ok(())
    }
}

#[async_trait]
pub trait PlanVersionRepository: Send + Sync {
    /// Newest first, without snapshots.
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        status: Option<PlanStatus>,
    ) -> AppResult<Vec<PlanVersion>>;
    async fn get(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        with_snapshot: bool,
    ) -> AppResult<Option<PlanVersion>>;
    async fn current_approved(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Option<PlanVersion>>;
    async fn next_plan_number(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<u32>;
    async fn insert(&self, ctx: &TenantContext, version: &PlanVersion) -> AppResult<()>;
    /// Saves a status change; approving retires the previously approved version atomically.
    async fn save_status(&self, ctx: &TenantContext, version: &PlanVersion) -> AppResult<()>;
}

/// Port: (de)serializes the aggregate in the published `PlanSnapshot` format.
pub trait SnapshotCodec: Send + Sync {
    fn encode(&self, aggregate: &PlanAggregate) -> AppResult<String>;
    fn decode(&self, snapshot: &str) -> AppResult<PlanAggregate>;
}

/// Port: renders the offline emergency handbook. Without a version it renders the live draft.
pub trait PlanRenderer: Send + Sync {
    fn markdown(
        &self,
        version: Option<&PlanVersion>,
        aggregate: &PlanAggregate,
        lang: Language,
    ) -> String;
}
