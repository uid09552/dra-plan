//! Business-facing IT services (workflow step 1). They own the BIA, scenarios and plan versions.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::shared::kernel::{
    AppResult, Issues, Meta, Page, PageRequest, Provenance, TenantContext, non_blank, str_enum,
};

str_enum! {
    /// BSI protection requirement (Schutzbedarf) for availability.
    pub enum ProtectionRequirement { Normal = "normal", High = "high", VeryHigh = "very_high" }
}

str_enum! {
    /// NIST FIPS 199 availability impact level.
    pub enum ImpactLevel { Low = "low", Moderate = "moderate", High = "high" }
}

str_enum! {
    pub enum LifecycleStatus { Planned = "planned", Active = "active", Retired = "retired" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItService {
    pub meta: Meta,
    pub provenance: Provenance,
    pub name: String,
    pub description: Option<String>,
    pub business_owner_id: Option<Uuid>,
    pub technical_owner_id: Option<Uuid>,
    pub consumers: Vec<String>,
    pub protection_requirement_availability: Option<ProtectionRequirement>,
    pub impact_level: Option<ImpactLevel>,
    pub lifecycle_status: LifecycleStatus,
}

/// Create/patch input. For create, absent fields take defaults; for patch, absent fields stay.
#[derive(Debug, Clone, Default)]
pub struct ItServiceInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub business_owner_id: Option<Option<Uuid>>,
    pub technical_owner_id: Option<Option<Uuid>>,
    pub consumers: Option<Vec<String>>,
    pub protection_requirement_availability: Option<ProtectionRequirement>,
    pub impact_level: Option<ImpactLevel>,
    pub lifecycle_status: Option<LifecycleStatus>,
}

impl ItService {
    pub fn create(ctx: &TenantContext, input: ItServiceInput) -> AppResult<Self> {
        let mut service = ItService {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            name: String::new(),
            description: None,
            business_owner_id: None,
            technical_owner_id: None,
            consumers: Vec::new(),
            protection_requirement_availability: None,
            impact_level: None,
            lifecycle_status: LifecycleStatus::Active,
        };
        service.apply(input)?;
        Ok(service)
    }

    pub fn apply(&mut self, p: ItServiceInput) -> AppResult<()> {
        if let Some(v) = p.name {
            self.name = v.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        if let Some(v) = p.business_owner_id {
            self.business_owner_id = v;
        }
        if let Some(v) = p.technical_owner_id {
            self.technical_owner_id = v;
        }
        if let Some(v) = p.consumers {
            self.consumers = v
                .into_iter()
                .map(|c| c.trim().to_owned())
                .filter(|c| !c.is_empty())
                .collect();
        }
        if p.protection_requirement_availability.is_some() {
            self.protection_requirement_availability = p.protection_requirement_availability;
        }
        if p.impact_level.is_some() {
            self.impact_level = p.impact_level;
        }
        if let Some(v) = p.lifecycle_status {
            self.lifecycle_status = v;
        }
        let mut issues = Issues::new();
        issues.check(
            (1..=200).contains(&self.name.chars().count()),
            "INVALID_VALUE",
            "/name",
            "name must be 1-200 characters",
        );
        issues.into_result()
    }
}

/// Read model shown with every service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServiceSummary {
    pub microservice_count: i64,
    pub selected_scenario_count: i64,
    pub completed_workflow_steps: i64,
    pub current_plan_version_id: Option<Uuid>,
    pub current_plan_status: Option<String>,
    pub next_review_due: Option<NaiveDate>,
    pub last_tested_at: Option<DateTime<Utc>>,
    pub open_action_item_count: i64,
    pub active_recovery_run_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceFilter {
    pub query: Option<String>,
    pub lifecycle_status: Option<LifecycleStatus>,
    pub review_overdue: Option<bool>,
}

#[async_trait]
pub trait ItServiceRepository: Send + Sync {
    async fn list(
        &self,
        ctx: &TenantContext,
        filter: &ServiceFilter,
        page: PageRequest,
    ) -> AppResult<Page<ItService>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<ItService>>;
    async fn insert(&self, ctx: &TenantContext, service: &ItService) -> AppResult<ItService>;
    async fn update(&self, ctx: &TenantContext, service: &ItService) -> AppResult<ItService>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
    async fn summaries(
        &self,
        ctx: &TenantContext,
        ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, ServiceSummary>>;
}
