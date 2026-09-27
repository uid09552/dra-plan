//! Contact directory (persons), DR role catalog (BSI BAO roles) and tenant members.

use async_trait::async_trait;
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Issues, Meta, Page, PageRequest, TenantContext, TenantRole, non_blank,
};

/// DR roles seeded into every new tenant (name, description).
pub const DEFAULT_ROLES: &[(&str, &str)] = &[
    (
        "Incident Commander",
        "Declares DR, leads the recovery and authorizes failover/failback",
    ),
    (
        "Service Owner",
        "Business owner; approves the plan and business decisions",
    ),
    ("DBA", "Database recovery"),
    ("Platform", "Infrastructure and platform recovery"),
    ("Service Team", "Application recovery and validation"),
    ("Communications", "Internal and customer communication"),
];

// ───────────────────────────── Person ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub meta: Meta,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub alternate_contact: Option<String>,
    pub team: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PersonInput {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub alternate_contact: Option<String>,
    pub team: Option<String>,
}

impl Person {
    pub fn create(ctx: &TenantContext, input: PersonInput) -> AppResult<Self> {
        let mut person = Person {
            meta: Meta::new(ctx),
            name: String::new(),
            email: None,
            phone: None,
            alternate_contact: None,
            team: None,
        };
        person.apply(input)?;
        Ok(person)
    }

    /// Applies a merge patch: absent fields stay, empty strings clear optional fields.
    pub fn apply(&mut self, p: PersonInput) -> AppResult<()> {
        if let Some(name) = p.name {
            self.name = name.trim().to_owned();
        }
        if p.email.is_some() {
            self.email = non_blank(p.email);
        }
        if p.phone.is_some() {
            self.phone = non_blank(p.phone);
        }
        if p.alternate_contact.is_some() {
            self.alternate_contact = non_blank(p.alternate_contact);
        }
        if p.team.is_some() {
            self.team = non_blank(p.team);
        }
        let mut issues = Issues::new();
        issues.check(
            !self.name.is_empty(),
            "REQUIRED",
            "/name",
            "name is required",
        );
        issues.into_result()
    }
}

#[async_trait]
pub trait PersonRepository: Send + Sync {
    async fn list(
        &self,
        ctx: &TenantContext,
        query: Option<&str>,
        page: PageRequest,
    ) -> AppResult<Page<Person>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Person>>;
    async fn get_many(&self, ctx: &TenantContext, ids: &[Uuid]) -> AppResult<Vec<Person>>;
    async fn insert(&self, ctx: &TenantContext, person: &Person) -> AppResult<Person>;
    async fn update(&self, ctx: &TenantContext, person: &Person) -> AppResult<Person>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
    async fn owns_open_action_items(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool>;
}

// ───────────────────────────── Role ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Role {
    pub meta: Meta,
    pub name: String,
    pub description: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoleInput {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl Role {
    pub fn create(ctx: &TenantContext, input: RoleInput) -> AppResult<Self> {
        let mut role = Role {
            meta: Meta::new(ctx),
            name: String::new(),
            description: None,
            is_default: false,
        };
        role.apply(input)?;
        Ok(role)
    }

    pub fn apply(&mut self, p: RoleInput) -> AppResult<()> {
        if let Some(name) = p.name {
            self.name = name.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        let mut issues = Issues::new();
        issues.check(
            !self.name.is_empty(),
            "REQUIRED",
            "/name",
            "name is required",
        );
        issues.into_result()
    }

    pub fn ensure_deletable(&self) -> AppResult<()> {
        if self.is_default {
            return Err(AppError::conflict("default roles cannot be deleted"));
        }
        Ok(())
    }
}

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn list(&self, ctx: &TenantContext) -> AppResult<Vec<Role>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Role>>;
    async fn insert(&self, ctx: &TenantContext, role: &Role) -> AppResult<Role>;
    async fn update(&self, ctx: &TenantContext, role: &Role) -> AppResult<Role>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
}

// ───────────────────────────── Member ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub meta: Meta,
    pub user_ref: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub tenant_role: TenantRole,
    pub person_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewMember {
    pub user_ref: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub tenant_role: TenantRole,
    pub person_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct MemberPatch {
    pub tenant_role: Option<TenantRole>,
    pub person_id: Option<Option<Uuid>>,
}

impl Member {
    pub fn create(ctx: &TenantContext, new: NewMember) -> AppResult<Self> {
        let user_ref = new.user_ref.trim().to_owned();
        if user_ref.is_empty() {
            return Err(AppError::invalid(
                "REQUIRED",
                Some("/userRef"),
                "userRef is required",
            ));
        }
        Ok(Member {
            meta: Meta::new(ctx),
            user_ref,
            email: non_blank(new.email),
            display_name: non_blank(new.display_name),
            tenant_role: new.tenant_role,
            person_id: new.person_id,
        })
    }

    pub fn apply(&mut self, p: MemberPatch) {
        if let Some(role) = p.tenant_role {
            self.tenant_role = role;
        }
        if let Some(person_id) = p.person_id {
            self.person_id = person_id;
        }
    }
}

#[async_trait]
pub trait MemberRepository: Send + Sync {
    async fn list(&self, ctx: &TenantContext, page: PageRequest) -> AppResult<Page<Member>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Member>>;
    async fn insert(&self, ctx: &TenantContext, member: &Member) -> AppResult<Member>;
    async fn update(&self, ctx: &TenantContext, member: &Member) -> AppResult<Member>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool>;
}
