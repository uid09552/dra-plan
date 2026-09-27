use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Bia, BiaRepository, ImpactRating};
use crate::shared::infra::{Db, MetaRow, ProvenanceRow, Tx, db_enum, require_updated, write_err};
use crate::shared::kernel::{AppError, AppResult, Minutes, Rating, TenantContext};

#[derive(FromRow)]
struct BiaRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    service_id: Uuid,
    mtpd_minutes: i32,
    service_rto_minutes: i32,
    service_rpo_minutes: i32,
    minimum_operating_level: Option<String>,
    regulatory_requirements: Option<String>,
    approved_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct RatingRow {
    impact_category: String,
    time_window_minutes: i32,
    level: i32,
    rationale: Option<String>,
}

impl TryFrom<RatingRow> for ImpactRating {
    type Error = AppError;

    fn try_from(r: RatingRow) -> AppResult<Self> {
        Ok(ImpactRating {
            category: db_enum(&r.impact_category)?,
            time_window: Minutes::from_db(r.time_window_minutes)?,
            level: Rating::new(i64::from(r.level)).map_err(AppError::internal)?,
            rationale: r.rationale,
        })
    }
}

async fn load(tx: &mut Tx, row: BiaRow) -> AppResult<Bia> {
    let ratings: Vec<RatingRow> = sqlx::query_as(
        "select impact_category, time_window_minutes, level, rationale from impact_rating
         where bia_id = $1 order by impact_category, time_window_minutes",
    )
    .bind(row.meta.id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(Bia {
        meta: row.meta.into(),
        provenance: row.provenance.try_into()?,
        service_id: row.service_id,
        mtpd: Minutes::from_db(row.mtpd_minutes)?,
        service_rto: Minutes::from_db(row.service_rto_minutes)?,
        service_rpo: Minutes::from_db(row.service_rpo_minutes)?,
        minimum_operating_level: row.minimum_operating_level,
        regulatory_requirements: row.regulatory_requirements,
        impact_ratings: ratings
            .into_iter()
            .map(ImpactRating::try_from)
            .collect::<AppResult<_>>()?,
        approved_at: row.approved_at,
    })
}

pub struct PgBiaRepository(pub Db);

#[async_trait]
impl BiaRepository for PgBiaRepository {
    async fn get(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Option<Bia>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<BiaRow> = sqlx::query_as(
            "select * from business_impact_analysis where tenant_id = $1 and service_id = $2",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_optional(&mut *tx)
        .await?;
        let bia = match row {
            Some(r) => Some(load(&mut tx, r).await?),
            None => None,
        };
        tx.commit().await?;
        Ok(bia)
    }

    async fn save(&self, ctx: &TenantContext, b: &Bia, is_new: bool) -> AppResult<Bia> {
        let mut tx = self.0.begin(ctx).await?;
        let row: BiaRow = if is_new {
            sqlx::query_as(
                "insert into business_impact_analysis (id, tenant_id, service_id, mtpd_minutes, service_rto_minutes,
                        service_rpo_minutes, minimum_operating_level, regulatory_requirements, created_by, updated_by)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9) returning *",
            )
            .bind(b.meta.id)
            .bind(ctx.tenant_id.0)
            .bind(b.service_id)
            .bind(b.mtpd.to_db())
            .bind(b.service_rto.to_db())
            .bind(b.service_rpo.to_db())
            .bind(&b.minimum_operating_level)
            .bind(&b.regulatory_requirements)
            .bind(ctx.actor())
            .fetch_one(&mut *tx)
            .await
            .map_err(write_err)?
        } else {
            let row: Option<BiaRow> = sqlx::query_as(
                "update business_impact_analysis set mtpd_minutes = $4, service_rto_minutes = $5,
                        service_rpo_minutes = $6, minimum_operating_level = $7, regulatory_requirements = $8,
                        version = version + 1, updated_at = now(), updated_by = $9
                 where tenant_id = $1 and id = $2 and version = $3 returning *",
            )
            .bind(ctx.tenant_id.0)
            .bind(b.meta.id)
            .bind(b.meta.version)
            .bind(b.mtpd.to_db())
            .bind(b.service_rto.to_db())
            .bind(b.service_rpo.to_db())
            .bind(&b.minimum_operating_level)
            .bind(&b.regulatory_requirements)
            .bind(ctx.actor())
            .fetch_optional(&mut *tx)
            .await
            .map_err(write_err)?;
            require_updated(row)?
        };

        sqlx::query("delete from impact_rating where bia_id = $1")
            .bind(b.meta.id)
            .execute(&mut *tx)
            .await?;
        for r in &b.impact_ratings {
            sqlx::query(
                "insert into impact_rating (tenant_id, bia_id, impact_category, time_window_minutes, level, rationale)
                 values ($1, $2, $3, $4, $5, $6)",
            )
            .bind(ctx.tenant_id.0)
            .bind(b.meta.id)
            .bind(r.category.as_str())
            .bind(r.time_window.to_db())
            .bind(i32::from(r.level.get()))
            .bind(&r.rationale)
            .execute(&mut *tx)
            .await
            .map_err(write_err)?;
        }
        let bia = load(&mut tx, row).await?;
        tx.commit().await?;
        Ok(bia)
    }
}
