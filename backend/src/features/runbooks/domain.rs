//! Runbooks: the executable mitigation steps of a microservice for a scenario (workflow step 9).
//! Steps are grouped into the NIST SP 800-34 phases Activation → Recovery → Reconstitution.
//!
//! Execution semantics: steps run sequentially by `seq` unless `depends_on` is given, which
//! replaces the implicit "previous step" dependency and allows parallel work.

use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Issue, Issues, Meta, Minutes, Provenance, TenantContext, non_blank,
    str_enum,
};

str_enum! {
    /// NIST SP 800-34 plan phase (BSI: Alarmierung / Wiederanlauf / Wiederherstellung).
    pub enum RunbookPhase { Activation = "activation", Recovery = "recovery", Reconstitution = "reconstitution" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Runbook {
    pub meta: Meta,
    pub provenance: Provenance,
    pub microservice_id: Uuid,
    pub scenario_id: Uuid,
    pub strategy_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunbookInput {
    pub scenario_id: Option<Uuid>,
    pub strategy_id: Option<Option<Uuid>>,
    pub title: Option<String>,
    pub description: Option<String>,
}

impl Runbook {
    pub fn create(
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Uuid,
        input: RunbookInput,
    ) -> AppResult<Self> {
        let mut r = Runbook {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            microservice_id,
            scenario_id,
            strategy_id: None,
            title: String::new(),
            description: None,
        };
        r.apply(input)?;
        Ok(r)
    }

    pub fn apply(&mut self, p: RunbookInput) -> AppResult<()> {
        if let Some(v) = p.scenario_id {
            self.scenario_id = v;
        }
        if let Some(v) = p.strategy_id {
            self.strategy_id = v;
        }
        if let Some(v) = p.title {
            self.title = v.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        let mut issues = Issues::new();
        issues.check(
            !self.title.is_empty(),
            "REQUIRED",
            "/title",
            "title is required",
        );
        issues.into_result()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunbookStep {
    pub meta: Meta,
    pub provenance: Provenance,
    pub runbook_id: Uuid,
    pub seq: u32,
    pub phase: RunbookPhase,
    pub title: String,
    pub instructions: Option<String>,
    pub owner_role_id: Option<Uuid>,
    pub expected_duration: Option<Minutes>,
    pub verification: Option<String>,
    pub depends_on: Vec<Uuid>,
    pub is_decision_point: bool,
    pub requires_authorization_role_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct StepInput {
    pub phase: Option<RunbookPhase>,
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub owner_role_id: Option<Option<Uuid>>,
    pub expected_duration: Option<Option<Minutes>>,
    pub verification: Option<String>,
    pub depends_on: Option<Vec<Uuid>>,
    pub is_decision_point: Option<bool>,
    pub requires_authorization_role_id: Option<Option<Uuid>>,
}

impl RunbookStep {
    pub fn create(
        ctx: &TenantContext,
        runbook_id: Uuid,
        seq: u32,
        phase: RunbookPhase,
        input: StepInput,
    ) -> AppResult<Self> {
        let mut s = RunbookStep {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            runbook_id,
            seq,
            phase,
            title: String::new(),
            instructions: None,
            owner_role_id: None,
            expected_duration: None,
            verification: None,
            depends_on: Vec::new(),
            is_decision_point: false,
            requires_authorization_role_id: None,
        };
        s.apply(input)?;
        Ok(s)
    }

    pub fn apply(&mut self, p: StepInput) -> AppResult<()> {
        if let Some(v) = p.phase {
            self.phase = v;
        }
        if let Some(v) = p.title {
            self.title = v.trim().to_owned();
        }
        if p.instructions.is_some() {
            self.instructions = non_blank(p.instructions);
        }
        if let Some(v) = p.owner_role_id {
            self.owner_role_id = v;
        }
        if let Some(v) = p.expected_duration {
            self.expected_duration = v;
        }
        if p.verification.is_some() {
            self.verification = non_blank(p.verification);
        }
        if let Some(mut v) = p.depends_on {
            v.sort();
            v.dedup();
            self.depends_on = v;
        }
        if let Some(v) = p.is_decision_point {
            self.is_decision_point = v;
        }
        if let Some(v) = p.requires_authorization_role_id {
            self.requires_authorization_role_id = v;
        }
        let mut issues = Issues::new();
        issues.check(
            !self.title.is_empty(),
            "REQUIRED",
            "/title",
            "title is required",
        );
        issues.check(
            !self.depends_on.contains(&self.meta.id),
            "INVALID_VALUE",
            "/dependsOnStepIds",
            "a step cannot depend on itself",
        );
        issues.into_result()
    }
}

/// Validates the steps of one runbook: dependencies exist in the runbook and come earlier.
pub fn validate_steps(steps: &[RunbookStep]) -> AppResult<()> {
    let seq_of: HashMap<Uuid, u32> = steps.iter().map(|s| (s.meta.id, s.seq)).collect();
    let mut issues = Issues::new();
    for step in steps {
        for dep in &step.depends_on {
            match seq_of.get(dep) {
                None => issues.push(
                    Issue::blocking(
                        "UNKNOWN_STEP_DEPENDENCY",
                        format!(
                            "step `{}` depends on a step outside this runbook",
                            step.title
                        ),
                    )
                    .field("/dependsOnStepIds")
                    .entity("runbook_step", step.meta.id),
                ),
                Some(dep_seq) if *dep_seq >= step.seq => issues.push(
                    Issue::blocking(
                        "STEP_ORDER_VIOLATES_DEPENDENCY",
                        format!(
                            "step `{}` must come after the steps it depends on",
                            step.title
                        ),
                    )
                    .field("/dependsOnStepIds")
                    .entity("runbook_step", step.meta.id),
                ),
                Some(_) => {}
            }
        }
    }
    issues.into_result()
}

/// Effective predecessors of each step: explicit `depends_on`, else the previous step by `seq`.
pub fn effective_dependencies(steps: &[RunbookStep]) -> HashMap<Uuid, Vec<Uuid>> {
    let mut ordered: Vec<&RunbookStep> = steps.iter().collect();
    ordered.sort_by_key(|s| s.seq);
    let mut result = HashMap::new();
    let mut previous: Option<Uuid> = None;
    for step in ordered {
        let deps = if step.depends_on.is_empty() {
            previous.into_iter().collect()
        } else {
            step.depends_on.clone()
        };
        result.insert(step.meta.id, deps);
        previous = Some(step.meta.id);
    }
    result
}

/// Longest chain of expected durations (steps without a duration count as 0).
pub fn critical_path(steps: &[RunbookStep]) -> Minutes {
    critical_path_where(steps, |_| true)
}

/// Critical path over the steps accepted by `include` (e.g. the remaining steps of a run).
pub fn critical_path_where(
    steps: &[RunbookStep],
    include: impl Fn(&RunbookStep) -> bool,
) -> Minutes {
    let deps = effective_dependencies(steps);
    let mut ordered: Vec<&RunbookStep> = steps.iter().collect();
    ordered.sort_by_key(|s| s.seq);
    let mut finish: HashMap<Uuid, u64> = HashMap::new();
    for step in ordered {
        let start = deps
            .get(&step.meta.id)
            .into_iter()
            .flatten()
            .filter_map(|d| finish.get(d))
            .copied()
            .max()
            .unwrap_or(0);
        let own = if include(step) {
            step.expected_duration.map_or(0, |m| u64::from(m.get()))
        } else {
            0
        };
        finish.insert(step.meta.id, start + own);
    }
    Minutes::saturating(finish.values().copied().max().unwrap_or(0))
}

/// Ensures no other step depends on `step_id` before deleting it.
pub fn ensure_not_depended_on(steps: &[RunbookStep], step_id: Uuid) -> AppResult<()> {
    if steps.iter().any(|s| s.depends_on.contains(&step_id)) {
        return Err(AppError::conflict(
            "other steps depend on this step; remove the dependency first",
        ));
    }
    Ok(())
}

#[async_trait]
pub trait RunbookRepository: Send + Sync {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<Runbook>>;
    /// Runbooks of all microservices of a service, ordered by the microservices' restore order.
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<Runbook>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Runbook>>;
    async fn insert(
        &self,
        ctx: &TenantContext,
        runbook: &Runbook,
        steps: &[RunbookStep],
    ) -> AppResult<Runbook>;
    async fn update(&self, ctx: &TenantContext, runbook: &Runbook) -> AppResult<Runbook>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
    /// Steps of the given runbooks, ordered by runbook and `seq`.
    async fn steps(&self, ctx: &TenantContext, runbook_ids: &[Uuid])
    -> AppResult<Vec<RunbookStep>>;
    async fn get_step(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RunbookStep>>;
    /// Replaces all steps' ordering/content of one runbook in a single transaction
    /// (insert new, update changed, delete missing).
    async fn save_steps(
        &self,
        ctx: &TenantContext,
        runbook_id: Uuid,
        steps: &[RunbookStep],
        deleted: &[Uuid],
    ) -> AppResult<Vec<RunbookStep>>;
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

    fn step(seq: u32, minutes: u32, depends_on: Vec<Uuid>) -> RunbookStep {
        RunbookStep::create(
            &ctx(),
            Uuid::nil(),
            seq,
            RunbookPhase::Recovery,
            StepInput {
                title: Some(format!("step {seq}")),
                expected_duration: Some(Some(Minutes::new(minutes).expect("valid"))),
                depends_on: Some(depends_on),
                ..Default::default()
            },
        )
        .expect("valid")
    }

    #[test]
    fn sequential_steps_add_up() {
        let steps = vec![step(1, 10, vec![]), step(2, 20, vec![]), step(3, 5, vec![])];
        assert_eq!(critical_path(&steps).get(), 35);
    }

    #[test]
    fn explicit_dependencies_allow_parallel_work() {
        let a = step(1, 10, vec![]);
        let b = step(2, 30, vec![a.meta.id]);
        let c = step(3, 20, vec![a.meta.id]); // parallel to b
        let d = step(4, 5, vec![b.meta.id, c.meta.id]);
        let steps = vec![a, b, c, d];
        assert_eq!(critical_path(&steps).get(), 45);
        validate_steps(&steps).expect("valid order");
    }

    #[test]
    fn dependencies_must_precede() {
        let mut a = step(1, 10, vec![]);
        let b = step(2, 10, vec![]);
        a.depends_on = vec![b.meta.id];
        assert!(validate_steps(&[a, b]).is_err());
    }
}
