//! Role assignments with deputies (BSI BAO, Vertretung) and communication rules (workflow steps 10–11).

use async_trait::async_trait;
use uuid::Uuid;

use crate::shared::kernel::{
    AppResult, Issues, Meta, Minutes, Provenance, TenantContext, non_blank, str_enum,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RoleAssignment {
    pub meta: Meta,
    pub service_id: Uuid,
    pub role_id: Uuid,
    pub person_id: Uuid,
    pub is_deputy: bool,
    pub escalation_order: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RoleAssignmentInput {
    pub role_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub is_deputy: Option<bool>,
    pub escalation_order: Option<u32>,
}

impl RoleAssignment {
    pub fn create(
        ctx: &TenantContext,
        service_id: Uuid,
        role_id: Uuid,
        person_id: Uuid,
        input: RoleAssignmentInput,
    ) -> AppResult<Self> {
        let mut a = RoleAssignment {
            meta: Meta::new(ctx),
            service_id,
            role_id,
            person_id,
            is_deputy: false,
            escalation_order: None,
        };
        a.apply(input)?;
        Ok(a)
    }

    pub fn apply(&mut self, p: RoleAssignmentInput) -> AppResult<()> {
        if let Some(v) = p.role_id {
            self.role_id = v;
        }
        if let Some(v) = p.person_id {
            self.person_id = v;
        }
        if let Some(v) = p.is_deputy {
            self.is_deputy = v;
        }
        if p.escalation_order.is_some() {
            self.escalation_order = p.escalation_order;
        }
        let mut issues = Issues::new();
        issues.check(
            self.escalation_order != Some(0),
            "OUT_OF_RANGE",
            "/escalationOrder",
            "escalation order starts at 1",
        );
        issues.into_result()
    }
}

str_enum! {
    pub enum CommunicationTrigger {
        DrDeclared = "dr_declared",
        StatusUpdate = "status_update",
        Recovered = "recovered",
        Failback = "failback",
        Aborted = "aborted",
    }
}

str_enum! {
    pub enum CommunicationAudience {
        Internal = "internal",
        Management = "management",
        Customers = "customers",
        Regulator = "regulator",
        Suppliers = "suppliers",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunicationRule {
    pub meta: Meta,
    pub provenance: Provenance,
    pub service_id: Uuid,
    pub trigger: CommunicationTrigger,
    pub audience: CommunicationAudience,
    pub channel: String,
    pub frequency: Option<Minutes>,
    pub responsible_role_id: Uuid,
    pub authorizer_role_id: Option<Uuid>,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CommunicationRuleInput {
    pub trigger: Option<CommunicationTrigger>,
    pub audience: Option<CommunicationAudience>,
    pub channel: Option<String>,
    pub frequency: Option<Option<Minutes>>,
    pub responsible_role_id: Option<Uuid>,
    pub authorizer_role_id: Option<Option<Uuid>>,
    pub template: Option<String>,
}

impl CommunicationRule {
    pub fn create(
        ctx: &TenantContext,
        service_id: Uuid,
        trigger: CommunicationTrigger,
        audience: CommunicationAudience,
        responsible_role_id: Uuid,
        input: CommunicationRuleInput,
    ) -> AppResult<Self> {
        let mut r = CommunicationRule {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            service_id,
            trigger,
            audience,
            channel: String::new(),
            frequency: None,
            responsible_role_id,
            authorizer_role_id: None,
            template: None,
        };
        r.apply(input)?;
        Ok(r)
    }

    pub fn apply(&mut self, p: CommunicationRuleInput) -> AppResult<()> {
        if let Some(v) = p.trigger {
            self.trigger = v;
        }
        if let Some(v) = p.audience {
            self.audience = v;
        }
        if let Some(v) = p.channel {
            self.channel = v.trim().to_owned();
        }
        if let Some(v) = p.frequency {
            self.frequency = v;
        }
        if let Some(v) = p.responsible_role_id {
            self.responsible_role_id = v;
        }
        if let Some(v) = p.authorizer_role_id {
            self.authorizer_role_id = v;
        }
        if p.template.is_some() {
            self.template = non_blank(p.template);
        }
        let mut issues = Issues::new();
        issues.check(
            !self.channel.is_empty(),
            "REQUIRED",
            "/channel",
            "channel is required",
        );
        issues.check(
            self.frequency.is_none_or(|f| f.get() >= 1),
            "OUT_OF_RANGE",
            "/frequencyMinutes",
            "frequency must be at least 1 minute",
        );
        issues.check(
            self.trigger != CommunicationTrigger::StatusUpdate || self.frequency.is_some(),
            "REQUIRED",
            "/frequencyMinutes",
            "status updates need a frequency",
        );
        issues.into_result()
    }
}

#[async_trait]
pub trait RoleAssignmentRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RoleAssignment>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RoleAssignment>>;
    async fn insert(&self, ctx: &TenantContext, a: &RoleAssignment) -> AppResult<RoleAssignment>;
    async fn update(&self, ctx: &TenantContext, a: &RoleAssignment) -> AppResult<RoleAssignment>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool>;
}

#[async_trait]
pub trait CommunicationRuleRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<CommunicationRule>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<CommunicationRule>>;
    async fn insert(
        &self,
        ctx: &TenantContext,
        r: &CommunicationRule,
    ) -> AppResult<CommunicationRule>;
    async fn update(
        &self,
        ctx: &TenantContext,
        r: &CommunicationRule,
    ) -> AppResult<CommunicationRule>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool>;
}
