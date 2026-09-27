use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{ActionItem, ActionItemFilter, ActionItemRepository, RelatedEntity};
use crate::shared::infra::{Db, MetaRow, ProvenanceRow, Tx, db_enum, require_updated, write_err};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct ActionItemRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    service_id: Uuid,
    source: String,
    source_id: Option<Uuid>,
    title: String,
    description: Option<String>,
    owner_person_id: Option<Uuid>,
    due_date: Option<NaiveDate>,
    status: String,
    related_entity_type: Option<String>,
    related_entity_id: Option<Uuid>,
}

impl TryFrom<ActionItemRow> for ActionItem {
    type Error = AppError;

    fn try_from(r: ActionItemRow) -> AppResult<Self> {
        Ok(ActionItem {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            service_id: r.service_id,
            source: db_enum(&r.source)?,
            source_id: r.source_id,
            title: r.title,
            description: r.description,
            owner_person_id: r.owner_person_id,
            due_date: r.due_date,
            status: db_enum(&r.status)?,
            related_entity: match (r.related_entity_type, r.related_entity_id) {
                (Some(entity_type), Some(id)) => Some(RelatedEntity { entity_type, id }),
                _ => None,
            },
        })
    }
}

/// Inserts an action item inside an existing transaction (used by other features' adapters for
/// atomic operations, e.g. accepting a strategy gap or closing a recovery run).
pub async fn insert_action_item(tx: &mut Tx, ctx: &TenantContext, a: &ActionItem) -> AppResult<()> {
    sqlx::query(
        "insert into action_item (id, tenant_id, service_id, source, source_id, title, description, owner_person_id,
                due_date, status, related_entity_type, related_entity_id, origin, ai_suggestion_id, created_by, updated_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
    )
    .bind(a.meta.id)
    .bind(ctx.tenant_id.0)
    .bind(a.service_id)
    .bind(a.source.as_str())
    .bind(a.source_id)
    .bind(&a.title)
    .bind(&a.description)
    .bind(a.owner_person_id)
    .bind(a.due_date)
    .bind(a.status.as_str())
    .bind(a.related_entity.as_ref().map(|r| r.entity_type.as_str()))
    .bind(a.related_entity.as_ref().map(|r| r.id))
    .bind(a.provenance.origin.as_str())
    .bind(a.provenance.ai_suggestion_id)
    .bind(ctx.actor())
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

pub struct PgActionItemRepository(pub Db);

#[async_trait]
impl ActionItemRepository for PgActionItemRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        f: &ActionItemFilter,
    ) -> AppResult<Vec<ActionItem>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ActionItemRow> = sqlx::query_as(
            "select * from action_item
             where tenant_id = $1 and service_id = $2
               and ($3::text is null or status = $3) and ($4::text is null or source = $4)
             order by (status in ('open', 'in_progress')) desc, due_date nulls last, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .bind(f.status.map(|v| v.as_str()))
        .bind(f.source.map(|v| v.as_str()))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(ActionItem::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<ActionItem>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ActionItemRow> =
            sqlx::query_as("select * from action_item where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(ActionItem::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, a: &ActionItem) -> AppResult<ActionItem> {
        let mut tx = self.0.begin(ctx).await?;
        insert_action_item(&mut tx, ctx, a).await?;
        let row: ActionItemRow = sqlx::query_as("select * from action_item where id = $1")
            .bind(a.meta.id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, a: &ActionItem) -> AppResult<ActionItem> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<ActionItemRow> = sqlx::query_as(
            "update action_item set title = $4, description = $5, owner_person_id = $6, due_date = $7, status = $8,
                    related_entity_type = $9, related_entity_id = $10,
                    version = version + 1, updated_at = now(), updated_by = $11
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(a.meta.id)
        .bind(a.meta.version)
        .bind(&a.title)
        .bind(&a.description)
        .bind(a.owner_person_id)
        .bind(a.due_date)
        .bind(a.status.as_str())
        .bind(a.related_entity.as_ref().map(|r| r.entity_type.as_str()))
        .bind(a.related_entity.as_ref().map(|r| r.id))
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }
}
