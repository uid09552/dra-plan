use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::domain::{
    ComplianceStatus, NextAction, REQUIREMENTS, Readiness, Requirement, StepFacts, assess,
    requirement_status,
};
use crate::features::it_services::domain::{ItServiceRepository, ServiceFilter};
use crate::features::workflow::application::{Evaluation, WorkflowUseCases};
use crate::features::workflow::domain::WorkflowStepStatus;
use crate::shared::kernel::{AppResult, PageRequest, Severity, TenantContext};

#[derive(Debug, Clone)]
pub struct ServiceReadiness {
    pub service_id: Uuid,
    pub name: String,
    pub readiness: Readiness,
}

/// Tenant-wide readiness ("How ready are we?") with the most important next actions.
#[derive(Debug, Clone)]
pub struct TenantReadiness {
    pub score: u8,
    pub critical: usize,
    pub attention: usize,
    pub completed: usize,
    pub total: usize,
    pub last_exercise_at: Option<DateTime<Utc>>,
    pub rto_compliance: Option<u8>,
    pub plans_requiring_review: usize,
    pub untested_scenarios: usize,
    /// Across services: blocking before warnings, then by service readiness (lowest first).
    pub next_actions: Vec<(Uuid, String, NextAction)>,
    pub services: Vec<ServiceReadiness>,
}

pub struct ComplianceItem {
    pub requirement: &'static Requirement,
    pub status: ComplianceStatus,
    pub blocking_issues: usize,
}

pub struct ReadinessUseCases {
    workflow: Arc<WorkflowUseCases>,
    services: Arc<dyn ItServiceRepository>,
}

impl ReadinessUseCases {
    pub fn new(workflow: Arc<WorkflowUseCases>, services: Arc<dyn ItServiceRepository>) -> Self {
        Self { workflow, services }
    }

    pub async fn service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<ServiceReadiness> {
        let evaluation = self.workflow.evaluate(ctx, service_id).await?;
        Ok(readiness_of(&evaluation))
    }

    pub async fn tenant(&self, ctx: &TenantContext) -> AppResult<TenantReadiness> {
        let mut services = Vec::new();
        let mut page = PageRequest::parse(None, Some(PageRequest::MAX_LIMIT))?;
        loop {
            let batch = self
                .services
                .list(ctx, &ServiceFilter::default(), page)
                .await?;
            for service in &batch.items {
                services.push(self.service(ctx, service.meta.id).await?);
            }
            match batch.next_cursor {
                Some(cursor) => {
                    page = PageRequest::parse(Some(&cursor), Some(PageRequest::MAX_LIMIT))?
                }
                None => break,
            }
        }
        Ok(summarize(services))
    }

    pub async fn compliance(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<ComplianceItem>> {
        let evaluation = self.workflow.evaluate(ctx, service_id).await?;
        let facts = step_facts(&evaluation);
        Ok(REQUIREMENTS
            .iter()
            .map(|requirement| {
                let (status, blocking_issues) = requirement_status(requirement, &facts);
                ComplianceItem {
                    requirement,
                    status,
                    blocking_issues,
                }
            })
            .collect())
    }
}

fn step_facts(e: &Evaluation) -> Vec<StepFacts<'_>> {
    e.state
        .steps
        .iter()
        .map(|s| StepFacts {
            definition: s.definition,
            complete: s.status == WorkflowStepStatus::Complete,
            issues: &s.issues,
        })
        .collect()
}

fn readiness_of(e: &Evaluation) -> ServiceReadiness {
    let dr_scenarios: Vec<Uuid> = e.aggregate.dr_scenarios().map(|s| s.meta.id).collect();
    let review = e.approved_plan.as_ref().map(|p| p.next_review_due);
    let readiness = assess(
        &step_facts(e),
        &e.dr_tests,
        &dr_scenarios,
        review,
        Utc::now().date_naive(),
    );
    ServiceReadiness {
        service_id: e.aggregate.service.meta.id,
        name: e.aggregate.service.name.clone(),
        readiness,
    }
}

fn summarize(mut services: Vec<ServiceReadiness>) -> TenantReadiness {
    services.sort_by_key(|s| (s.readiness.score, s.name.clone()));
    let sum = |f: fn(&Readiness) -> usize| services.iter().map(|s| f(&s.readiness)).sum::<usize>();
    let compliance: Vec<u8> = services
        .iter()
        .filter_map(|s| s.readiness.rto_compliance)
        .collect();
    let mut next_actions: Vec<(Uuid, String, NextAction)> = services
        .iter()
        .flat_map(|s| {
            s.readiness
                .next_actions
                .iter()
                .map(move |a| (s.service_id, s.name.clone(), a.clone()))
        })
        .collect();
    // Stable sort keeps "lowest readiness first" within the same severity.
    next_actions.sort_by_key(|(_, _, a)| match a.severity {
        Severity::Blocking => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });
    next_actions.truncate(super::domain::MAX_NEXT_ACTIONS * 2);
    TenantReadiness {
        score: if services.is_empty() {
            0
        } else {
            (services
                .iter()
                .map(|s| u32::from(s.readiness.score))
                .sum::<u32>()
                / services.len() as u32) as u8
        },
        critical: sum(|r| r.critical),
        attention: sum(|r| r.attention),
        completed: sum(|r| r.completed),
        total: sum(|r| r.total),
        last_exercise_at: services
            .iter()
            .filter_map(|s| s.readiness.last_exercise_at)
            .max(),
        rto_compliance: (!compliance.is_empty()).then(|| {
            (compliance.iter().map(|c| u32::from(*c)).sum::<u32>() / compliance.len() as u32) as u8
        }),
        plans_requiring_review: services.iter().filter(|s| s.readiness.review_due).count(),
        untested_scenarios: sum(|r| r.untested_scenarios),
        next_actions,
        services,
    }
}
