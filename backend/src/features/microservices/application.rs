use std::sync::Arc;

use uuid::Uuid;

use super::domain::{Microservice, MicroserviceInput, MicroserviceRepository};
use crate::features::it_services::application::ItServiceUseCases;
use crate::shared::kernel::{AppError, AppResult, TenantContext};

pub struct MicroserviceUseCases {
    repo: Arc<dyn MicroserviceRepository>,
    services: Arc<ItServiceUseCases>,
}

impl MicroserviceUseCases {
    pub fn new(repo: Arc<dyn MicroserviceRepository>, services: Arc<ItServiceUseCases>) -> Self {
        Self { repo, services }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<Microservice>> {
        self.services.require(ctx, service_id).await?;
        self.repo.list_by_service(ctx, service_id).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Microservice> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("microservice"))
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        input: MicroserviceInput,
    ) -> AppResult<Microservice> {
        self.services.require(ctx, service_id).await?;
        let m = Microservice::create(ctx, service_id, input)?;
        self.repo.insert(ctx, &m).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
        patch: MicroserviceInput,
    ) -> AppResult<Microservice> {
        let mut m = self.get(ctx, id).await?;
        m.meta.check_version(if_match)?;
        m.apply(patch)?;
        self.repo.update(ctx, &m).await
    }

    /// Deletes the microservice and its DR items. Fails (409) while other microservices depend on it.
    pub async fn delete(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
    ) -> AppResult<()> {
        let m = self.get(ctx, id).await?;
        m.meta.check_version(if_match)?;
        if m.is_default {
            return Err(AppError::conflict(
                "the default component stands for the whole service and cannot be deleted",
            ));
        }
        self.repo.delete(ctx, id).await
    }
}
