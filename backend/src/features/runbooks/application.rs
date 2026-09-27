use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    Runbook, RunbookInput, RunbookPhase, RunbookRepository, RunbookStep, StepInput, critical_path,
    ensure_not_depended_on, validate_steps,
};
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::microservices::domain::Microservice;
use crate::features::objectives::domain::{ObjectiveRepository, RecoveryObjective, effective};
use crate::features::scenarios::domain::ScenarioRepository;
use crate::features::strategies::domain::StrategyRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

#[derive(Debug, Clone)]
pub struct RunbookSummary {
    pub step_count: usize,
    pub critical_path: Minutes,
    /// `None` when no objective applies.
    pub exceeds_rto: Option<bool>,
}

impl RunbookSummary {
    pub fn compute(
        runbook: &Runbook,
        steps: &[RunbookStep],
        objectives: &[RecoveryObjective],
    ) -> Self {
        let critical_path = critical_path(steps);
        let exceeds_rto = effective(
            objectives,
            runbook.microservice_id,
            Some(runbook.scenario_id),
        )
        .map(|o| critical_path > o.rto);
        RunbookSummary {
            step_count: steps.len(),
            critical_path,
            exceeds_rto,
        }
    }
}

pub struct RunbookView {
    pub runbook: Runbook,
    pub summary: RunbookSummary,
    pub steps: Vec<RunbookStep>,
}

/// New step as sent by clients (inline on runbook creation or added later).
pub struct NewStep {
    pub phase: RunbookPhase,
    pub position: Option<u32>,
    pub input: StepInput,
}

pub struct RunbookUseCases {
    repo: Arc<dyn RunbookRepository>,
    services: Arc<ItServiceUseCases>,
    microservices: Arc<MicroserviceUseCases>,
    scenarios: Arc<dyn ScenarioRepository>,
    strategies: Arc<dyn StrategyRepository>,
    objectives: Arc<dyn ObjectiveRepository>,
}

impl RunbookUseCases {
    pub fn new(
        repo: Arc<dyn RunbookRepository>,
        services: Arc<ItServiceUseCases>,
        microservices: Arc<MicroserviceUseCases>,
        scenarios: Arc<dyn ScenarioRepository>,
        strategies: Arc<dyn StrategyRepository>,
        objectives: Arc<dyn ObjectiveRepository>,
    ) -> Self {
        Self {
            repo,
            services,
            microservices,
            scenarios,
            strategies,
            objectives,
        }
    }

    pub async fn list_for_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<RunbookView>> {
        self.microservices.get(ctx, microservice_id).await?;
        let runbooks = self
            .repo
            .list_by_microservice(ctx, microservice_id, scenario_id)
            .await?;
        let objectives = self
            .objectives
            .list_by_microservice(ctx, microservice_id)
            .await?;
        self.views(ctx, runbooks, &objectives).await
    }

