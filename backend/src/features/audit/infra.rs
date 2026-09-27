use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{AuditEntry, AuditFilter, AuditRepository};
use crate::shared::infra::Db;
use crate::shared::kernel::{AppResult, Page, PageRequest, TenantContext};

#[derive(FromRow)]
struct AuditRow {
    id: i64,
    at: DateTime<Utc>,
    actor: String,
    action: String,
    entity_type: String,
    entity_id: Option<Uuid>,
    diff: Option<String>,
}

pub struct PgAuditRepository(pub Db);

#[async_trait]
impl AuditRepository for PgAuditRepository {
    async fn list(
        &self,
        ctx: &TenantContext,
        f: &AuditFilter,
        page: PageRequest,
    ) -> AppResult<Page<AuditEntry>> {
        // audit_log has no row-level security; the tenant filter below is mandatory.
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<AuditRow> = sqlx::query_as(
            "select id, at, actor, action, entity_type, entity_id, diff::text as diff from audit_log
             where tenant_id = $1
               and ($2::text is null or entity_type = $2) and ($3::uuid is null or entity_id = $3)
               and ($4::timestamptz is null or at >= $4) and ($5::timestamptz is null or at < $5)
             order by id desc limit $6 offset $7",
        )
        .bind(ctx.tenant_id.0)
        .bind(&f.entity_type)
        .bind(f.entity_id)
        .bind(f.from)
        .bind(f.to)
        .bind(page.fetch_limit())
        .bind(page.offset)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let items = rows
            .into_iter()
            .map(|r| AuditEntry {
                id: r.id,
                at: r.at,
                actor: r.actor,
                action: r.action,
                entity_type: r.entity_type,
                entity_id: r.entity_id,
                diff: r.diff,
            })
            .collect();
        Ok(Page::from_overfetch(items, page))
    }
}
