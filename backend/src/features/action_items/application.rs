use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    ActionItem, ActionItemFilter, ActionItemInput, ActionItemRepository, ActionItemSource,
};
use crate::features::it_services::application::ItServiceUseCases;
use crate::shared::kernel::{AppError, AppResult, TenantContext};

pub struct ActionItemUseCases {
    repo: Arc<dyn ActionItemRepository>,
    services: Arc<ItServiceUseCases>,
}

impl ActionItemUseCases {
    pub fn new(repo: Arc<dyn ActionItemRepository>, services: Arc<ItServiceUseCases>) -> Self {
        Self { repo, services }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        filter: &ActionItemFilter,
    ) -> AppResult<Vec<ActionItem>> {
        self.services.require(ctx, service_id).await?;
        self.repo.list_by_service(ctx, service_id, filter).await
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        input: ActionItemInput,
    ) -> AppResult<ActionItem> {
        self.services.require(ctx, service_id).await?;
        let item = ActionItem::create(ctx, service_id, ActionItemSource::Manual, None, input)?;
        self.repo.insert(ctx, &item).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: ActionItemInput,
    ) -> AppResult<ActionItem> {
        let mut item = self
            .repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("action item"))?;
        item.apply(patch)?;
        self.repo.update(ctx, &item).await
    }
}
