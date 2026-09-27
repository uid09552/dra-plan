use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Dependency, DependencyRepository};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, db_enum, db_minutes, opt_minutes, require_updated, write_err,
};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct DependencyRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    microservice_id: Uuid,
    kind: String,
    target_microservice_id: Option<Uuid>,
    target_name: Option<String>,
    direction: String,
    criticality: String,
    dependency_rto_minutes: Option<i32>,
    has_own_dr_plan: bool,
    notes: Option<String>,
}

impl TryFrom<DependencyRow> for Dependency {
    type Error = AppError;

    fn try_from(r: DependencyRow) -> AppResult<Self> {
        Ok(Dependency {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            microservice_id: r.microservice_id,
            kind: db_enum(&r.kind)?,
            target_microservice_id: r.target_microservice_id,
            target_name: r.target_name,
            direction: db_enum(&r.direction)?,
            criticality: db_enum(&r.criticality)?,
            dependency_rto: db_minutes(r.dependency_rto_minutes)?,
            has_own_dr_plan: r.has_own_dr_plan,
            notes: r.notes,
        })
    }
}

pub struct PgDependencyRepository(pub Db);

#[async_trait]
impl DependencyRepository for PgDependencyRepository {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<Dependency>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<DependencyRow> = sqlx::query_as(
            "select * from dependency where tenant_id = $1 and microservice_id = $2 order by created_at, id",
        )
        .bind(ctx.tenant_id.0)
        .bind(microservice_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(Dependency::try_from).collect()
    }

    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<Dependency>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<DependencyRow> = sqlx::query_as(
            "select d.* from dependency d join microservice m on m.id = d.microservice_id
             where d.tenant_id = $1 and m.service_id = $2 order by d.microservice_id, d.created_at, d.id",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(Dependency::try_from).collect()
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Dependency>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DependencyRow> =
            sqlx::query_as("select * from dependency where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(Dependency::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, d: &Dependency) -> AppResult<Dependency> {
        let mut tx = self.0.begin(ctx).await?;
        let row: DependencyRow = sqlx::query_as(
            "insert into dependency (id, tenant_id, microservice_id, kind, target_microservice_id, target_name, direction,
                    criticality, dependency_rto_minutes, has_own_dr_plan, notes, origin, ai_suggestion_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14) returning *",
        )
        .bind(d.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(d.microservice_id)
        .bind(d.kind.as_str())
        .bind(d.target_microservice_id)
        .bind(&d.target_name)
        .bind(d.direction.as_str())
        .bind(d.criticality.as_str())
        .bind(opt_minutes(d.dependency_rto))
        .bind(d.has_own_dr_plan)
        .bind(&d.notes)
        .bind(d.provenance.origin.as_str())
        .bind(d.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, d: &Dependency) -> AppResult<Dependency> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<DependencyRow> = sqlx::query_as(
            "update dependency set kind = $4, target_microservice_id = $5, target_name = $6, direction = $7,
                    criticality = $8, dependency_rto_minutes = $9, has_own_dr_plan = $10, notes = $11,
                    version = version + 1, updated_at = now(), updated_by = $12
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(d.meta.id)
        .bind(d.meta.version)
        .bind(d.kind.as_str())
        .bind(d.target_microservice_id)
        .bind(&d.target_name)
        .bind(d.direction.as_str())
        .bind(d.criticality.as_str())
        .bind(opt_minutes(d.dependency_rto))
        .bind(d.has_own_dr_plan)
        .bind(&d.notes)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from dependency where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
