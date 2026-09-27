use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    GapCheck, GapStatus, RecoveryStrategy, StrategyInput, StrategyRepository, StrategyType,
};
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::microservices::domain::Microservice;
use crate::features::objectives::domain::ObjectiveRepository;
use crate::features::scenarios::domain::ScenarioRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

pub struct StrategyUseCases {
    repo: Arc<dyn StrategyRepository>,
    microservices: Arc<MicroserviceUseCases>,
    scenarios: Arc<dyn ScenarioRepository>,
    objectives: Arc<dyn ObjectiveRepository>,
}

impl StrategyUseCases {
    pub fn new(
        repo: Arc<dyn StrategyRepository>,
        microservices: Arc<MicroserviceUseCases>,
        scenarios: Arc<dyn ScenarioRepository>,
        objectives: Arc<dyn ObjectiveRepository>,
    ) -> Self {
        Self {
            repo,
            microservices,
            scenarios,
            objectives,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<(RecoveryStrategy, GapCheck)>> {
        self.microservices.get(ctx, microservice_id).await?;
        let strategies = self
            .repo
            .list_by_microservice(ctx, microservice_id, scenario_id)
            .await?;
        let objectives = self
            .objectives
            .list_by_microservice(ctx, microservice_id)
            .await?;
        Ok(strategies
            .into_iter()
            .map(|s| {
                let g = s.gap_check(&objectives);
                (s, g)
            })
            .collect())
    }

    /// All measures of an IT service (every component), with gap checks.
    pub async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<(RecoveryStrategy, GapCheck)>> {
        self.microservices.list(ctx, service_id).await?;
        let strategies = self.repo.list_by_service(ctx, service_id).await?;
        let objectives = self.objectives.list_by_service(ctx, service_id).await?;
        Ok(strategies
            .into_iter()
            .map(|s| {
                let g = s.gap_check(&objectives);
                (s, g)
            })
            .collect())
    }

    pub async fn get(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> AppResult<(RecoveryStrategy, GapCheck)> {
        let s = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery strategy"))?;
        self.with_gap(ctx, s).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_ids: Vec<Uuid>,
        strategy_type: StrategyType,
        estimated_rto: Minutes,
        estimated_rpo: Minutes,
        input: StrategyInput,
    ) -> AppResult<(RecoveryStrategy, GapCheck)> {
        let ms = self.microservices.get(ctx, microservice_id).await?;
        self.check_scenarios(ctx, &ms, &scenario_ids).await?;
        let s = RecoveryStrategy::create(
            ctx,
            microservice_id,
            scenario_ids,
            strategy_type,
            estimated_rto,
            estimated_rpo,
            input,
        )?;
        let saved = self.repo.insert(ctx, &s).await?;
        self.with_gap(ctx, saved).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: StrategyInput,
    ) -> AppResult<(RecoveryStrategy, GapCheck)> {
        let mut s = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery strategy"))?;
        let ms = self.microservices.get(ctx, s.microservice_id).await?;
        let before = s.scenario_ids.clone();
        s.apply(patch)?;
        self.check_scenarios(ctx, &ms, &s.scenario_ids).await?;
        if s.is_selected && s.scenario_ids != before {
            // A selected measure now covers other scenarios: the selection rules apply again
            // (an unaccepted gap blocks; other selected measures of those scenarios are deselected).
            let (_, gap) = self.with_gap(ctx, s.clone()).await?;
            if gap.status == GapStatus::Gap && s.accepted_gap_action_item_id.is_none() {
                return Err(AppError::invalid(
                    "STRATEGY_GAP",
                    Some("/scenarioIds"),
                    "the measure misses the objective of a newly covered scenario; select it again with acceptGap",
                ));
            }
            let saved = self.repo.select(ctx, &s, None).await?;
            return self.with_gap(ctx, saved).await;
        }
        let saved = self.repo.update(ctx, &s).await?;
        self.with_gap(ctx, saved).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("recovery strategy"))?;
        self.repo.delete(ctx, id).await
    }

    pub async fn select(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        accept_gap: bool,
        justification: Option<String>,
    ) -> AppResult<(RecoveryStrategy, GapCheck)> {
        let (mut s, gap) = self.get(ctx, id).await?;
        let ms = self.microservices.get(ctx, s.microservice_id).await?;
        let item = s.select(ctx, ms.service_id, &gap, accept_gap, justification)?;
        let saved = self.repo.select(ctx, &s, item.as_ref()).await?;
        self.with_gap(ctx, saved).await
    }

    async fn with_gap(
        &self,
        ctx: &TenantContext,
        s: RecoveryStrategy,
    ) -> AppResult<(RecoveryStrategy, GapCheck)> {
        let objectives = self
            .objectives
            .list_by_microservice(ctx, s.microservice_id)
            .await?;
        let gap = s.gap_check(&objectives);
        Ok((s, gap))
    }

    async fn check_scenarios(
        &self,
        ctx: &TenantContext,
        ms: &Microservice,
        scenario_ids: &[Uuid],
    ) -> AppResult<()> {
        for id in scenario_ids {
            match self.scenarios.get(ctx, *id).await? {
                Some(s) if s.service_id == ms.service_id => {}
                _ => {
                    return Err(AppError::invalid(
                        "REFERENCE_NOT_FOUND",
                        Some("/scenarioIds"),
                        "every scenario must belong to the component's IT service",
                    ));
                }
            }
        }
        Ok(())
    }
}
