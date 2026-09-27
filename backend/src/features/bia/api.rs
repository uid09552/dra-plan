use axum::Router;
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{Bia, BiaInput, ImpactRating};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, ImpactCategory, Minutes, Rating, TenantContext};
use crate::shared::web::{
    ApiPath, ApiResult, IfMatch, MetaDto, ProvenanceDto, ValidJson, with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/services/{service_id}/bia", get(get_bia).put(put_bia))
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImpactRatingDto {
    #[serde_as(as = "DisplayFromStr")]
    pub impact_category: ImpactCategory,
    #[validate(range(max = 5_270_400))]
    pub time_window_minutes: u32,
    #[validate(range(min = 1, max = 4))]
    pub level: u8,
    #[validate(length(max = 2000))]
    pub rationale: Option<String>,
}

impl ImpactRatingDto {
    fn into_domain(self) -> AppResult<ImpactRating> {
        Ok(ImpactRating {
            category: self.impact_category,
            time_window: Minutes::new(self.time_window_minutes)?,
            level: Rating::new(i64::from(self.level))?,
            rationale: self.rationale,
        })
    }
}

impl From<ImpactRating> for ImpactRatingDto {
    fn from(r: ImpactRating) -> Self {
        ImpactRatingDto {
            impact_category: r.category,
            time_window_minutes: r.time_window.get(),
            level: r.level.get(),
            rationale: r.rationale,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BiaInputDto {
    #[validate(range(max = 5_270_400))]
    pub mtpd_minutes: u32,
    #[validate(range(max = 5_270_400))]
    pub service_rto_minutes: u32,
    #[validate(range(max = 5_270_400))]
    pub service_rpo_minutes: u32,
    #[validate(length(max = 5000))]
    pub minimum_operating_level: Option<String>,
    #[validate(length(max = 5000))]
    pub regulatory_requirements: Option<String>,
    #[validate(nested, length(max = 200))]
    #[serde(default)]
    pub impact_ratings: Vec<ImpactRatingDto>,
}

impl BiaInputDto {
    fn into_domain(self) -> AppResult<BiaInput> {
        Ok(BiaInput {
            mtpd: Minutes::new(self.mtpd_minutes)?,
            service_rto: Minutes::new(self.service_rto_minutes)?,
            service_rpo: Minutes::new(self.service_rpo_minutes)?,
            minimum_operating_level: self.minimum_operating_level,
            regulatory_requirements: self.regulatory_requirements,
            impact_ratings: self
                .impact_ratings
                .into_iter()
                .map(ImpactRatingDto::into_domain)
                .collect::<AppResult<_>>()?,
        })
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiaDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub service_id: Uuid,
    pub mtpd_minutes: u32,
    pub service_rto_minutes: u32,
    pub service_rpo_minutes: u32,
    pub minimum_operating_level: Option<String>,
    pub regulatory_requirements: Option<String>,
    pub impact_ratings: Vec<ImpactRatingDto>,
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub derived_mtpd_minutes: Option<u32>,
}

impl BiaDto {
    pub fn new(b: Bia, derived: Option<Minutes>) -> Self {
        BiaDto {
            meta: b.meta.into(),
            provenance: b.provenance.into(),
            service_id: b.service_id,
            mtpd_minutes: b.mtpd.get(),
            service_rto_minutes: b.service_rto.get(),
            service_rpo_minutes: b.service_rpo.get(),
            minimum_operating_level: b.minimum_operating_level,
            regulatory_requirements: b.regulatory_requirements,
            impact_ratings: b
                .impact_ratings
                .into_iter()
                .map(ImpactRatingDto::from)
                .collect(),
            approved_at: b.approved_at,
            derived_mtpd_minutes: derived.map(Minutes::get),
        }
    }
}

impl TryFrom<BiaDto> for Bia {
    type Error = AppError;

    fn try_from(d: BiaDto) -> AppResult<Self> {
        Ok(Bia {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            service_id: d.service_id,
            mtpd: Minutes::new(d.mtpd_minutes)?,
            service_rto: Minutes::new(d.service_rto_minutes)?,
            service_rpo: Minutes::new(d.service_rpo_minutes)?,
            minimum_operating_level: d.minimum_operating_level,
            regulatory_requirements: d.regulatory_requirements,
            impact_ratings: d
                .impact_ratings
                .into_iter()
                .map(ImpactRatingDto::into_domain)
                .collect::<AppResult<_>>()?,
            approved_at: d.approved_at,
        })
    }
}

async fn get_bia(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let (bia, derived) = app.bia.get(&ctx, service_id).await?;
    Ok(with_etag(bia.meta.version, BiaDto::new(bia, derived)))
}

async fn put_bia(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<BiaInputDto>,
) -> ApiResult<Response> {
    let (bia, derived) = app
        .bia
        .put(&ctx, service_id, if_match, dto.into_domain()?)
        .await?;
    Ok(with_etag(bia.meta.version, BiaDto::new(bia, derived)))
}
