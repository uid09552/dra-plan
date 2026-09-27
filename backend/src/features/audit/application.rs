use std::sync::Arc;

use super::domain::{AuditEntry, AuditFilter, AuditRepository};
use crate::shared::kernel::{AppResult, Page, PageRequest, TenantContext};

pub struct AuditUseCases {
    repo: Arc<dyn AuditRepository>,
}

impl AuditUseCases {
    pub fn new(repo: Arc<dyn AuditRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        filter: &AuditFilter,
        page: PageRequest,
    ) -> AppResult<Page<AuditEntry>> {
        self.repo.list(ctx, filter, page).await
    }
}
