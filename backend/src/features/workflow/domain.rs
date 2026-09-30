//! The guided 15-step authoring workflow (docs/workflows/plan-authoring.md) with its gates.
//! Gates evaluate the integrity rules of docs/domain/dr-plan-model.md §4 on the live aggregate.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::features::action_items::domain::ActionItem;
use crate::features::catalog::domain::Text;
use crate::features::dependencies::domain::build_graph;
use crate::features::dr_tests::domain::{DrTest, DrTestResult, DrTestType};
use crate::features::objectives::domain::effective;
use crate::features::plans::domain::PlanAggregate;
use crate::features::roles_comm::domain::CommunicationTrigger;
use crate::features::runbooks::domain::critical_path;
use crate::features::scenarios::domain::ScenarioStatus;
use crate::features::strategies::domain::{GapStatus, ImplementationStatus};
use crate::shared::kernel::{AppResult, Issue, Severity, TenantContext, str_enum};

str_enum! {
    pub enum WorkflowStepKey {
        DefineService = "define_service",
        MapDependencies = "map_dependencies",
        BusinessImpact = "business_impact",
        BrainstormScenarios = "brainstorm_scenarios",
        ConsolidateScenarios = "consolidate_scenarios",
        SelectScenarios = "select_scenarios",
        RecoveryObjectives = "recovery_objectives",
        RecoveryStrategies = "recovery_strategies",
        Runbooks = "runbooks",
        Roles = "roles",
        Communication = "communication",
        ReviewApprove = "review_approve",
        Test = "test",
        Measure = "measure",
        Improve = "improve",
    }
}

str_enum! {
    pub enum WorkflowPhase {
        Understand = "understand",
        Assess = "assess",
        Design = "design",
        Document = "document",
        ValidateMaintain = "validate_maintain",
    }
}

