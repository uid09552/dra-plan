use std::sync::Arc;

use super::domain::{Tenant, TenantPatch, TenantRepository, TenantSettingsPatch};
use crate::features::directory::domain::{DEFAULT_ROLES, NewMember};
use crate::shared::kernel::{
    AppError, AppResult, Page, PageRequest, Principal, TenantContext, TenantId, TenantRole,
};

pub struct TenantUseCases {
    repo: Arc<dyn TenantRepository>,
}

impl TenantUseCases {
    pub fn new(repo: Arc<dyn TenantRepository>) -> Self {
        Self { repo }
    }

    pub async fn list_for(
        &self,
        principal: &Principal,
        page: PageRequest,
    ) -> AppResult<Page<Tenant>> {
        self.repo.list_for_user(&principal.user, page).await
    }

    /// Creates a tenant; the caller becomes its admin and the default DR roles are seeded.
    pub async fn create(
        &self,
        principal: &Principal,
        name: String,
        slug: String,
        settings: TenantSettingsPatch,
    ) -> AppResult<Tenant> {
        let tenant = Tenant::create(&principal.user, name, slug, settings)?;
        if self.repo.find_by_slug(&tenant.slug).await?.is_some() {
            return Err(AppError::invalid(
                "DUPLICATE",
                Some("/slug"),
                "a tenant with this slug already exists",
            ));
        }
        self.repo
            .create(&tenant, &admin_member(&principal.user), DEFAULT_ROLES)
            .await
    }

    pub async fn get_current(&self, ctx: &TenantContext) -> AppResult<Tenant> {
        self.repo
            .get(ctx)
            .await?
            .ok_or(AppError::NotFound("tenant"))
    }

    pub async fn update_current(
        &self,
        ctx: &TenantContext,
        if_match: Option<i32>,
        patch: TenantPatch,
    ) -> AppResult<Tenant> {
        let mut tenant = self.get_current(ctx).await?;
        tenant.meta.check_version(if_match)?;
        let slug_changed = patch.slug.as_ref().is_some_and(|s| s.trim() != tenant.slug);
        tenant.apply(patch)?;
        if slug_changed && self.repo.find_by_slug(&tenant.slug).await?.is_some() {
            return Err(AppError::invalid(
                "DUPLICATE",
                Some("/slug"),
                "a tenant with this slug already exists",
            ));
        }
        self.repo.update(ctx, &tenant).await
    }

    pub async fn delete_current(
        &self,
        ctx: &TenantContext,
        if_match: Option<i32>,
    ) -> AppResult<()> {
        let tenant = self.get_current(ctx).await?;
        tenant.meta.check_version(if_match)?;
        self.repo.delete(ctx).await
    }

    /// `--dev-mode`: makes sure the default tenant exists and the dev user is its admin.
    pub async fn ensure_dev_tenant(&self, slug: &str, user: &str) -> AppResult<TenantId> {
        let tenant = match self.repo.find_by_slug(slug).await? {
            Some(t) => t,
            None => {
                let name = format!("{}{} (dev)", slug[..1].to_uppercase(), &slug[1..]);
                let tenant =
                    Tenant::create(user, name, slug.to_owned(), TenantSettingsPatch::default())?;
                self.repo
                    .create(&tenant, &admin_member(user), DEFAULT_ROLES)
                    .await?
            }
        };
        self.repo
            .ensure_member(tenant.id(), &admin_member(user))
            .await?;
        Ok(tenant.id())
    }

    pub async fn find_by_slug(&self, slug: &str) -> AppResult<Option<Tenant>> {
        self.repo.find_by_slug(slug).await
    }
}

fn admin_member(user: &str) -> NewMember {
    NewMember {
        user_ref: user.to_owned(),
        email: None,
        display_name: None,
        tenant_role: TenantRole::Admin,
        person_id: None,
    }
}
