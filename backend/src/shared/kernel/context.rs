use std::fmt;

use uuid::Uuid;

use super::enums::str_enum;

/// Identifier of the tenant all data of a request belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TenantId(pub Uuid);

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

str_enum! {
    pub enum TenantRole {
        Admin = "admin",
        Author = "author",
        Reviewer = "reviewer",
        Responder = "responder",
        Auditor = "auditor",
    }
}

/// The authenticated caller.
#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    /// Subject identifier (`sub` claim).
    pub user: String,
    pub role: TenantRole,
}

/// Tenant and caller of a request. Every repository call requires it, so no query can run
/// without a tenant scope.
#[derive(Debug, Clone, PartialEq)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub principal: Principal,
}

impl TenantContext {
    pub fn actor(&self) -> &str {
        &self.principal.user
    }
}
