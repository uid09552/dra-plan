use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{DataProtection, DataProtectionRepository};
use crate::shared::infra::{Db, MetaRow, ProvenanceRow, db_enum, require_updated, write_err};
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

#[derive(FromRow)]
struct DataProtectionRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    microservice_id: Uuid,
    data_store: String,
    method: String,
    frequency_minutes: i32,
    retention_days: Option<i32>,
    offsite: bool,
    immutable: bool,
    encrypted: bool,
    last_restore_test_at: Option<DateTime<Utc>>,
}

impl TryFrom<DataProtectionRow> for DataProtection {
    type Error = AppError;

    fn try_from(r: DataProtectionRow) -> AppResult<Self> {
        Ok(DataProtection {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            microservice_id: r.microservice_id,
            data_store: r.data_store,
            method: db_enum(&r.method)?,
            frequency: Minutes::from_db(r.frequency_minutes)?,
            retention_days: r.retention_days.map(|v| v.max(0) as u32),
            offsite: r.offsite,
            immutable: r.immutable,
            encrypted: r.encrypted,
            last_restore_test_at: r.last_restore_test_at,
        })
    }
}

pub struct PgDataProtectionRepository(pub Db);

#[async_trait]
impl DataProtectionRepository for PgDataProtectionRepository {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<DataProtection>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<DataProtectionRow> = sqlx::query_as(
            "select * from data_protection where tenant_id = $1 and microservice_id = $2 order by data_store, created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(microservice_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(DataProtection::try_from).collect()
    }

    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<DataProtection>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<DataProtectionRow> = sqlx::query_as(
            "select d.* from data_protection d join microservice m on m.id = d.microservice_id
             where d.tenant_id = $1 and m.service_id = $2 order by d.microservice_id, d.data_store",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(DataProtection::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<DataProtection>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DataProtectionRow> =
            sqlx::query_as("select * from data_protection where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(DataProtection::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, d: &DataProtection) -> AppResult<DataProtection> {
        let mut tx = self.0.begin(ctx).await?;
        let row: DataProtectionRow = sqlx::query_as(
            "insert into data_protection (id, tenant_id, microservice_id, data_store, method, frequency_minutes,
                    retention_days, offsite, immutable, encrypted, last_restore_test_at, origin, ai_suggestion_id,
                    created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14) returning *",
        )
        .bind(d.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(d.microservice_id)
        .bind(&d.data_store)
        .bind(d.method.as_str())
        .bind(d.frequency.to_db())
        .bind(d.retention_days.map(|v| v as i32))
        .bind(d.offsite)
        .bind(d.immutable)
        .bind(d.encrypted)
        .bind(d.last_restore_test_at)
        .bind(d.provenance.origin.as_str())
        .bind(d.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, d: &DataProtection) -> AppResult<DataProtection> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DataProtectionRow> = sqlx::query_as(
            "update data_protection set data_store = $4, method = $5, frequency_minutes = $6, retention_days = $7,
                    offsite = $8, immutable = $9, encrypted = $10, last_restore_test_at = $11,
                    version = version + 1, updated_at = now(), updated_by = $12
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(d.meta.id)
        .bind(d.meta.version)
        .bind(&d.data_store)
        .bind(d.method.as_str())
        .bind(d.frequency.to_db())
        .bind(d.retention_days.map(|v| v as i32))
        .bind(d.offsite)
        .bind(d.immutable)
        .bind(d.encrypted)
        .bind(d.last_restore_test_at)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool> {
        let mut tx = self.0.begin(ctx).await?;
        let result = sqlx::query("delete from data_protection where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }
}
