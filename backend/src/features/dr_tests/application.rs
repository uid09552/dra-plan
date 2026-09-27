use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::domain::{DrTest, DrTestInput, DrTestRepository, DrTestResult, DrTestType};
use crate::features::microservices::domain::MicroserviceRepository;
use crate::features::objectives::domain::{ObjectiveRepository, RecoveryObjective, effective};
use crate::features::plans::application::PlanUseCases;
use crate::features::scenarios::domain::ScenarioRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

/// Measured values as entered by users; targets are computed.
pub struct ResultInput {
    pub microservice_id: Uuid,
    pub achieved_rto: Option<Minutes>,
    pub achieved_rpo: Option<Minutes>,
    pub notes: Option<String>,
}

pub struct DrTestUseCases {
    repo: Arc<dyn DrTestRepository>,
    plans: Arc<PlanUseCases>,
    scenarios: Arc<dyn ScenarioRepository>,
    microservices: Arc<dyn MicroserviceRepository>,
    objectives: Arc<dyn ObjectiveRepository>,
}

impl DrTestUseCases {
    pub fn new(
        repo: Arc<dyn DrTestRepository>,
        plans: Arc<PlanUseCases>,
        scenarios: Arc<dyn ScenarioRepository>,
        microservices: Arc<dyn MicroserviceRepository>,
        objectives: Arc<dyn ObjectiveRepository>,
    ) -> Self {
        Self {
            repo,
            plans,
            scenarios,
            microservices,
            objectives,
        }
    }

    pub async fn list(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Vec<DrTest>> {
        self.plans.loader().require_service(ctx, service_id).await?;
        self.repo.list_by_service(ctx, service_id).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<DrTest> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("DR test"))
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        test_type: DrTestType,
        scenario_id: Uuid,
        planned_at: DateTime<Utc>,
        mut input: DrTestInput,
    ) -> AppResult<DrTest> {
        self.plans.loader().require_service(ctx, service_id).await?;
        if input.plan_version_id.is_none() {
            input.plan_version_id = self
                .plans
                .current_approved(ctx, service_id)
                .await?
                .map(|v| v.id);
        }
        let test = DrTest::create(ctx, service_id, test_type, scenario_id, planned_at, input)?;
        self.check_references(ctx, &test).await?;
        self.repo.insert(ctx, &test).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: DrTestInput,
    ) -> AppResult<DrTest> {
        let mut test = self.get(ctx, id).await?;
        test.apply(patch)?;
        self.check_references(ctx, &test).await?;
        self.repo.update(ctx, &test).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.get(ctx, id).await?.ensure_deletable()?;
        self.repo.delete(ctx, id).await
    }

    pub async fn results(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Vec<DrTestResult>> {
        self.get(ctx, id).await?;
        Ok(self
            .repo
            .results(ctx, &[id])
            .await?
            .into_iter()
            .map(|(_, r)| r)
            .collect())
    }

    /// Records measured values; targets come from the objectives of the tested plan version
    /// (or the live objectives if no plan version is linked).
    pub async fn record_results(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        inputs: Vec<ResultInput>,
    ) -> AppResult<Vec<DrTestResult>> {
        let test = self.get(ctx, id).await?;
        let own: HashSet<Uuid> = self
            .microservices
            .list_by_service(ctx, test.service_id)
            .await?
            .into_iter()
            .map(|m| m.meta.id)
            .collect();
        if inputs.iter().any(|i| !own.contains(&i.microservice_id)) {
            return Err(AppError::invalid(
                "REFERENCE_NOT_FOUND",
                Some("/microserviceId"),
                "microservices must belong to the tested IT service",
            ));
        }
        let objectives = self.target_objectives(ctx, &test).await?;
        let results: Vec<DrTestResult> = inputs
            .into_iter()
            .map(|i| to_result(i, &objectives, test.scenario_id))
            .collect();
        self.repo.replace_results(ctx, id, &results).await?;
        self.results(ctx, id).await
    }

    pub async fn target_objectives(
        &self,
        ctx: &TenantContext,
        test: &DrTest,
    ) -> AppResult<Vec<RecoveryObjective>> {
        match test.plan_version_id {
            Some(pv) => {
                let version = self.plans.get(ctx, pv).await?;
                Ok(self.plans.decode(&version)?.objectives())
            }
            None => self.objectives.list_by_service(ctx, test.service_id).await,
        }
    }

    async fn check_references(&self, ctx: &TenantContext, t: &DrTest) -> AppResult<()> {
        match self.scenarios.get(ctx, t.scenario_id).await? {
            Some(s) if s.service_id == t.service_id => {}
            _ => {
                return Err(AppError::invalid(
                    "REFERENCE_NOT_FOUND",
                    Some("/scenarioId"),
                    "the scenario must belong to the IT service",
                ));
            }
        }
        if let Some(pv) = t.plan_version_id {
            let version = self.plans.get(ctx, pv).await?;
            if version.service_id != Some(t.service_id) {
                return Err(AppError::invalid(
                    "REFERENCE_NOT_FOUND",
                    Some("/planVersionId"),
                    "the plan version must belong to the IT service",
                ));
            }
        }
        Ok(())
    }
}

pub fn to_result(
    i: ResultInput,
    objectives: &[RecoveryObjective],
    scenario_id: Uuid,
) -> DrTestResult {
    let target = effective(objectives, i.microservice_id, Some(scenario_id));
    DrTestResult {
        microservice_id: i.microservice_id,
        target_rto: target.map(|o| o.rto),
        target_rpo: target.map(|o| o.rpo),
        achieved_rto: i.achieved_rto,
        achieved_rpo: i.achieved_rpo,
        notes: i.notes,
    }
}
