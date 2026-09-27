use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Microservice, MicroserviceRepository};
use crate::shared::infra::{Db, MetaRow, ProvenanceRow, db_enum_opt, require_updated, write_err};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct MicroserviceRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    service_id: Uuid,
    name: String,
    description: Option<String>,
    owner_team: Option<String>,
    platform: Option<String>,
    hosting_location: Option<String>,
    data_stores: Vec<String>,
    restore_order: Option<i32>,
    criticality_within_service: Option<String>,
    is_default: bool,
}

impl TryFrom<MicroserviceRow> for Microservice {
    type Error = AppError;

    fn try_from(r: MicroserviceRow) -> AppResult<Self> {
        Ok(Microservice {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            service_id: r.service_id,
            name: r.name,
            description: r.description,
            owner_team: r.owner_team,
            platform: r.platform,
            hosting_location: r.hosting_location,
            data_stores: r.data_stores,
            restore_order: r.restore_order.map(|v| v.max(1) as u32),
            criticality_within_service: db_enum_opt(r.criticality_within_service.as_deref())?,
            is_default: r.is_default,
        })
    }
}

pub struct PgMicroserviceRepository(pub Db);

#[async_trait]
impl MicroserviceRepository for PgMicroserviceRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<Microservice>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<MicroserviceRow> = sqlx::query_as(
            "select * from microservice where tenant_id = $1 and service_id = $2
             order by is_default desc, restore_order nulls last, name, id",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(Microservice::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Microservice>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<MicroserviceRow> =
            sqlx::query_as("select * from microservice where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(Microservice::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, m: &Microservice) -> AppResult<Microservice> {
        let mut tx = self.0.begin(ctx).await?;
        let row: MicroserviceRow = sqlx::query_as(
            "insert into microservice (id, tenant_id, service_id, name, description, owner_team, platform, hosting_location,
                                       data_stores, restore_order, criticality_within_service, origin, ai_suggestion_id,
                                       created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14) returning *",
        )
        .bind(m.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(m.service_id)
        .bind(&m.name)
        .bind(&m.description)
        .bind(&m.owner_team)
        .bind(&m.platform)
        .bind(&m.hosting_location)
        .bind(&m.data_stores)
        .bind(m.restore_order.map(|v| v as i32))
        .bind(m.criticality_within_service.map(|v| v.as_str()))
        .bind(m.provenance.origin.as_str())
        .bind(m.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, m: &Microservice) -> AppResult<Microservice> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<MicroserviceRow> = sqlx::query_as(
            "update microservice set name = $4, description = $5, owner_team = $6, platform = $7, hosting_location = $8,
                    data_stores = $9, restore_order = $10, criticality_within_service = $11,
                    version = version + 1, updated_at = now(), updated_by = $12
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(m.meta.id)
        .bind(m.meta.version)
        .bind(&m.name)
        .bind(&m.description)
        .bind(&m.owner_team)
        .bind(&m.platform)
        .bind(&m.hosting_location)
        .bind(&m.data_stores)
        .bind(m.restore_order.map(|v| v as i32))
        .bind(m.criticality_within_service.map(|v| v.as_str()))
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from microservice where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
