//! Append-only audit log, written by a database trigger for every change (see migration).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::kernel::{AppResult, Page, PageRequest, TenantContext};

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    /// JSON object.
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn list(
        &self,
        ctx: &TenantContext,
        filter: &AuditFilter,
        page: PageRequest,
    ) -> AppResult<Page<AuditEntry>>;
}
