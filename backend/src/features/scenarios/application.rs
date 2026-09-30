use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    Scenario, ScenarioDecision, ScenarioFilter, ScenarioInput, ScenarioRepository, check_parent,
};
use crate::features::catalog::domain::{
    ScenarioTemplate, ServiceProfile, scenario_template, suggest,
};
use crate::features::dependencies::domain::DependencyRepository;
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::microservices::domain::MicroserviceRepository;
use crate::shared::kernel::{AppError, AppResult, Language, TenantContext};

pub struct ScenarioUseCases {
    repo: Arc<dyn ScenarioRepository>,
    services: Arc<ItServiceUseCases>,
    microservices: Arc<dyn MicroserviceRepository>,
    dependencies: Arc<dyn DependencyRepository>,
}

impl ScenarioUseCases {
    pub fn new(
        repo: Arc<dyn ScenarioRepository>,
        services: Arc<ItServiceUseCases>,
        microservices: Arc<dyn MicroserviceRepository>,
        dependencies: Arc<dyn DependencyRepository>,
    ) -> Self {
        Self {
            repo,
            services,
            microservices,
            dependencies,
        }
    }

    /// Catalog scenarios relevant for the service's profile that are not captured yet.
    pub async fn suggestions(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<&'static ScenarioTemplate>> {
        let service = self.services.get(ctx, service_id).await?;
        let microservices = self.microservices.list_by_service(ctx, service_id).await?;
        let dependencies = self.dependencies.list_by_service(ctx, service_id).await?;
        let existing: Vec<String> = self
            .repo
            .list_by_service(ctx, service_id, &ScenarioFilter::default())
            .await?
            .into_iter()
            .map(|s| s.title)
            .collect();
        let profile = ServiceProfile::derive(&service, &microservices, &dependencies);
        Ok(suggest(&profile, &existing))
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        filter: &ScenarioFilter,
    ) -> AppResult<Vec<Scenario>> {
        self.services.require(ctx, service_id).await?;
        self.repo.list_by_service(ctx, service_id, filter).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Scenario> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("scenario"))
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        mut input: ScenarioInput,
        template_id: Option<&str>,
    ) -> AppResult<Scenario> {
        self.services.require(ctx, service_id).await?;
        if let Some(id) = template_id {
            let template = scenario_template(id).ok_or_else(|| {
                AppError::invalid(
                    "UNKNOWN_TEMPLATE",
                    Some("/catalogTemplateId"),
                    "unknown catalog template",
                )
            })?;
            input.title = input
                .title
                .or_else(|| Some(template.title(Language::En).to_owned()));
            input.description = input
                .description
                .or_else(|| Some(template.description(Language::En).to_owned()));
            input.category = input
                .category
                .or_else(|| Some(template.category.to_string()));
        }
        let scenario = Scenario::create(ctx, service_id, input)?;
        self.check_microservices(ctx, &scenario).await?;
        self.check_parent(ctx, &scenario).await?;
        self.repo.insert(ctx, &scenario).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
        patch: ScenarioInput,
    ) -> AppResult<Scenario> {
        let mut scenario = self.get(ctx, id).await?;
        scenario.meta.check_version(if_match)?;
        scenario.apply(patch)?;
        self.check_microservices(ctx, &scenario).await?;
        self.check_parent(ctx, &scenario).await?;
        single(self.repo.update_all(ctx, &[scenario]).await?)
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.get(ctx, id).await?;
        self.repo.delete(ctx, id).await
    }

    pub async fn merge(
        &self,
        ctx: &TenantContext,
        target_id: Uuid,
        source_ids: &[Uuid],
        merged_description: Option<String>,
    ) -> AppResult<Scenario> {
        let mut target = self.get(ctx, target_id).await?;
        let unique: HashSet<Uuid> = source_ids.iter().copied().collect();
        let mut sources = self
            .repo
            .get_many(ctx, &unique.iter().copied().collect::<Vec<_>>())
            .await?;
        if sources.len() != unique.len() {
            return Err(AppError::invalid(
                "REFERENCE_NOT_FOUND",
                Some("/sourceScenarioIds"),
                "unknown source scenario",
            ));
        }
        target.absorb(&mut sources, merged_description)?;
        let mut all = vec![target];
        all.extend(sources);
        let saved = self.repo.update_all(ctx, &all).await?;
        saved
            .into_iter()
            .find(|s| s.meta.id == target_id)
            .ok_or_else(|| AppError::internal("merge target missing"))
    }

    pub async fn decide(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        decision: ScenarioDecision,
    ) -> AppResult<Scenario> {
        let mut scenario = self.get(ctx, id).await?;
        scenario.decide(ctx.actor(), decision)?;
        self.check_microservices(ctx, &scenario).await?;
        single(self.repo.update_all(ctx, &[scenario]).await?)
    }

    /// Affected microservices must belong to the scenario's IT service.
    /// Sub-scenarios: the parent must be an unmerged scenario of the same service, without cycles.
    async fn check_parent(&self, ctx: &TenantContext, s: &Scenario) -> AppResult<()> {
        let Some(parent_id) = s.parent_scenario_id else {
            return Ok(());
        };
        let siblings = self
            .repo
            .list_by_service(ctx, s.service_id, &ScenarioFilter::default())
            .await?;
        check_parent(s, parent_id, &siblings)
    }

    async fn check_microservices(&self, ctx: &TenantContext, s: &Scenario) -> AppResult<()> {
        if s.affected_microservice_ids.is_empty() {
            return Ok(());
        }
        let own: HashSet<Uuid> = self
            .microservices
            .list_by_service(ctx, s.service_id)
            .await?
            .into_iter()
            .map(|m| m.meta.id)
            .collect();
        if s.affected_microservice_ids
            .iter()
            .all(|id| own.contains(id))
        {
            Ok(())
        } else {
            Err(AppError::invalid(
                "REFERENCE_NOT_FOUND",
                Some("/affectedMicroserviceIds"),
                "affected microservices must belong to the scenario's IT service",
            ))
        }
    }
}

/// The single scenario returned by an update of one scenario.
fn single(mut saved: Vec<Scenario>) -> AppResult<Scenario> {
    saved
        .pop()
        .ok_or_else(|| AppError::internal("update returned nothing"))
}
