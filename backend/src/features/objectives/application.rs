use std::sync::Arc;

use uuid::Uuid;

use super::domain::{ObjectiveInput, ObjectiveRepository, RecoveryObjective};
use crate::features::bia::domain::BiaRepository;
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::microservices::domain::Microservice;
use crate::features::scenarios::domain::ScenarioRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

pub struct ObjectiveUseCases {
    repo: Arc<dyn ObjectiveRepository>,
    microservices: Arc<MicroserviceUseCases>,
    scenarios: Arc<dyn ScenarioRepository>,
    bia: Arc<dyn BiaRepository>,
}

impl ObjectiveUseCases {
    pub fn new(
        repo: Arc<dyn ObjectiveRepository>,
        microservices: Arc<MicroserviceUseCases>,
        scenarios: Arc<dyn ScenarioRepository>,
        bia: Arc<dyn BiaRepository>,
    ) -> Self {
        Self {
            repo,
            microservices,
            scenarios,
            bia,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<RecoveryObjective>> {
        self.microservices.get(ctx, microservice_id).await?;
        self.repo.list_by_microservice(ctx, microservice_id).await
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        rto: Minutes,
        rpo: Minutes,
        input: ObjectiveInput,
    ) -> AppResult<RecoveryObjective> {
        let ms = self.microservices.get(ctx, microservice_id).await?;
        let objective = RecoveryObjective::create(ctx, microservice_id, rto, rpo, input)?;
        self.validate(ctx, &ms, &objective).await?;
        self.repo.insert(ctx, &objective).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: ObjectiveInput,
    ) -> AppResult<RecoveryObjective> {
        let mut objective = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery objective"))?;
        let ms = self
            .microservices
            .get(ctx, objective.microservice_id)
            .await?;
        objective.apply(patch)?;
        self.validate(ctx, &ms, &objective).await?;
        self.repo.update(ctx, &objective).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery objective"))?;
        self.repo.delete(ctx, id).await
    }

    /// The scenario must belong to the same IT service; the objective must fit the BIA targets.
    async fn validate(
        &self,
        ctx: &TenantContext,
        ms: &Microservice,
        o: &RecoveryObjective,
    ) -> AppResult<()> {
        if let Some(scenario_id) = o.scenario_id {
            let scenario = self.scenarios.get(ctx, scenario_id).await?;
            if scenario.is_none_or(|s| s.service_id != ms.service_id) {
                return Err(AppError::invalid(
                    "REFERENCE_NOT_FOUND",
                    Some("/scenarioId"),
                    "the scenario must belong to the microservice's IT service",
                ));
            }
        }
        if let Some(bia) = self.bia.get(ctx, ms.service_id).await? {
            let issues = o.check_against_bia(&bia);
            if !issues.is_empty() {
                return Err(AppError::Validation(issues));
            }
        }
        Ok(())
    }
}
