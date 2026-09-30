use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{CustomCategory, CustomCategoryRepository};
use crate::shared::infra::{Db, MetaRow, write_err};
use crate::shared::kernel::{AppResult, TenantContext};

#[derive(FromRow)]
struct CategoryRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    key: String,
    label: String,
}

impl From<CategoryRow> for CustomCategory {
    fn from(r: CategoryRow) -> Self {
        CustomCategory {
            meta: r.meta.into(),
            key: r.key,
            label: r.label,
        }
    }
}

pub struct PgCustomCategoryRepository(pub Db);

#[async_trait]
impl CustomCategoryRepository for PgCustomCategoryRepository {
    async fn list(&self, ctx: &TenantContext) -> AppResult<Vec<CustomCategory>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<CategoryRow> = sqlx::query_as(
            "select * from scenario_category where tenant_id = $1 order by label, id",
        )
        .bind(ctx.tenant_id.0)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn insert(&self, ctx: &TenantContext, c: &CustomCategory) -> AppResult<CustomCategory> {
        let mut tx = self.0.begin(ctx).await?;
        let row: CategoryRow = sqlx::query_as(
            "insert into scenario_category (id, tenant_id, key, label, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $5) returning *",
        )
        .bind(c.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(&c.key)
        .bind(&c.label)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(row.into())
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from scenario_category where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(write_err)?;
        tx.commit().await?;
        Ok(())
    }
}
