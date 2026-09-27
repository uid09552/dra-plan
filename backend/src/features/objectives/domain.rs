//! Recovery objectives (RTO, RPO, MTTR) per microservice, as a default or per scenario (workflow step 7).

use async_trait::async_trait;
use uuid::Uuid;

use crate::features::bia::domain::Bia;
use crate::shared::kernel::{AppResult, Issue, Issues, Meta, Minutes, Provenance, TenantContext};

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryObjective {
    pub meta: Meta,
    pub provenance: Provenance,
    pub microservice_id: Uuid,
    /// `None` = default objective for all scenarios.
    pub scenario_id: Option<Uuid>,
    pub rto: Minutes,
    pub rpo: Minutes,
    pub mttr_target: Option<Minutes>,
    pub restore_priority: Option<u32>,
    pub first_functions: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectiveInput {
    pub scenario_id: Option<Option<Uuid>>,
    pub rto: Option<Minutes>,
    pub rpo: Option<Minutes>,
    pub mttr_target: Option<Option<Minutes>>,
    pub restore_priority: Option<u32>,
    pub first_functions: Option<Vec<String>>,
}

impl RecoveryObjective {
    pub fn create(
        ctx: &TenantContext,
        microservice_id: Uuid,
        rto: Minutes,
        rpo: Minutes,
        input: ObjectiveInput,
    ) -> AppResult<Self> {
        let mut o = RecoveryObjective {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            microservice_id,
            scenario_id: None,
            rto,
            rpo,
            mttr_target: None,
            restore_priority: None,
            first_functions: Vec::new(),
        };
        o.apply(input)?;
        Ok(o)
    }

    pub fn apply(&mut self, p: ObjectiveInput) -> AppResult<()> {
        if let Some(v) = p.scenario_id {
            self.scenario_id = v;
        }
        if let Some(v) = p.rto {
            self.rto = v;
        }
        if let Some(v) = p.rpo {
            self.rpo = v;
        }
        if let Some(v) = p.mttr_target {
            self.mttr_target = v;
        }
        if p.restore_priority.is_some() {
            self.restore_priority = p.restore_priority;
        }
        if let Some(v) = p.first_functions {
            self.first_functions = v
                .into_iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
        }
        let mut issues = Issues::new();
        issues.check(
            self.restore_priority != Some(0),
            "OUT_OF_RANGE",
            "/restorePriority",
            "restore priority starts at 1",
        );
        issues.into_result()
    }

    /// Integrity rules 1 and 2: objective within the service targets from the BIA.
    pub fn check_against_bia(&self, bia: &Bia) -> Vec<Issue> {
        let mut issues = Vec::new();
        if self.rto > bia.service_rto {
            issues.push(
                Issue::blocking(
                    "RTO_EXCEEDS_SERVICE_RTO",
                    format!(
                        "RTO {} min exceeds the service RTO {} min",
                        self.rto.get(),
                        bia.service_rto.get()
                    ),
                )
                .field("/rtoMinutes")
                .entity("recovery_objective", self.meta.id)
                .standard("NIST SP 800-34 §3.2 / BSI 200-4 WAZ"),
            );
        }
        if self.rpo > bia.service_rpo {
            issues.push(
                Issue::blocking(
                    "RPO_EXCEEDS_SERVICE_RPO",
                    format!(
                        "RPO {} min exceeds the service RPO {} min",
                        self.rpo.get(),
                        bia.service_rpo.get()
                    ),
                )
                .field("/rpoMinutes")
                .entity("recovery_objective", self.meta.id)
                .standard("NIST SP 800-34 §3.2 / BSI 200-4 MTDV"),
            );
        }
        issues
    }
}

/// The objective that applies to a microservice in a scenario: scenario-specific, else the default.
pub fn effective(
    objectives: &[RecoveryObjective],
    microservice_id: Uuid,
    scenario_id: Option<Uuid>,
) -> Option<&RecoveryObjective> {
    let own = || {
        objectives
            .iter()
            .filter(move |o| o.microservice_id == microservice_id)
    };
    scenario_id
        .and_then(|sid| own().find(|o| o.scenario_id == Some(sid)))
        .or_else(|| own().find(|o| o.scenario_id.is_none()))
}

#[async_trait]
pub trait ObjectiveRepository: Send + Sync {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<RecoveryObjective>>;
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RecoveryObjective>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryObjective>>;
    async fn insert(
        &self,
        ctx: &TenantContext,
        o: &RecoveryObjective,
    ) -> AppResult<RecoveryObjective>;
    async fn update(
        &self,
        ctx: &TenantContext,
        o: &RecoveryObjective,
    ) -> AppResult<RecoveryObjective>;
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

    fn objective(ms: Uuid, scenario: Option<Uuid>, rto: u32) -> RecoveryObjective {
        let m = |v| Minutes::new(v).expect("valid");
        RecoveryObjective::create(
            &ctx(),
            ms,
            m(rto),
            m(15),
            ObjectiveInput {
                scenario_id: Some(scenario),
                ..Default::default()
            },
        )
        .expect("valid")
    }

    #[test]
    fn prefers_scenario_specific_objective() {
        let ms = Uuid::new_v4();
        let sc = Uuid::new_v4();
        let all = vec![objective(ms, None, 240), objective(ms, Some(sc), 60)];
        assert_eq!(effective(&all, ms, Some(sc)).map(|o| o.rto.get()), Some(60));
        assert_eq!(
            effective(&all, ms, Some(Uuid::new_v4())).map(|o| o.rto.get()),
            Some(240)
        );
        assert_eq!(effective(&all, ms, None).map(|o| o.rto.get()), Some(240));
        assert!(effective(&all, Uuid::new_v4(), None).is_none());
    }
}