str_enum! {
    pub enum WorkflowStepStatus {
        NotStarted = "not_started",
        InProgress = "in_progress",
        Complete = "complete",
        NeedsReview = "needs_review",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StepDefinition {
    pub key: WorkflowStepKey,
    pub number: u8,
    pub phase: WorkflowPhase,
    pub title: Text,
    pub nist_ref: &'static str,
    pub bsi_ref: &'static str,
}

const fn step(
    key: WorkflowStepKey,
    number: u8,
    phase: WorkflowPhase,
    en: &'static str,
    de: &'static str,
    nist_ref: &'static str,
    bsi_ref: &'static str,
) -> StepDefinition {
    StepDefinition {
        key,
        number,
        phase,
        title: Text { en, de },
        nist_ref,
        bsi_ref,
    }
}

use WorkflowPhase as P;
use WorkflowStepKey as K;

pub const STEP_DEFINITIONS: &[StepDefinition] = &[
    step(
        K::DefineService,
        1,
        P::Understand,
        "Define the service",
        "Dienst beschreiben",
        "SP 800-34 step 2 (BIA: identify processes)",
        "200-4 Prozess- und Ressourcenerhebung",
    ),
    step(
        K::MapDependencies,
        2,
        P::Understand,
        "Map dependencies",
        "Abhängigkeiten erfassen",
        "SP 800-34 step 2 (resource requirements)",
        "200-4 Ressourcen / Abhängigkeiten",
    ),
    step(
        K::BusinessImpact,
        3,
        P::Assess,
        "Business impact analysis",
        "Business-Impact-Analyse",
        "SP 800-34 step 2 (MTD, RTO, RPO)",
        "200-4 BIA (MTA, WAZ, MTDV, Notbetriebsniveau)",
    ),
    step(
        K::BrainstormScenarios,
        4,
        P::Assess,
        "Brainstorm scenarios",
        "Szenarien sammeln",
        "SP 800-34 step 2",
        "200-4 Risikoanalyse",
    ),
    step(
        K::ConsolidateScenarios,
        5,
        P::Assess,
        "Consolidate scenarios",
        "Szenarien konsolidieren",
        "SP 800-34 step 2",
        "200-4 Risikoanalyse",
    ),
    step(
        K::SelectScenarios,
        6,
        P::Assess,
        "Select and prioritize",
        "Auswählen und priorisieren",
        "SP 800-34 step 2",
        "200-4 Risikoanalyse / Risikobewertung",
    ),
    step(
        K::RecoveryObjectives,
        7,
        P::Design,
        "Recovery objectives",
        "Wiederanlaufziele",
        "SP 800-34 step 2 (recovery priorities)",
        "200-4 WAZ / MTDV je Ressource",
    ),
    step(
        K::RecoveryStrategies,
        8,
        P::Design,
        "Recovery strategies",
        "Kontinuitätsstrategien",
        "SP 800-34 steps 3–4",
        "200-4 Soll-Ist-Vergleich, Kontinuitätsstrategien; CON.3",
    ),
    step(
        K::Runbooks,
        9,
        P::Document,
        "Runbooks",
        "Wiederanlaufpläne",
        "SP 800-34 step 5 (Activation, Recovery, Reconstitution)",
        "200-4 Wiederanlauf- und Wiederherstellungspläne",
    ),
    step(
        K::Roles,
        10,
        P::Document,
        "Roles and ownership",
        "Rollen und Verantwortung",
        "SP 800-34 step 5 (roles)",
        "200-4 BAO, Vertretungsregelung",
    ),
    step(
        K::Communication,
        11,
        P::Document,
        "Communication",
        "Kommunikation",
        "SP 800-34 step 5 (notification)",
        "200-4 Alarmierung und Eskalation",
    ),
    step(
        K::ReviewApprove,
        12,
        P::Document,
        "Review and approve",
        "Prüfen und freigeben",
        "SP 800-34 step 5",
        "200-4 Notfallhandbuch, Freigabe",
    ),
    step(
        K::Test,
        13,
        P::ValidateMaintain,
        "Test",
        "Testen und üben",
        "SP 800-34 step 6 (TT&E)",
        "200-4 Tests und Übungen",
    ),
    step(
        K::Measure,
        14,
        P::ValidateMaintain,
        "Measure",
        "Ergebnisse messen",
        "SP 800-34 step 6",
        "200-4 Übungsauswertung",
    ),
    step(
        K::Improve,
        15,
        P::ValidateMaintain,
        "Improve",
        "Verbessern",
        "SP 800-34 step 7 (maintenance)",
        "200-4 Kontinuierliche Verbesserung",
    ),
];

pub fn definition(key: WorkflowStepKey) -> &'static StepDefinition {
    STEP_DEFINITIONS
        .iter()
        .find(|d| d.key == key)
        .unwrap_or(&STEP_DEFINITIONS[0])
}

/// Stored progress of one step.
#[derive(Debug, Clone, PartialEq)]
pub struct StepProgress {
    pub key: WorkflowStepKey,
    pub status: WorkflowStepStatus,
    pub comment: Option<String>,
    pub completed_by: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl StepProgress {
    pub fn new(key: WorkflowStepKey) -> Self {
        StepProgress {
            key,
            status: WorkflowStepStatus::NotStarted,
            comment: None,
            completed_by: None,
            completed_at: None,
        }
    }

    pub fn completed(key: WorkflowStepKey, actor: &str, comment: Option<String>) -> Self {
        StepProgress {
            key,
            status: WorkflowStepStatus::Complete,
            comment,
            completed_by: Some(actor.to_owned()),
            completed_at: Some(Utc::now()),
        }
    }
}

#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn list(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Vec<StepProgress>>;
    async fn save(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        steps: &[StepProgress],
    ) -> AppResult<()>;
}

/// Everything the gates look at.
pub struct GateContext<'a> {
    pub aggregate: &'a PlanAggregate,
    pub has_approved_plan: bool,
    pub dr_tests: &'a [(DrTest, Vec<DrTestResult>)],
    pub action_items: &'a [ActionItem],
}

