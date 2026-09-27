use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    ItService, ItServiceInput, ItServiceRepository, ServiceFilter, ServiceSummary,
};
use crate::shared::kernel::{AppError, AppResult, Page, PageRequest, TenantContext};

pub struct ItServiceUseCases {
    repo: Arc<dyn ItServiceRepository>,
}

impl ItServiceUseCases {
    pub fn new(repo: Arc<dyn ItServiceRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        filter: &ServiceFilter,
        page: PageRequest,
    ) -> AppResult<Page<(ItService, ServiceSummary)>> {
        let page = self.repo.list(ctx, filter, page).await?;
        let ids: Vec<Uuid> = page.items.iter().map(|s| s.meta.id).collect();
        let mut summaries = self.repo.summaries(ctx, &ids).await?;
        Ok(page.map(|s| {
            let summary = summaries.remove(&s.meta.id).unwrap_or_default();
            (s, summary)
        }))
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<ItService> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("IT service"))
    }

    pub async fn get_with_summary(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> AppResult<(ItService, ServiceSummary)> {
        let service = self.get(ctx, id).await?;
        let summary = self
            .repo
            .summaries(ctx, &[id])
            .await?
            .remove(&id)
            .unwrap_or_default();
        Ok((service, summary))
    }

    pub async fn create(&self, ctx: &TenantContext, input: ItServiceInput) -> AppResult<ItService> {
        let service = ItService::create(ctx, input)?;
        self.repo.insert(ctx, &service).await
    }

    pub async fn update(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
        patch: ItServiceInput,
    ) -> AppResult<ItService> {
        let mut service = self.get(ctx, id).await?;
        service.meta.check_version(if_match)?;
        service.apply(patch)?;
        self.repo.update(ctx, &service).await
    }

    /// Fails while a recovery run is active; approved plan versions are kept (service id nulled).
    pub async fn delete(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
    ) -> AppResult<()> {
        let (service, summary) = self.get_with_summary(ctx, id).await?;
        service.meta.check_version(if_match)?;
        if summary.active_recovery_run_id.is_some() {
            return Err(AppError::conflict("the service has an active recovery run"));
        }
        self.repo.delete(ctx, id).await
    }

    /// Ensures the service exists in the caller's tenant (404 otherwise).
    pub async fn require(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.get(ctx, id).await.map(|_| ())
    }
}
