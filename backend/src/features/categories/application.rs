use std::sync::Arc;

use uuid::Uuid;

use super::domain::{CustomCategory, CustomCategoryInput, CustomCategoryRepository};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

pub struct CategoryUseCases {
    repo: Arc<dyn CustomCategoryRepository>,
}

impl CategoryUseCases {
    pub fn new(repo: Arc<dyn CustomCategoryRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, ctx: &TenantContext) -> AppResult<Vec<CustomCategory>> {
        self.repo.list(ctx).await
    }

    pub async fn create(
        &self,
        ctx: &TenantContext,
        input: CustomCategoryInput,
    ) -> AppResult<CustomCategory> {
        let category = CustomCategory::create(ctx, input)?;
        if self
            .repo
            .list(ctx)
            .await?
            .iter()
            .any(|c| c.key == category.key)
        {
            return Err(AppError::invalid(
                "DUPLICATE",
                Some("/key"),
                "a category with this key already exists",
            ));
        }
        self.repo.insert(ctx, &category).await
    }

    pub async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.repo
            .list(ctx)
            .await?
            .into_iter()
            .find(|c| c.meta.id == id)
            .ok_or(AppError::NotFound("category"))?;
        self.repo.delete(ctx, id).await
    }
}