fn issue(
    key: WorkflowStepKey,
    severity: Severity,
    rule: &str,
    message: impl Into<String>,
) -> Issue {
    let d = definition(key);
    let mut i = Issue::new(severity, rule, message);
    i.workflow_step = Some(key.to_string());
    i.standard_ref = Some(format!("NIST {} / BSI {}", d.nist_ref, d.bsi_ref));
    i
}

struct Collector {
    key: WorkflowStepKey,
    issues: Vec<Issue>,
}

impl Collector {
    fn add(
        &mut self,
        severity: Severity,
        rule: &str,
        message: String,
        entity: Option<(&str, Uuid)>,
    ) {
        let mut i = issue(self.key, severity, rule, message);
        if let Some((t, id)) = entity {
            i = i.entity(t, id);
        }
        self.issues.push(i);
    }

    fn block(&mut self, rule: &str, message: String, entity: Option<(&str, Uuid)>) {
        self.add(Severity::Blocking, rule, message, entity);
    }

    fn warn(&mut self, rule: &str, message: String, entity: Option<(&str, Uuid)>) {
        self.add(Severity::Warning, rule, message, entity);
    }

    fn push(&mut self, issue: Issue) {
        self.issues.push(issue);
    }
}

/// Evaluates the gate of one step.
pub fn evaluate(key: WorkflowStepKey, g: &GateContext<'_>) -> Vec<Issue> {
    let a = g.aggregate;
    let mut c = Collector {
        key,
        issues: Vec::new(),
    };
    let objectives = a.objectives();
    // Pairs of (DR scenario, affected component) that need objectives, strategies and runbooks.
    // Scenarios without explicitly affected components apply to the default component.
    let pairs: Vec<(Uuid, &str, Uuid, &str)> = a
        .dr_scenarios()
        .flat_map(|s| {
            a.affected_components(s)
                .into_iter()
                .filter_map(|m| a.bundle(m))
                .map(move |b| {
                    (
                        s.meta.id,
                        s.title.as_str(),
                        b.microservice.meta.id,
                        b.microservice.name.as_str(),
                    )
                })
        })
        .collect();

    match key {
        K::DefineService => {
            if a.service.business_owner_id.is_none() {
                c.block(
                    "MISSING_BUSINESS_OWNER",
                    "the service has no business owner".into(),
                    None,
                );
            }
            if a.service.technical_owner_id.is_none() {
                c.block(
                    "MISSING_TECHNICAL_OWNER",
                    "the service has no technical owner".into(),
                    None,
                );
            }
        }
        K::MapDependencies => {
            // Dependencies are optional: a component without dependencies is not an issue.
            let ms: Vec<_> = a
                .microservices
                .iter()
                .map(|b| b.microservice.clone())
                .collect();
            let deps: Vec<_> = a
                .microservices
                .iter()
                .flat_map(|b| b.dependencies.clone())
                .collect();
            if !build_graph(&ms, &[], &deps, &objectives).cycles.is_empty() {
                c.warn(
                    "DEPENDENCY_CYCLE",
                    "the dependency graph contains a cycle".into(),
                    None,
                );
            }
        }
        K::BusinessImpact => match &a.bia {
            None => c.block(
                "BIA_MISSING",
                "no business impact analysis recorded".into(),
                None,
            ),
            Some(b) => {
                if b.impact_ratings.is_empty() {
                    c.warn(
                        "NO_IMPACT_RATINGS",
                        "the BIA has no impact ratings over time".into(),
                        None,
                    );
                }
                if b.minimum_operating_level.is_none() {
                    c.warn(
                        "NO_MINIMUM_OPERATING_LEVEL",
                        "no minimum operating level (Notbetriebsniveau) defined".into(),
                        None,
                    );
                }
            }
        },
        K::BrainstormScenarios => {
            let active: Vec<_> = a.active_scenarios().collect();
            if active.is_empty() {
                c.block(
                    "NO_SCENARIOS",
                    "no disaster scenarios recorded".into(),
                    None,
                );
            }
            let categories: HashSet<_> = a
                .scenarios
                .iter()
                .filter_map(|s| s.category.clone())
                .collect();
            if !active.is_empty() && categories.len() < 3 {
                c.warn(
                    "FEW_SCENARIO_CATEGORIES",
                    "scenarios cover fewer than three categories".into(),
                    None,
                );
            }
        }
        K::ConsolidateScenarios => {
            for s in a.active_scenarios().filter(|s| s.category.is_none()) {
                c.block(
                    "SCENARIO_UNCATEGORIZED",
                    format!("scenario `{}` has no category", s.title),
                    Some(("scenario", s.meta.id)),
                );
            }
        }
        K::SelectScenarios => {
            for s in a.active_scenarios() {
                if s.status == ScenarioStatus::Brainstormed {
                    c.block(
                        "SCENARIO_UNDECIDED",
                        format!("scenario `{}` is neither selected nor rejected", s.title),
                        Some(("scenario", s.meta.id)),
                    );
                }
            }
            if a.dr_scenarios().next().is_none() {
                c.warn(
                    "NO_DR_SCENARIO",
                    "no selected scenario requires a DR plan".into(),
                    None,
                );
            }
        }
        K::RecoveryObjectives => {
            for (sid, stitle, mid, mname) in &pairs {
                if effective(&objectives, *mid, Some(*sid)).is_none() {
                    c.block(
                        "MISSING_OBJECTIVE",
                        format!("no recovery objective for `{mname}` in scenario `{stitle}`"),
                        Some(("microservice", *mid)),
                    );
                }
            }
            if let Some(bia) = &a.bia {
                for o in &objectives {
                    for mut i in o.check_against_bia(bia) {
                        i.workflow_step = Some(key.to_string());
                        c.push(i);
                    }
                }
            }
            for b in &a.microservices {
                for d in b
                    .dependencies
                    .iter()
                    .filter(|d| d.rto_conflict(&objectives))
                {
                    c.push(
                        issue(key, Severity::Blocking, "DEPENDENCY_RTO_CONFLICT", format!(
                            "`{}` cannot recover within its RTO because a critical dependency recovers slower",
                            b.microservice.name
                        ))
                        .entity("dependency", d.meta.id),
                    );
                }
            }
        }
        K::RecoveryStrategies => {
            for (sid, stitle, mid, mname) in &pairs {
                let Some(b) = a.bundle(*mid) else { continue };
                match b
                    .strategies
                    .iter()
                    .find(|s| s.covers(*sid) && s.is_selected)
                {
                    None => c.block(
                        "MISSING_STRATEGY",
                        format!(
                            "no selected recovery strategy for `{mname}` in scenario `{stitle}`"
                        ),
                        Some(("microservice", *mid)),
                    ),
                    Some(s) => {
                        let gap = s.gap_for(&objectives, *sid);
                        if gap.status == GapStatus::Gap {
                            if s.accepted_gap_action_item_id.is_some() {
                                c.warn(
                                    "STRATEGY_GAP_ACCEPTED",
                                    format!("accepted recovery gap for `{mname}` in `{stitle}`"),
                                    Some(("recovery_strategy", s.meta.id)),
                                );
                            } else {
                                c.block(
                                    "STRATEGY_GAP",
                                    format!(
                                        "the selected strategy for `{mname}` misses its objective"
                                    ),
                                    Some(("recovery_strategy", s.meta.id)),
                                );
                            }
                        }
                        match s.implementation_status {
                            ImplementationStatus::Implemented if s.last_tested_at.is_none() => c.warn(
                                "MITIGATION_NEVER_TESTED",
                                format!("the recovery capability for `{mname}` in `{stitle}` has never been tested"),
                                Some(("recovery_strategy", s.meta.id)),
                            ),
                            ImplementationStatus::Implemented => {}
                            _ => c.warn(
                                "MITIGATION_NOT_IMPLEMENTED",
                                format!("the mitigation for `{mname}` in `{stitle}` is not implemented yet"),
                                Some(("recovery_strategy", s.meta.id)),
                            ),
                        }
                    }
                }
            }
            for b in &a.microservices {
                if !b.microservice.data_stores.is_empty() && b.data_protection.is_empty() {
                    c.warn(
                        "NO_DATA_PROTECTION",
                        format!(
                            "`{}` has data stores but no backup configuration",
                            b.microservice.name
                        ),
                        Some(("microservice", b.microservice.meta.id)),
                    );
                }
                for d in &b.data_protection {
                    if d.supports_rpo(&objectives) == Some(false) {
                        c.warn(
                            "BACKUP_FREQUENCY_EXCEEDS_RPO",
                            format!("backup of `{}` runs less often than the RPO", d.data_store),
                            Some(("data_protection", d.meta.id)),
                        );
                    }
                }
            }
        }
        K::Runbooks => {
            for (sid, stitle, mid, mname) in &pairs {
                let Some(b) = a.bundle(*mid) else { continue };
                let runbooks: Vec<_> = b
                    .runbooks
                    .iter()
                    .filter(|r| r.runbook.scenario_id == *sid)
                    .collect();
                if runbooks.is_empty() {
                    c.block(
                        "MISSING_RUNBOOK",
                        format!("no runbook for `{mname}` in scenario `{stitle}`"),
                        Some(("microservice", *mid)),
                    );
                }
                let objective = effective(&objectives, *mid, Some(*sid));
                for rb in runbooks {
                    let id = rb.runbook.meta.id;
                    if rb.steps.is_empty() {
                        c.block(
                            "EMPTY_RUNBOOK",
                            format!("runbook `{}` has no steps", rb.runbook.title),
                            Some(("runbook", id)),
                        );
                    }
                    for s in &rb.steps {
                        if s.owner_role_id.is_none() {
                            c.block(
                                "STEP_WITHOUT_OWNER",
                                format!("step `{}` has no owner role", s.title),
                                Some(("runbook_step", s.meta.id)),
                            );
                        }
                        if s.verification.is_none() {
                            c.block(
                                "STEP_WITHOUT_VERIFICATION",
                                format!("step `{}` has no verification", s.title),
                                Some(("runbook_step", s.meta.id)),
                            );
                        }
                        if s.expected_duration.is_none() {
                            c.warn(
                                "STEP_WITHOUT_DURATION",
                                format!("step `{}` has no expected duration", s.title),
                                Some(("runbook_step", s.meta.id)),
                            );
                        }
                    }
                    if let Some(o) = objective {
                        let path = critical_path(&rb.steps);
                        if path > o.rto {
                            c.block(
                                "RUNBOOK_EXCEEDS_RTO",
                                format!(
                                    "runbook `{}` takes {} min on its critical path, RTO is {} min",
                                    rb.runbook.title,
                                    path.get(),
                                    o.rto.get()
                                ),
                                Some(("runbook", id)),
                            );
                        }
                    }
                }
            }
        }
        K::Roles => {
            let mut used: HashSet<Uuid> = HashSet::new();
            for b in &a.microservices {
                for rb in &b.runbooks {
                    for s in &rb.steps {
                        used.extend(s.owner_role_id);
                        used.extend(s.requires_authorization_role_id);
                    }
                }
            }
            for r in &a.communication_rules {
                used.insert(r.responsible_role_id);
                used.extend(r.authorizer_role_id);
            }
            let mut staffing: HashMap<Uuid, (bool, bool)> = HashMap::new();
            for e in &a.role_assignments {
                let entry = staffing.entry(e.assignment.role_id).or_default();
                if e.assignment.is_deputy {
                    entry.1 = true
                } else {
                    entry.0 = true
                }
            }
            let mut used: Vec<_> = used.into_iter().collect();
            used.sort();
            for role_id in used {
                let name = a.role_name(role_id).unwrap_or("unknown role");
                let (primary, deputy) = staffing.get(&role_id).copied().unwrap_or_default();
                if !primary {
                    c.block(
                        "ROLE_WITHOUT_ASSIGNEE",
                        format!("role `{name}` is used but has no assigned person"),
                        Some(("role", role_id)),
                    );
                }
                if !deputy {
                    c.block(
                        "ROLE_WITHOUT_DEPUTY",
                        format!("role `{name}` has no deputy"),
                        Some(("role", role_id)),
                    );
                }
            }
        }
        K::Communication => {
            let has =
                |t: CommunicationTrigger| a.communication_rules.iter().any(|r| r.trigger == t);
            if !has(CommunicationTrigger::DrDeclared) {
                c.block(
                    "MISSING_DR_DECLARED_RULE",
                    "no communication rule for DR declaration".into(),
                    None,
                );
            }
            if !has(CommunicationTrigger::Recovered) {
                c.block(
                    "MISSING_RECOVERED_RULE",
                    "no communication rule for service recovery".into(),
                    None,
                );
            }
            let authorizer = a
                .communication_rules
                .iter()
                .any(|r| r.authorizer_role_id.is_some())
                || a.microservices
                    .iter()
                    .flat_map(|b| &b.runbooks)
                    .flat_map(|r| &r.steps)
                    .any(|s| s.requires_authorization_role_id.is_some());
            if !authorizer {
                c.block(
                    "MISSING_FAILOVER_AUTHORIZER",
                    "nobody is designated to authorize failover".into(),
                    None,
                );
            }
        }
        K::ReviewApprove => {
            if !g.has_approved_plan {
                c.block("PLAN_NOT_APPROVED", "no approved plan version".into(), None);
            }
        }
        K::Test => {
            let executed: Vec<_> = g.dr_tests.iter().filter(|(t, _)| t.is_executed()).collect();
            if executed.is_empty() {
                c.block(
                    "NO_EXECUTED_TEST",
                    "no DR test or exercise has been executed".into(),
                    None,
                );
            } else if executed
                .iter()
                .all(|(t, _)| t.test_type == DrTestType::PlanReview)
            {
                c.warn(
                    "ONLY_PLAN_REVIEWS",
                    "only plan reviews were executed; run at least a tabletop exercise".into(),
                    None,
                );
            }
        }
        K::Measure => {
            for (t, results) in g.dr_tests.iter().filter(|(t, _)| t.is_executed()) {
                if results.is_empty() && t.test_type != DrTestType::PlanReview {
                    c.block(
                        "TEST_WITHOUT_RESULTS",
                        "an executed test has no recorded results".into(),
                        Some(("dr_test", t.meta.id)),
                    );
                }
            }
        }
        K::Improve => {
            for item in g.action_items.iter().filter(|i| i.is_open()) {
                if item.owner_person_id.is_none() {
                    c.block(
                        "ACTION_WITHOUT_OWNER",
                        format!("action item `{}` has no owner", item.title),
                        Some(("action_item", item.meta.id)),
                    );
                }
                if item.due_date.is_none() {
                    c.block(
                        "ACTION_WITHOUT_DUE_DATE",
                        format!("action item `{}` has no due date", item.title),
                        Some(("action_item", item.meta.id)),
                    );
                }
            }
        }
    }
    c.issues
}

/// Issues of the design steps 1–11 (used by the validation report and plan submission).
pub fn design_issues(aggregate: &PlanAggregate) -> Vec<Issue> {
    let g = GateContext {
        aggregate,
        has_approved_plan: false,
        dr_tests: &[],
        action_items: &[],
    };
    STEP_DEFINITIONS
        .iter()
        .filter(|d| d.number <= 11)
        .flat_map(|d| evaluate(d.key, &g))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_cover_every_step_in_order() {
        assert_eq!(STEP_DEFINITIONS.len(), WorkflowStepKey::ALL.len());
        for (i, d) in STEP_DEFINITIONS.iter().enumerate() {
            assert_eq!(d.number as usize, i + 1);
            assert_eq!(d.key, WorkflowStepKey::ALL[i]);
        }
    }
}
