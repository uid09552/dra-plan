use std::sync::Arc;

use uuid::Uuid;

use super::domain::{Bia, BiaInput, BiaRepository};
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::tenants::domain::TenantRepository;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

pub struct BiaUseCases {
    repo: Arc<dyn BiaRepository>,
    services: Arc<ItServiceUseCases>,
    tenants: Arc<dyn TenantRepository>,
}

impl BiaUseCases {
    pub fn new(
        repo: Arc<dyn BiaRepository>,
        services: Arc<ItServiceUseCases>,
        tenants: Arc<dyn TenantRepository>,
    ) -> Self {
        Self {
            repo,
            services,
            tenants,
        }
    }

    /// Returns the BIA and the MTPD derived from the impact ratings (plausibility check).
    pub async fn get(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<(Bia, Option<Minutes>)> {
        self.services.require(ctx, service_id).await?;
        let bia = self
            .repo
            .get(ctx, service_id)
            .await?
            .ok_or(AppError::NotFound("business impact analysis"))?;
        let derived = self.derived(ctx, &bia).await?;
        Ok((bia, derived))
    }

    pub async fn put(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        if_match: Option<i32>,
        input: BiaInput,
    ) -> AppResult<(Bia, Option<Minutes>)> {
        self.services.require(ctx, service_id).await?;
        let existing = self.repo.get(ctx, service_id).await?;
        if let Some(b) = &existing {
            b.meta.check_version(if_match)?;
        }
        let is_new = existing.is_none();
        let bia = Bia::replace(ctx, existing, service_id, input)?;
        let saved = self.repo.save(ctx, &bia, is_new).await?;
        let derived = self.derived(ctx, &saved).await?;
        Ok((saved, derived))
    }

    async fn derived(&self, ctx: &TenantContext, bia: &Bia) -> AppResult<Option<Minutes>> {
        let tenant = self
            .tenants
            .get(ctx)
            .await?
            .ok_or(AppError::NotFound("tenant"))?;
        Ok(bia.derived_mtpd(tenant.settings.impact_tolerance_level))
    }
}
