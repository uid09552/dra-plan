//! Tenants and tenant settings (NIST SP 800-34 step 1 policy / BSI 200-4 Initiierung).

use async_trait::async_trait;

use crate::features::directory::domain::NewMember;
use crate::shared::kernel::{
    AppResult, ImpactCategory, Issues, Language, Meta, Minutes, Page, PageRequest, Rating,
    TenantContext, TenantId,
};

#[derive(Debug, Clone, PartialEq)]
pub struct TenantSettings {
    pub review_interval_months: u32,
    pub impact_categories: Vec<ImpactCategory>,
    pub impact_time_windows: Vec<Minutes>,
    /// Impact level at which a disruption becomes intolerable (derives the MTPD).
    pub impact_tolerance_level: Rating,
    pub default_language: Language,
    pub ai_enabled: bool,
}

impl TenantSettings {
    pub fn defaults() -> AppResult<Self> {
        Ok(Self {
            review_interval_months: 12,
            impact_categories: ImpactCategory::ALL.to_vec(),
            impact_time_windows: [60, 240, 1440, 4320, 10080]
                .into_iter()
                .map(Minutes::new)
                .collect::<AppResult<_>>()?,
            impact_tolerance_level: Rating::new(3)?,
            default_language: Language::En,
            ai_enabled: true,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TenantSettingsPatch {
    pub review_interval_months: Option<u32>,
    pub impact_categories: Option<Vec<ImpactCategory>>,
    pub impact_time_windows: Option<Vec<Minutes>>,
    pub impact_tolerance_level: Option<Rating>,
    pub default_language: Option<Language>,
    pub ai_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tenant {
    /// `meta.tenant_id == meta.id` for tenants.
    pub meta: Meta,
    pub name: String,
    pub slug: String,
    pub settings: TenantSettings,
}

#[derive(Debug, Clone, Default)]
pub struct TenantPatch {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub settings: TenantSettingsPatch,
}

impl Tenant {
    pub fn create(
        actor: &str,
        name: String,
        slug: String,
        settings: TenantSettingsPatch,
    ) -> AppResult<Self> {
        let id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();
        let mut tenant = Tenant {
            meta: Meta {
                id,
                tenant_id: TenantId(id),
                version: 1,
                created_at: now,
                created_by: actor.to_owned(),
                updated_at: now,
                updated_by: actor.to_owned(),
            },
            name: String::new(),
            slug: String::new(),
            settings: TenantSettings::defaults()?,
        };
        tenant.apply(TenantPatch {
            name: Some(name),
            slug: Some(slug),
            settings,
        })?;
        Ok(tenant)
    }

    pub fn id(&self) -> TenantId {
        TenantId(self.meta.id)
    }

    pub fn apply(&mut self, p: TenantPatch) -> AppResult<()> {
        if let Some(name) = p.name {
            self.name = name.trim().to_owned();
        }
        if let Some(slug) = p.slug {
            self.slug = slug.trim().to_owned();
        }
        let s = p.settings;
        if let Some(v) = s.review_interval_months {
            self.settings.review_interval_months = v;
        }
        if let Some(v) = s.impact_categories {
            self.settings.impact_categories = v;
        }
        if let Some(mut v) = s.impact_time_windows {
            v.sort();
            v.dedup();
            self.settings.impact_time_windows = v;
        }
        if let Some(v) = s.impact_tolerance_level {
            self.settings.impact_tolerance_level = v;
        }
        if let Some(v) = s.default_language {
            self.settings.default_language = v;
        }
        if let Some(v) = s.ai_enabled {
            self.settings.ai_enabled = v;
        }
        self.validate()
    }

    fn validate(&self) -> AppResult<()> {
        let mut issues = Issues::new();
        issues.check(
            (1..=200).contains(&self.name.chars().count()),
            "INVALID_VALUE",
            "/name",
            "name must be 1-200 characters",
        );
        issues.check(
            is_valid_slug(&self.slug),
            "INVALID_VALUE",
            "/slug",
            "slug must match ^[a-z0-9-]{2,63}$",
        );
        issues.check(
            self.settings.review_interval_months >= 1,
            "OUT_OF_RANGE",
            "/settings/reviewIntervalMonths",
            "review interval must be at least 1 month",
        );
        issues.check(
            !self.settings.impact_categories.is_empty(),
            "REQUIRED",
            "/settings/impactCategories",
            "at least one impact category is required",
        );
        issues.check(
            !self.settings.impact_time_windows.is_empty(),
            "REQUIRED",
            "/settings/impactTimeWindowsMinutes",
            "at least one time window is required",
        );
        issues.into_result()
    }
}

pub fn is_valid_slug(slug: &str) -> bool {
    (2..=63).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Tenants the user is a member of (tenant-independent).
    async fn list_for_user(&self, user: &str, page: PageRequest) -> AppResult<Page<Tenant>>;
    /// Creates the tenant, its first member and the default DR roles atomically.
    async fn create(
        &self,
        tenant: &Tenant,
        admin: &NewMember,
        default_roles: &[(&str, &str)],
    ) -> AppResult<Tenant>;
    async fn find_by_slug(&self, slug: &str) -> AppResult<Option<Tenant>>;
    async fn get(&self, ctx: &TenantContext) -> AppResult<Option<Tenant>>;
    async fn update(&self, ctx: &TenantContext, tenant: &Tenant) -> AppResult<Tenant>;
    async fn delete(&self, ctx: &TenantContext) -> AppResult<()>;
    /// Adds a membership if the user is not yet a member (idempotent).
    async fn ensure_member(&self, tenant_id: TenantId, member: &NewMember) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_slug_and_name() {
        assert!(
            Tenant::create(
                "u",
                "Acme".into(),
                "acme-1".into(),
                TenantSettingsPatch::default()
            )
            .is_ok()
        );
        assert!(
            Tenant::create(
                "u",
                "Acme".into(),
                "Acme".into(),
                TenantSettingsPatch::default()
            )
            .is_err()
        );
        assert!(
            Tenant::create(
                "u",
                " ".into(),
                "acme".into(),
                TenantSettingsPatch::default()
            )
            .is_err()
        );
    }
}
