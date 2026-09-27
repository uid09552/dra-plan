use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    DataProtection, DataProtectionInput, DataProtectionMethod, DataProtectionRepository,
};
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::objectives::domain::ObjectiveRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

pub struct DataProtectionUseCases {
    repo: Arc<dyn DataProtectionRepository>,
    microservices: Arc<MicroserviceUseCases>,
    objectives: Arc<dyn ObjectiveRepository>,
}

impl DataProtectionUseCases {
    pub fn new(
        repo: Arc<dyn DataProtectionRepository>,
        microservices: Arc<MicroserviceUseCases>,
        objectives: Arc<dyn ObjectiveRepository>,
    ) -> Self {
        Self {
            repo,
            microservices,
            objectives,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<(DataProtection, Option<bool>)>> {
        self.microservices.get(ctx, microservice_id).await?;
        let items = self.repo.list_by_microservice(ctx, microservice_id).await?;
        let objectives = self
            .objectives
            .list_by_microservice(ctx, microservice_id)
            .await?;
        Ok(items
            .into_iter()
            .map(|d| {
                let s = d.supports_rpo(&objectives);
                (d, s)
            })
            .collect())
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        method: DataProtectionMethod,
        frequency: Minutes,
        input: DataProtectionInput,
    ) -> AppResult<(DataProtection, Option<bool>)> {
        self.microservices.get(ctx, microservice_id).await?;
        let d = DataProtection::create(ctx, microservice_id, method, frequency, input)?;
        let saved = self.repo.insert(ctx, &d).await?;
        self.with_rpo(ctx, saved).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: DataProtectionInput,
    ) -> AppResult<(DataProtection, Option<bool>)> {
        let mut d = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("data protection entry"))?;
        d.apply(patch)?;
        let saved = self.repo.update(ctx, &d).await?;
        self.with_rpo(ctx, saved).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        if self.repo.delete(ctx, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("data protection entry"))
        }
    }

    async fn with_rpo(
        &self,
        ctx: &TenantContext,
        d: DataProtection,
    ) -> AppResult<(DataProtection, Option<bool>)> {
        let objectives = self
            .objectives
            .list_by_microservice(ctx, d.microservice_id)
            .await?;
        let supports = d.supports_rpo(&objectives);
        Ok((d, supports))
    }
}