    pub async fn list_for_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<RunbookView>> {
        self.services.require(ctx, service_id).await?;
        let runbooks = self
            .repo
            .list_by_service(ctx, service_id, scenario_id)
            .await?;
        let objectives = self.objectives.list_by_service(ctx, service_id).await?;
        self.views(ctx, runbooks, &objectives).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<RunbookView> {
        let runbook = self.load(ctx, id).await?;
        let objectives = self
            .objectives
            .list_by_microservice(ctx, runbook.microservice_id)
            .await?;
        Ok(self.views(ctx, vec![runbook], &objectives).await?.remove(0))
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Uuid,
        input: RunbookInput,
        new_steps: Vec<NewStep>,
    ) -> AppResult<RunbookView> {
        let ms = self.microservices.get(ctx, microservice_id).await?;
        let runbook = Runbook::create(ctx, microservice_id, scenario_id, input)?;
        self.check_references(ctx, &ms, &runbook).await?;
        let mut steps = Vec::with_capacity(new_steps.len());
        for (i, s) in new_steps.into_iter().enumerate() {
            steps.push(RunbookStep::create(
                ctx,
                runbook.meta.id,
                (i + 1) as u32,
                s.phase,
                s.input,
            )?);
        }
        validate_steps(&steps)?;
        let saved = self.repo.insert(ctx, &runbook, &steps).await?;
        self.get(ctx, saved.meta.id).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
        patch: RunbookInput,
    ) -> AppResult<RunbookView> {
        let mut runbook = self.load(ctx, id).await?;
        runbook.meta.check_version(if_match)?;
        let ms = self.microservices.get(ctx, runbook.microservice_id).await?;
        runbook.apply(patch)?;
        self.check_references(ctx, &ms, &runbook).await?;
        self.repo.update(ctx, &runbook).await?;
        self.get(ctx, id).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.load(ctx, id).await?;
        self.repo.delete(ctx, id).await
    }

    pub async fn steps(
        &self,
        ctx: &TenantContext,
        runbook_id: Uuid,
    ) -> AppResult<Vec<RunbookStep>> {
        self.load(ctx, runbook_id).await?;
        self.repo.steps(ctx, &[runbook_id]).await
    }

    /// Inserts a step at `position` (1-based; default: at the end) and renumbers the others.
    pub async fn add_step(
        &self,
        ctx: &TenantContext,
        runbook_id: Uuid,
        new: NewStep,
    ) -> AppResult<RunbookStep> {
        self.load(ctx, runbook_id).await?;
        let mut steps = self.repo.steps(ctx, &[runbook_id]).await?;
        let position = new
            .position
            .unwrap_or(steps.len() as u32 + 1)
            .clamp(1, steps.len() as u32 + 1);
        let step = RunbookStep::create(ctx, runbook_id, position, new.phase, new.input)?;
        let id = step.meta.id;
        steps.insert(position as usize - 1, step);
        renumber(&mut steps);
        validate_steps(&steps)?;
        let saved = self.repo.save_steps(ctx, runbook_id, &steps, &[]).await?;
        saved
            .into_iter()
            .find(|s| s.meta.id == id)
            .ok_or_else(|| AppError::internal("inserted step missing"))
    }

    pub async fn update_step(
        &self,
        ctx: &TenantContext,
        step_id: Uuid,
        patch: StepInput,
    ) -> AppResult<RunbookStep> {
        let current = self
            .repo
            .get_step(ctx, step_id)
            .await?
            .ok_or(AppError::NotFound("runbook step"))?;
        let mut steps = self.repo.steps(ctx, &[current.runbook_id]).await?;
        let step = steps
            .iter_mut()
            .find(|s| s.meta.id == step_id)
            .ok_or(AppError::NotFound("runbook step"))?;
        step.apply(patch)?;
        validate_steps(&steps)?;
        let saved = self
            .repo
            .save_steps(ctx, current.runbook_id, &steps, &[])
            .await?;
        saved
            .into_iter()
            .find(|s| s.meta.id == step_id)
            .ok_or_else(|| AppError::internal("updated step missing"))
    }

    pub async fn delete_step(&self, ctx: &TenantContext, step_id: Uuid) -> AppResult<()> {
        let current = self
            .repo
            .get_step(ctx, step_id)
            .await?
            .ok_or(AppError::NotFound("runbook step"))?;
        let mut steps = self.repo.steps(ctx, &[current.runbook_id]).await?;
        ensure_not_depended_on(&steps, step_id)?;
        steps.retain(|s| s.meta.id != step_id);
        renumber(&mut steps);
        self.repo
            .save_steps(ctx, current.runbook_id, &steps, &[step_id])
            .await?;
        Ok(())
    }

    /// Reorders all steps; must contain every step id exactly once.
    pub async fn reorder(
        &self,
        ctx: &TenantContext,
        runbook_id: Uuid,
        order: &[Uuid],
    ) -> AppResult<Vec<RunbookStep>> {
        self.load(ctx, runbook_id).await?;
        let steps = self.repo.steps(ctx, &[runbook_id]).await?;
        let requested: HashSet<Uuid> = order.iter().copied().collect();
        let existing: HashSet<Uuid> = steps.iter().map(|s| s.meta.id).collect();
        if requested != existing || order.len() != steps.len() {
            return Err(AppError::invalid(
                "INVALID_VALUE",
                Some("/stepIds"),
                "stepIds must contain every step of the runbook exactly once",
            ));
        }
        let mut by_id: HashMap<Uuid, RunbookStep> =
            steps.into_iter().map(|s| (s.meta.id, s)).collect();
        let mut reordered: Vec<RunbookStep> =
            order.iter().filter_map(|id| by_id.remove(id)).collect();
        renumber(&mut reordered);
        validate_steps(&reordered)?;
        self.repo.save_steps(ctx, runbook_id, &reordered, &[]).await
    }

    async fn load(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Runbook> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("runbook"))
    }

    async fn views(
        &self,
        ctx: &TenantContext,
        runbooks: Vec<Runbook>,
        objectives: &[RecoveryObjective],
    ) -> AppResult<Vec<RunbookView>> {
        let ids: Vec<Uuid> = runbooks.iter().map(|r| r.meta.id).collect();
        let mut steps_by_runbook: HashMap<Uuid, Vec<RunbookStep>> = HashMap::new();
        for step in self.repo.steps(ctx, &ids).await? {
            steps_by_runbook
                .entry(step.runbook_id)
                .or_default()
                .push(step);
        }
        Ok(runbooks
            .into_iter()
            .map(|runbook| {
                let steps = steps_by_runbook
                    .remove(&runbook.meta.id)
                    .unwrap_or_default();
                let summary = RunbookSummary::compute(&runbook, &steps, objectives);
                RunbookView {
                    runbook,
                    summary,
                    steps,
                }
            })
            .collect())
    }

    /// Scenario must belong to the microservice's service; the strategy to the same microservice and scenario.
    async fn check_references(
        &self,
        ctx: &TenantContext,
        ms: &Microservice,
        r: &Runbook,
    ) -> AppResult<()> {
        match self.scenarios.get(ctx, r.scenario_id).await? {
            Some(s) if s.service_id == ms.service_id => {}
            _ => {
                return Err(AppError::invalid(
                    "REFERENCE_NOT_FOUND",
                    Some("/scenarioId"),
                    "the scenario must belong to the microservice's IT service",
                ));
            }
        }
        if let Some(strategy_id) = r.strategy_id {
            match self.strategies.get(ctx, strategy_id).await? {
                Some(s) if s.microservice_id == r.microservice_id && s.covers(r.scenario_id) => {}
                _ => {
                    return Err(AppError::invalid(
                        "REFERENCE_NOT_FOUND",
                        Some("/strategyId"),
                        "the strategy must belong to the same component and cover the scenario",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn renumber(steps: &mut [RunbookStep]) {
    for (i, s) in steps.iter_mut().enumerate() {
        s.seq = (i + 1) as u32;
    }
}
