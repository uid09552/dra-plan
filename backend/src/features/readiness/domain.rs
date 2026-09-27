//! Readiness assessment and compliance mapping, computed from the workflow gates.
//!
//! Both are pure functions over the gate evaluation, so the dashboard, reports and compliance view
//! all use the same underlying data (requirements §16, §17).

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::features::catalog::domain::Text;
use crate::features::dr_tests::domain::{DrTest, DrTestResult};
use crate::features::workflow::domain::{StepDefinition, WorkflowStepKey};
use crate::shared::kernel::{Issue, Severity, str_enum};

/// What readiness needs to know about one workflow step.
pub struct StepFacts<'a> {
    pub definition: &'static StepDefinition,
    pub complete: bool,
    pub issues: &'a [Issue],
}

impl StepFacts<'_> {
    fn gate_passed(&self) -> bool {
        !self.issues.iter().any(Issue::is_blocking)
    }
}

/// A concrete next action, derived from a gate issue or an unfinished step.
#[derive(Debug, Clone, PartialEq)]
pub struct NextAction {
    pub step: WorkflowStepKey,
    pub step_number: u8,
    pub severity: Severity,
    pub rule_id: String,
    pub message: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Readiness {
    /// 0–100: complete steps count fully, steps with a passed gate count half.
    pub score: u8,
    /// Blocking gate issues ("critical gaps").
    pub critical: usize,
    /// Warnings ("needs attention").
    pub attention: usize,
    /// Steps complete with a passed gate.
    pub completed: usize,
    pub total: usize,
    pub next_actions: Vec<NextAction>,
    pub last_exercise_at: Option<DateTime<Utc>>,
    /// Share of measured test results that met the RTO (percent).
    pub rto_compliance: Option<u8>,
    /// No approved plan, or its review date has passed.
    pub review_due: bool,
    /// Selected DR scenarios without an executed test or exercise.
    pub untested_scenarios: usize,
}

pub const MAX_NEXT_ACTIONS: usize = 5;

/// Assesses the readiness of one IT service.
pub fn assess(
    steps: &[StepFacts<'_>],
    tests: &[(DrTest, Vec<DrTestResult>)],
    dr_scenario_ids: &[Uuid],
    approved_review_due: Option<Option<NaiveDate>>,
    today: NaiveDate,
) -> Readiness {
    let total = steps.len();
    let points: usize = steps
        .iter()
        .map(|s| match (s.complete, s.gate_passed()) {
            (true, true) => 2,
            (false, true) => 1,
            _ => 0,
        })
        .sum();
    let score = if total == 0 {
        0
    } else {
        (points * 100 / (2 * total)) as u8
    };
    let count = |sev: Severity| {
        steps
            .iter()
            .flat_map(|s| s.issues)
            .filter(|i| i.severity == sev)
            .count()
    };

    let executed: Vec<&(DrTest, Vec<DrTestResult>)> =
        tests.iter().filter(|(t, _)| t.is_executed()).collect();
    let tested: HashSet<Uuid> = executed.iter().map(|(t, _)| t.scenario_id).collect();
    let measured: Vec<bool> = executed
        .iter()
        .flat_map(|(_, r)| r)
        .filter_map(DrTestResult::rto_met)
        .collect();

    Readiness {
        score,
        critical: count(Severity::Blocking),
        attention: count(Severity::Warning),
        completed: steps
            .iter()
            .filter(|s| s.complete && s.gate_passed())
            .count(),
        total,
        next_actions: next_actions(steps),
        last_exercise_at: executed.iter().filter_map(|(t, _)| t.executed_at).max(),
        rto_compliance: (!measured.is_empty())
            .then(|| (measured.iter().filter(|m| **m).count() * 100 / measured.len()) as u8),
        review_due: match approved_review_due {
            None => true,
            Some(due) => due.is_some_and(|d| d < today),
        },
        untested_scenarios: dr_scenario_ids
            .iter()
            .filter(|id| !tested.contains(id))
            .count(),
    }
}

/// Blocking issues first, then warnings, both in workflow order; if nothing is open, the first
/// unfinished step.
fn next_actions(steps: &[StepFacts<'_>]) -> Vec<NextAction> {
    let action = |s: &StepFacts<'_>, i: &Issue| NextAction {
        step: s.definition.key,
        step_number: s.definition.number,
        severity: i.severity,
        rule_id: i.rule_id.clone(),
        message: i.message.clone(),
        entity_type: i.entity_type.clone(),
        entity_id: i.entity_id,
    };
    let mut actions: Vec<NextAction> = [Severity::Blocking, Severity::Warning]
        .iter()
        .flat_map(|sev| {
            steps.iter().flat_map(move |s| {
                s.issues
                    .iter()
                    .filter(move |i| i.severity == *sev)
                    .map(move |i| (s, i))
            })
        })
        .take(MAX_NEXT_ACTIONS)
        .map(|(s, i)| action(s, i))
        .collect();
    if actions.is_empty() {
        if let Some(s) = steps.iter().find(|s| !s.complete) {
            actions.push(NextAction {
                step: s.definition.key,
                step_number: s.definition.number,
                severity: Severity::Info,
                rule_id: "COMPLETE_STEP".into(),
                message: format!(
                    "complete step {}: {}",
                    s.definition.number, s.definition.title.en
                ),
                entity_type: None,
                entity_id: None,
            });
        }
    }
    actions
}

// ───────────────────────────── Compliance mapping ─────────────────────────────

str_enum! {
    pub enum ComplianceStatus { Fulfilled = "fulfilled", InProgress = "in_progress", Open = "open" }
}

/// A requirement of a framework, what to do for it, which evidence proves it, and the workflow
/// steps that implement it.
pub struct Requirement {
    pub id: &'static str,
    pub framework: &'static str,
    pub reference: &'static str,
    pub title: Text,
    pub what_to_do: Text,
    pub evidence: Text,
    pub steps: &'static [WorkflowStepKey],
}

const fn t(en: &'static str, de: &'static str) -> Text {
    Text { en, de }
}

use WorkflowStepKey as K;

/// Summary mapping; verify the references against the current publications before an audit.
pub const REQUIREMENTS: &[Requirement] = &[
    Requirement {
        id: "nist-bia",
        framework: "NIST SP 800-34",
        reference: "Step 2 — Business impact analysis",
        title: t("Business impact analysis", "Business-Impact-Analyse"),
        what_to_do: t(
            "Identify the service, its dependencies, MTD, RTO and RPO.",
            "Dienst, Abhängigkeiten, MTD, RTO und RPO ermitteln.",
        ),
        evidence: t(
            "Service definition, dependency map, BIA",
            "Dienstbeschreibung, Abhängigkeiten, BIA",
        ),
        steps: &[K::DefineService, K::MapDependencies, K::BusinessImpact],
    },
    Requirement {
        id: "nist-preventive",
        framework: "NIST SP 800-34",
        reference: "Step 3 — Preventive controls",
        title: t("Preventive controls", "Vorbeugende Maßnahmen"),
        what_to_do: t(
            "Document backups and replication that reduce outage impact.",
            "Datensicherung und Replikation dokumentieren, die Ausfallfolgen mindern.",
        ),
        evidence: t(
            "Data protection entries per microservice",
            "Datensicherungseinträge je Microservice",
        ),
        steps: &[K::RecoveryStrategies],
    },
    Requirement {
        id: "nist-strategies",
        framework: "NIST SP 800-34",
        reference: "Step 4 — Recovery strategies",
        title: t("Contingency strategies", "Wiederanlaufstrategien"),
        what_to_do: t(
            "Define recovery objectives and a strategy per scenario that meets them.",
            "Wiederanlaufziele und je Szenario eine passende Strategie festlegen.",
        ),
        evidence: t(
            "Recovery objectives, selected strategies, gap check",
            "Wiederanlaufziele, ausgewählte Strategien, Soll-Ist-Vergleich",
        ),
        steps: &[K::RecoveryObjectives, K::RecoveryStrategies],
    },
    Requirement {
        id: "nist-plan",
        framework: "NIST SP 800-34",
        reference: "Step 5 — Contingency plan",
        title: t(
            "Information system contingency plan",
            "Notfallplan des IT-Systems",
        ),
        what_to_do: t(
            "Write runbooks, assign roles, define notification, approve the plan.",
            "Wiederanlaufpläne erstellen, Rollen zuweisen, Alarmierung festlegen, Plan freigeben.",
        ),
        evidence: t(
            "Runbooks, role assignments, communication rules, approved plan version",
            "Wiederanlaufpläne, Rollenzuordnung, Kommunikationsregeln, freigegebene Planversion",
        ),
        steps: &[K::Runbooks, K::Roles, K::Communication, K::ReviewApprove],
    },
    Requirement {
        id: "nist-tte",
        framework: "NIST SP 800-34",
        reference: "Step 6 — Testing, training, exercises",
        title: t("Plan testing and exercises", "Tests und Übungen"),
        what_to_do: t(
            "Exercise the plan and measure achieved RTO/RPO.",
            "Plan üben und erreichte RTO/RPO messen.",
        ),
        evidence: t(
            "Executed DR tests with results",
            "Durchgeführte Tests mit Ergebnissen",
        ),
        steps: &[K::Test, K::Measure],
    },
    Requirement {
        id: "nist-maintenance",
        framework: "NIST SP 800-34",
        reference: "Step 7 — Plan maintenance",
        title: t("Plan maintenance", "Planpflege"),
        what_to_do: t(
            "Track findings and corrective actions with owners and due dates.",
            "Feststellungen und Maßnahmen mit Verantwortlichen und Terminen verfolgen.",
        ),
        evidence: t("Action items, plan versions", "Maßnahmen, Planversionen"),
        steps: &[K::Improve],
    },
    Requirement {
        id: "bsi-bia",
        framework: "BSI 200-4",
        reference: "Business-Impact-Analyse (MTA, WAZ, MTDV)",
        title: t("Business impact analysis", "Business-Impact-Analyse"),
        what_to_do: t(
            "Assess impact over time; set MTA, WAZ, MTDV and the minimum operating level.",
            "Schaden über die Zeit bewerten; MTA, WAZ, MTDV und Notbetriebsniveau festlegen.",
        ),
        evidence: t("BIA with impact ratings", "BIA mit Schadensbewertung"),
        steps: &[K::BusinessImpact],
    },
    Requirement {
        id: "bsi-risk",
        framework: "BSI 200-4",
        reference: "Risikoanalyse",
        title: t("Risk analysis", "Risikoanalyse"),
        what_to_do: t(
            "Brainstorm, consolidate and select the scenarios that need a plan.",
            "Szenarien sammeln, konsolidieren und die planungsrelevanten auswählen.",
        ),
        evidence: t(
            "Scenario catalogue with decisions and rationale",
            "Szenariokatalog mit Entscheidungen und Begründung",
        ),
        steps: &[
            K::BrainstormScenarios,
            K::ConsolidateScenarios,
            K::SelectScenarios,
        ],
    },
    Requirement {
        id: "bsi-soll-ist",
        framework: "BSI 200-4",
        reference: "Soll-Ist-Vergleich",
        title: t("Required vs. actual capability", "Soll-Ist-Vergleich"),
        what_to_do: t(
            "Compare required recovery times with what the strategies achieve; close or accept gaps.",
            "Geforderte Wiederanlaufzeiten mit den Strategien vergleichen; Lücken schließen oder akzeptieren.",
        ),
        evidence: t(
            "Gap check per strategy, accepted gaps as actions",
            "Soll-Ist-Vergleich je Strategie, akzeptierte Lücken als Maßnahmen",
        ),
        steps: &[K::RecoveryStrategies],
    },
    Requirement {
        id: "bsi-bao",
        framework: "BSI 200-4",
        reference: "Besondere Aufbauorganisation (BAO)",
        title: t("Emergency organization", "Besondere Aufbauorganisation"),
        what_to_do: t(
            "Staff every recovery role with a primary and a deputy.",
            "Jede Notfallrolle mit Hauptverantwortlichem und Vertretung besetzen.",
        ),
        evidence: t(
            "Role assignments with deputies",
            "Rollenzuordnung mit Vertretung",
        ),
        steps: &[K::Roles],
    },
    Requirement {
        id: "bsi-alerting",
        framework: "BSI 200-4",
        reference: "Alarmierung und Eskalation",
        title: t("Alerting and escalation", "Alarmierung und Eskalation"),
        what_to_do: t(
            "Define who is informed when, and who authorizes failover.",
            "Festlegen, wer wann informiert wird und wer das Umschalten freigibt.",
        ),
        evidence: t("Communication rules", "Kommunikationsregeln"),
        steps: &[K::Communication],
    },
    Requirement {
        id: "bsi-handbook",
        framework: "BSI 200-4",
        reference: "Notfallhandbuch, Wiederanlaufpläne",
        title: t("Emergency handbook", "Notfallhandbuch"),
        what_to_do: t(
            "Document executable recovery steps and approve the plan.",
            "Ausführbare Wiederanlaufschritte dokumentieren und den Plan freigeben.",
        ),
        evidence: t(
            "Runbooks and approved plan (handbook export)",
            "Wiederanlaufpläne und freigegebener Plan (Handbuch-Export)",
        ),
        steps: &[K::Runbooks, K::ReviewApprove],
    },
    Requirement {
        id: "bsi-exercises",
        framework: "BSI 200-4",
        reference: "Tests und Übungen",
        title: t("Tests and exercises", "Tests und Übungen"),
        what_to_do: t(
            "Run exercises of increasing depth and record the results.",
            "Übungen steigender Tiefe durchführen und Ergebnisse festhalten.",
        ),
        evidence: t("DR tests with results", "Tests mit Ergebnissen"),
        steps: &[K::Test, K::Measure],
    },
    Requirement {
        id: "bsi-improvement",
        framework: "BSI 200-4",
        reference: "Kontinuierliche Verbesserung",
        title: t("Continuous improvement", "Kontinuierliche Verbesserung"),
        what_to_do: t(
            "Turn findings into owned, scheduled actions and update the plan.",
            "Feststellungen in terminierte Maßnahmen mit Verantwortlichen überführen und den Plan anpassen.",
        ),
        evidence: t("Action items", "Maßnahmen"),
        steps: &[K::Improve],
    },
    Requirement {
        id: "grundschutz-con3",
        framework: "BSI IT-Grundschutz",
        reference: "CON.3 Datensicherungskonzept",
        title: t("Backup concept", "Datensicherungskonzept"),
        what_to_do: t(
            "Back up every data store often enough for its RPO; test restores.",
            "Jeden Datenspeicher passend zum RPO sichern; Wiederherstellung testen.",
        ),
        evidence: t(
            "Data protection entries, restore tests",
            "Datensicherungseinträge, Wiederherstellungstests",
        ),
        steps: &[K::RecoveryStrategies, K::Test],
    },
    Requirement {
        id: "grundschutz-der4",
        framework: "BSI IT-Grundschutz",
        reference: "DER.4 Notfallmanagement",
        title: t("Emergency management", "Notfallmanagement"),
        what_to_do: t(
            "Maintain an approved, tested emergency plan.",
            "Einen freigegebenen, geübten Notfallplan pflegen.",
        ),
        evidence: t(
            "Approved plan version, DR tests",
            "Freigegebene Planversion, Tests",
        ),
        steps: &[K::ReviewApprove, K::Test],
    },
];

/// Status of a requirement: fulfilled when all mapped steps are complete with passed gates; open
/// when a mapped step has blocking issues; in progress otherwise.
pub fn requirement_status(req: &Requirement, steps: &[StepFacts<'_>]) -> (ComplianceStatus, usize) {
    let mapped: Vec<&StepFacts<'_>> = steps
        .iter()
        .filter(|s| req.steps.contains(&s.definition.key))
        .collect();
    let blocking: usize = mapped
        .iter()
        .flat_map(|s| s.issues)
        .filter(|i| i.is_blocking())
        .count();
    let status = if mapped.iter().all(|s| s.complete && s.gate_passed()) {
        ComplianceStatus::Fulfilled
    } else if blocking > 0 {
        ComplianceStatus::Open
    } else {
        ComplianceStatus::InProgress
    };
    (status, blocking)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::workflow::domain::STEP_DEFINITIONS;

    fn facts<'a>(issues: &'a [Vec<Issue>], complete: &[bool]) -> Vec<StepFacts<'a>> {
        STEP_DEFINITIONS
            .iter()
            .enumerate()
            .map(|(i, d)| StepFacts {
                definition: d,
                complete: complete.get(i).copied().unwrap_or(false),
                issues: &issues[i],
            })
            .collect()
    }

    #[test]
    fn scores_and_prioritizes_blocking_issues() {
        let mut issues: Vec<Vec<Issue>> = vec![Vec::new(); STEP_DEFINITIONS.len()];
        issues[2] = vec![Issue::warning("NO_IMPACT_RATINGS", "w")];
        issues[6] = vec![Issue::blocking("MISSING_OBJECTIVE", "b")];
        let steps = facts(&issues, &[true, true]);
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");
        let r = assess(&steps, &[], &[Uuid::new_v4()], None, today);
        assert_eq!(r.critical, 1);
        assert_eq!(r.attention, 1);
        assert_eq!(r.completed, 2);
        // 2 complete (2 points each) + 12 passed gates (1 point each) out of 30.
        assert_eq!(r.score, (16 * 100 / 30) as u8);
        assert_eq!(r.next_actions[0].rule_id, "MISSING_OBJECTIVE");
        assert_eq!(r.next_actions[1].rule_id, "NO_IMPACT_RATINGS");
        assert!(r.review_due);
        assert_eq!(r.untested_scenarios, 1);
    }

    #[test]
    fn suggests_completing_the_next_step_when_nothing_is_open() {
        let issues: Vec<Vec<Issue>> = vec![Vec::new(); STEP_DEFINITIONS.len()];
        let steps = facts(&issues, &[true]);
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");
        let r = assess(
            &steps,
            &[],
            &[],
            Some(NaiveDate::from_ymd_opt(2027, 1, 1)),
            today,
        );
        assert_eq!(r.next_actions[0].rule_id, "COMPLETE_STEP");
        assert_eq!(r.next_actions[0].step_number, 2);
        assert!(!r.review_due);
    }

    #[test]
    fn requirement_status_follows_mapped_steps() {
        let mut issues: Vec<Vec<Issue>> = vec![Vec::new(); STEP_DEFINITIONS.len()];
        issues[9] = vec![Issue::blocking("ROLE_WITHOUT_DEPUTY", "x")];
        let all_complete = vec![true; STEP_DEFINITIONS.len()];
        let steps = facts(&issues, &all_complete);
        let bao = REQUIREMENTS
            .iter()
            .find(|r| r.id == "bsi-bao")
            .expect("exists");
        assert_eq!(requirement_status(bao, &steps).0, ComplianceStatus::Open);
        let bia = REQUIREMENTS
            .iter()
            .find(|r| r.id == "bsi-bia")
            .expect("exists");
        assert_eq!(
            requirement_status(bia, &steps).0,
            ComplianceStatus::Fulfilled
        );
    }
}
