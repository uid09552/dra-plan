//! Disaster scenarios: brainstorm → consolidate (merge) → select/reject (workflow steps 4–6,
//! BSI 200-4 risk analysis). Scenario selection must be agreed before recovery design.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Issues, Meta, Provenance, Rating, TenantContext, non_blank, str_enum,
};

str_enum! {
    /// Scenario categories of the requirements (docs/requirements/README.md §2).
    pub enum ScenarioCategory {
        Infrastructure = "infrastructure",
        Hardware = "hardware",
        Network = "network",
        Cloud = "cloud",
        Application = "application",
        Database = "database",
        Storage = "storage",
        Backup = "backup",
        Cybersecurity = "cybersecurity",
        People = "people",
        Supplier = "supplier",
        Facility = "facility",
        Power = "power",
        Environmental = "environmental",
        Operational = "operational",
    }
}

str_enum! {
    pub enum ScenarioStatus { Brainstormed = "brainstormed", Merged = "merged", Selected = "selected", Rejected = "rejected" }
}

str_enum! {
    pub enum DrRequired {
        Yes = "yes",
        NoHandledByHa = "no_handled_by_ha",
        NoAcceptedRisk = "no_accepted_risk",
        DegradedMode = "degraded_mode",
    }
}

str_enum! {
    pub enum Priority { Low = "low", Medium = "medium", High = "high", Critical = "critical" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub meta: Meta,
    pub provenance: Provenance,
    pub service_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub category: Option<ScenarioCategory>,
    /// Parent in the brainstorming tree (sub-scenario), `None` for top-level scenarios.
    pub parent_scenario_id: Option<Uuid>,
    pub affected_microservice_ids: Vec<Uuid>,
    pub status: ScenarioStatus,
    pub merged_into_id: Option<Uuid>,
    pub likelihood: Option<Rating>,
    pub impact: Option<Rating>,
    pub priority: Option<Priority>,
    pub dr_required: Option<DrRequired>,
    pub decision_rationale: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<ScenarioCategory>,
    pub parent_scenario_id: Option<Option<Uuid>>,
    pub affected_microservice_ids: Option<Vec<Uuid>>,
    /// Risk assessment before the decision (e.g. placing the scenario in the risk matrix).
    pub likelihood: Option<Rating>,
    pub impact: Option<Rating>,
}

str_enum! {
    pub enum Decision { Selected = "selected", Rejected = "rejected" }
}

#[derive(Debug, Clone)]
pub struct ScenarioDecision {
    pub decision: Decision,
    pub likelihood: Option<Rating>,
    pub impact: Option<Rating>,
    pub priority: Option<Priority>,
    pub dr_required: Option<DrRequired>,
    pub rationale: String,
    pub affected_microservice_ids: Option<Vec<Uuid>>,
}

impl Scenario {
    pub fn create(ctx: &TenantContext, service_id: Uuid, input: ScenarioInput) -> AppResult<Self> {
        let mut s = Scenario {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            service_id,
            title: String::new(),
            description: None,
            category: None,
            parent_scenario_id: None,
            affected_microservice_ids: Vec::new(),
            status: ScenarioStatus::Brainstormed,
            merged_into_id: None,
            likelihood: None,
            impact: None,
            priority: None,
            dr_required: None,
            decision_rationale: None,
            decided_by: None,
            decided_at: None,
        };
        s.apply(input)?;
        Ok(s)
    }

    pub fn apply(&mut self, p: ScenarioInput) -> AppResult<()> {
        if let Some(v) = p.title {
            self.title = v.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        if p.category.is_some() {
            self.category = p.category;
        }
        if let Some(ids) = p.affected_microservice_ids {
            self.affected_microservice_ids = dedup(ids);
        }
        if let Some(parent) = p.parent_scenario_id {
            self.parent_scenario_id = parent;
        }
        if p.likelihood.is_some() {
            self.likelihood = p.likelihood;
        }
        if p.impact.is_some() {
            self.impact = p.impact;
        }
        let mut issues = Issues::new();
        issues.check(
            !self.title.is_empty(),
            "REQUIRED",
            "/title",
            "title is required",
        );
        issues.check(
            self.parent_scenario_id != Some(self.meta.id),
            "INVALID_VALUE",
            "/parentScenarioId",
            "a scenario cannot be its own parent",
        );
        issues.into_result()
    }

    /// Records the selection decision (the key gate of the workflow).
    pub fn decide(&mut self, actor: &str, d: ScenarioDecision) -> AppResult<()> {
        if self.status == ScenarioStatus::Merged {
            return Err(AppError::conflict(
                "a merged scenario cannot be decided; decide the scenario it was merged into",
            ));
        }
        let rationale = d.rationale.trim().to_owned();
        let mut issues = Issues::new();
        issues.check(
            !rationale.is_empty(),
            "REQUIRED",
            "/decisionRationale",
            "a decision rationale is required",
        );
        issues.check(
            d.decision == Decision::Rejected || d.dr_required.is_some(),
            "REQUIRED",
            "/drRequired",
            "drRequired is required when selecting a scenario",
        );
        issues.into_result()?;
        self.status = match d.decision {
            Decision::Selected => ScenarioStatus::Selected,
            Decision::Rejected => ScenarioStatus::Rejected,
        };
        self.likelihood = d.likelihood.or(self.likelihood);
        self.impact = d.impact.or(self.impact);
        self.priority = d.priority.or(self.priority);
        self.dr_required = match d.decision {
            Decision::Selected => d.dr_required,
            Decision::Rejected => Some(d.dr_required.unwrap_or(DrRequired::NoAcceptedRisk)),
        };
        if let Some(ids) = d.affected_microservice_ids {
            self.affected_microservice_ids = dedup(ids);
        }
        self.decision_rationale = Some(rationale);
        self.decided_by = Some(actor.to_owned());
        self.decided_at = Some(Utc::now());
        Ok(())
    }

    /// Likelihood × impact (1..=16), when both are rated.
    pub fn risk_score(&self) -> Option<u8> {
        Some(self.likelihood?.get() * self.impact?.get())
    }

    /// Selected and requiring a DR plan (drives objectives, strategies and runbooks).
    pub fn requires_dr(&self) -> bool {
        self.status == ScenarioStatus::Selected && self.dr_required == Some(DrRequired::Yes)
    }

    /// Absorbs the given scenarios into this one (consolidation, workflow step 5).
    pub fn absorb(
        &mut self,
        sources: &mut [Scenario],
        merged_description: Option<String>,
    ) -> AppResult<()> {
        if self.status == ScenarioStatus::Merged {
            return Err(AppError::conflict(
                "cannot merge into a scenario that was itself merged",
            ));
        }
        for source in sources.iter_mut() {
            if source.meta.id == self.meta.id {
                return Err(AppError::invalid(
                    "INVALID_VALUE",
                    Some("/sourceScenarioIds"),
                    "a scenario cannot be merged into itself",
                ));
            }
            if source.service_id != self.service_id {
                return Err(AppError::invalid(
                    "INVALID_VALUE",
                    Some("/sourceScenarioIds"),
                    "scenarios can only be merged within the same IT service",
                ));
            }
            if source.status == ScenarioStatus::Merged {
                return Err(AppError::conflict(format!(
                    "scenario {} is already merged",
                    source.meta.id
                )));
            }
            source.status = ScenarioStatus::Merged;
            source.merged_into_id = Some(self.meta.id);
            self.affected_microservice_ids
                .extend(source.affected_microservice_ids.iter().copied());
            if self.category.is_none() {
                self.category = source.category;
            }
        }
        self.affected_microservice_ids = dedup(std::mem::take(&mut self.affected_microservice_ids));
        if let Some(d) = non_blank(merged_description) {
            self.description = Some(d);
        }
        Ok(())
    }
}

/// Checks that `parent_id` is a valid parent for `child` among the scenarios of its service:
/// it must exist, not be merged, and not be a descendant of the child (no cycles).
pub fn check_parent(
    child: &Scenario,
    parent_id: Uuid,
    service_scenarios: &[Scenario],
) -> AppResult<()> {
    let find = |id: Uuid| service_scenarios.iter().find(|s| s.meta.id == id);
    let parent = find(parent_id).ok_or_else(|| {
        AppError::invalid(
            "REFERENCE_NOT_FOUND",
            Some("/parentScenarioId"),
            "the parent scenario must belong to the same IT service",
        )
    })?;
    if parent.status == ScenarioStatus::Merged {
        return Err(AppError::invalid(
            "INVALID_VALUE",
            Some("/parentScenarioId"),
            "a merged scenario cannot be a parent",
        ));
    }
    let mut cursor = Some(parent_id);
    while let Some(id) = cursor {
        if id == child.meta.id {
            return Err(AppError::invalid(
                "SCENARIO_CYCLE",
                Some("/parentScenarioId"),
                "a scenario cannot be placed below its own sub-scenario",
            ));
        }
        cursor = find(id).and_then(|s| s.parent_scenario_id);
    }
    Ok(())
}

fn dedup(ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioFilter {
    pub status: Option<ScenarioStatus>,
    pub category: Option<ScenarioCategory>,
    pub dr_required: Option<DrRequired>,
}

#[async_trait]
pub trait ScenarioRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        filter: &ScenarioFilter,
    ) -> AppResult<Vec<Scenario>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Scenario>>;
    async fn get_many(&self, ctx: &TenantContext, ids: &[Uuid]) -> AppResult<Vec<Scenario>>;
    async fn insert(&self, ctx: &TenantContext, s: &Scenario) -> AppResult<Scenario>;
    /// Updates one or more scenarios atomically (optimistic version check on each).
    async fn update_all(
        &self,
        ctx: &TenantContext,
        scenarios: &[Scenario],
    ) -> AppResult<Vec<Scenario>>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::kernel::{Principal, TenantId, TenantRole};

    fn ctx() -> TenantContext {
        TenantContext {
            tenant_id: TenantId(Uuid::nil()),
            principal: Principal {
                user: "u".into(),
                role: TenantRole::Admin,
            },
        }
    }

    fn scenario(title: &str) -> Scenario {
        Scenario::create(
            &ctx(),
            Uuid::nil(),
            ScenarioInput {
                title: Some(title.into()),
                ..Default::default()
            },
        )
        .expect("valid")
    }

    #[test]
    fn selection_requires_rationale_and_dr_required() {
        let mut s = scenario("Region outage");
        let decision = ScenarioDecision {
            decision: Decision::Selected,
            likelihood: None,
            impact: None,
            priority: None,
            dr_required: None,
            rationale: "critical".into(),
            affected_microservice_ids: None,
        };
        assert!(s.decide("u", decision.clone()).is_err());
        let ok = ScenarioDecision {
            dr_required: Some(DrRequired::Yes),
            ..decision
        };
        s.decide("u", ok).expect("valid decision");
        assert!(s.requires_dr());
    }

    #[test]
    fn parent_must_not_create_a_cycle() {
        let mut root = scenario("Region outage");
        let mut child = scenario("AZ outage");
        child.parent_scenario_id = Some(root.meta.id);
        let all = vec![root.clone(), child.clone()];
        assert!(check_parent(&child, root.meta.id, &all).is_ok());
        assert!(check_parent(&root, child.meta.id, &all).is_err(), "cycle");
        root.parent_scenario_id = None;
        assert!(
            check_parent(&root, uuid::Uuid::new_v4(), &all).is_err(),
            "unknown parent"
        );
    }

    #[test]
    fn merge_marks_sources_and_unions_affected_microservices() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut target = scenario("DB corruption");
        target.affected_microservice_ids = vec![a];
        let mut source = scenario("DB data deleted");
        source.affected_microservice_ids = vec![a, b];
        let mut sources = vec![source];
        target.absorb(&mut sources, None).expect("merge");
        assert_eq!(sources[0].status, ScenarioStatus::Merged);
        assert_eq!(sources[0].merged_into_id, Some(target.meta.id));
        assert_eq!(target.affected_microservice_ids.len(), 2);
        assert!(
            sources[0]
                .decide(
                    "u",
                    ScenarioDecision {
                        decision: Decision::Rejected,
                        likelihood: None,
                        impact: None,
                        priority: None,
                        dr_required: None,
                        rationale: "x".into(),
                        affected_microservice_ids: None,
                    }
                )
                .is_err()
        );
    }
}
