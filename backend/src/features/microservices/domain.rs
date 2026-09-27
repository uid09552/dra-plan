//! Components of an IT service (API/DB name: microservice). They own the DR items.
//! Every service has one default component (`is_default`) that stands for the whole service; it is
//! created with the service (database trigger) and cannot be deleted (ADR-0011).

use async_trait::async_trait;
use uuid::Uuid;

use crate::shared::kernel::{
    AppResult, Issues, Meta, Provenance, TenantContext, non_blank, str_enum,
};

str_enum! {
    pub enum MicroserviceCriticality { Critical = "critical", Important = "important", Supporting = "supporting" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Microservice {
    pub meta: Meta,
    pub provenance: Provenance,
    pub service_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_team: Option<String>,
    pub platform: Option<String>,
    pub hosting_location: Option<String>,
    pub data_stores: Vec<String>,
    pub restore_order: Option<u32>,
    pub criticality_within_service: Option<MicroserviceCriticality>,
    /// Stands for the service as a whole.
    pub is_default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MicroserviceInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub owner_team: Option<String>,
    pub platform: Option<String>,
    pub hosting_location: Option<String>,
    pub data_stores: Option<Vec<String>>,
    pub restore_order: Option<u32>,
    pub criticality_within_service: Option<MicroserviceCriticality>,
}

impl Microservice {
    pub fn create(
        ctx: &TenantContext,
        service_id: Uuid,
        input: MicroserviceInput,
    ) -> AppResult<Self> {
        let mut m = Microservice {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            service_id,
            name: String::new(),
            description: None,
            owner_team: None,
            platform: None,
            hosting_location: None,
            data_stores: Vec::new(),
            restore_order: None,
            criticality_within_service: None,
            is_default: false,
        };
        m.apply(input)?;
        Ok(m)
    }

    pub fn apply(&mut self, p: MicroserviceInput) -> AppResult<()> {
        if let Some(v) = p.name {
            self.name = v.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        if p.owner_team.is_some() {
            self.owner_team = non_blank(p.owner_team);
        }
        if p.platform.is_some() {
            self.platform = non_blank(p.platform);
        }
        if p.hosting_location.is_some() {
            self.hosting_location = non_blank(p.hosting_location);
        }
        if let Some(v) = p.data_stores {
            self.data_stores = v
                .into_iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if p.restore_order.is_some() {
            self.restore_order = p.restore_order;
        }
        if p.criticality_within_service.is_some() {
            self.criticality_within_service = p.criticality_within_service;
        }
        let mut issues = Issues::new();
        issues.check(
            !self.name.is_empty(),
            "REQUIRED",
            "/name",
            "name is required",
        );
        issues.check(
            self.restore_order != Some(0),
            "OUT_OF_RANGE",
            "/restoreOrder",
            "restore order starts at 1",
        );
        issues.into_result()
    }
}

#[async_trait]
pub trait MicroserviceRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<Microservice>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Microservice>>;
    async fn insert(&self, ctx: &TenantContext, m: &Microservice) -> AppResult<Microservice>;
    async fn update(&self, ctx: &TenantContext, m: &Microservice) -> AppResult<Microservice>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
}
