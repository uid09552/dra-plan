use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    Dependency, DependencyCriticality, DependencyDirection, DependencyGraph, DependencyInput,
    DependencyKind, DependencyRepository, build_graph, graph_components,
};
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::microservices::domain::MicroserviceRepository;
use crate::features::objectives::domain::{ObjectiveRepository, RecoveryObjective};
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

/// A dependency with its computed recovery-time properties.
pub struct DependencyView {
    pub dependency: Dependency,
    pub effective_rto: Option<Minutes>,
    pub rto_conflict: bool,
}

pub struct DependencyUseCases {
    repo: Arc<dyn DependencyRepository>,
    services: Arc<ItServiceUseCases>,
    microservices: Arc<MicroserviceUseCases>,
    microservice_repo: Arc<dyn MicroserviceRepository>,
    objectives: Arc<dyn ObjectiveRepository>,
}

impl DependencyUseCases {
    pub fn new(
        repo: Arc<dyn DependencyRepository>,
        services: Arc<ItServiceUseCases>,
        microservices: Arc<MicroserviceUseCases>,
        microservice_repo: Arc<dyn MicroserviceRepository>,
        objectives: Arc<dyn ObjectiveRepository>,
    ) -> Self {
        Self {
            repo,
            services,
            microservices,
            microservice_repo,
            objectives,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<DependencyView>> {
        let ms = self.microservices.get(ctx, microservice_id).await?;
        let deps = self.repo.list_by_microservice(ctx, microservice_id).await?;
        let objectives = self.objectives_for(ctx, ms.service_id, &deps).await?;
        Ok(deps.into_iter().map(|d| view(d, &objectives)).collect())
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        kind: DependencyKind,
        direction: DependencyDirection,
        criticality: DependencyCriticality,
        input: DependencyInput,
    ) -> AppResult<DependencyView> {
        let ms = self.microservices.get(ctx, microservice_id).await?;
        let dep = Dependency::create(ctx, microservice_id, kind, direction, criticality, input)?;
        let saved = self.repo.insert(ctx, &dep).await?;
        let objectives = self
            .objectives_for(ctx, ms.service_id, std::slice::from_ref(&saved))
            .await?;
        Ok(view(saved, &objectives))
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: DependencyInput,
    ) -> AppResult<DependencyView> {
        let mut dep = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("dependency"))?;
        let ms = self.microservices.get(ctx, dep.microservice_id).await?;
        dep.apply(patch)?;
        let saved = self.repo.update(ctx, &dep).await?;
        let objectives = self
            .objectives_for(ctx, ms.service_id, std::slice::from_ref(&saved))
            .await?;
        Ok(view(saved, &objectives))
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("dependency"))?;
        self.repo.delete(ctx, id).await
    }

    pub async fn graph(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<DependencyGraph> {
        self.services.require(ctx, service_id).await?;
        let microservices = self
            .microservice_repo
            .list_by_service(ctx, service_id)
            .await?;
        let deps = self.repo.list_by_service(ctx, service_id).await?;
        let microservices = graph_components(microservices, &deps);
        let own: HashSet<Uuid> = microservices.iter().map(|m| m.meta.id).collect();
        let mut external = Vec::new();
        for target in deps
            .iter()
            .filter_map(|d| d.target_microservice_id)
            .filter(|t| !own.contains(t))
            .collect::<HashSet<_>>()
        {
            if let Some(m) = self.microservice_repo.get(ctx, target).await? {
                external.push(m);
            }
        }
        let objectives = self.objectives_for(ctx, service_id, &deps).await?;
        Ok(build_graph(&microservices, &external, &deps, &objectives))
    }

    /// Objectives of the service plus those of external target microservices.
    async fn objectives_for(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        deps: &[Dependency],
    ) -> AppResult<Vec<RecoveryObjective>> {
        let mut objectives = self.objectives.list_by_service(ctx, service_id).await?;
        let known: HashSet<Uuid> = objectives.iter().map(|o| o.microservice_id).collect();
        for target in deps
            .iter()
            .filter_map(|d| d.target_microservice_id)
            .filter(|t| !known.contains(t))
            .collect::<HashSet<_>>()
        {
            objectives.extend(self.objectives.list_by_microservice(ctx, target).await?);
        }
        Ok(objectives)
    }
}

fn view(dependency: Dependency, objectives: &[RecoveryObjective]) -> DependencyView {
    DependencyView {
        effective_rto: dependency.effective_rto(objectives),
        rto_conflict: dependency.rto_conflict(objectives),
        dependency,
    }
}
