use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{
    GapCheck, GapStatus, ImplementationStatus, RecoveryStrategy, StrategyInput, StrategyType,
};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, MetaDto, OptionalJson, ProvenanceDto, ValidJson, created,
    no_content,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/microservices/{microservice_id}/recovery-strategies",
            get(list).post(create),
        )
        .route(
            "/recovery-strategies/{strategy_id}",
            get(get_one).patch(update).delete(delete),
        )
        .route("/recovery-strategies/{strategy_id}/select", post(select))
        .route(
            "/services/{service_id}/recovery-strategies",
            get(list_by_service),
        )
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GapCheckDto {
    #[serde_as(as = "DisplayFromStr")]
    pub status: GapStatus,
    pub objective_id: Option<Uuid>,
    pub rto_gap_minutes: Option<i64>,
    pub rpo_gap_minutes: Option<i64>,
    pub accepted_gap_action_item_id: Option<Uuid>,
}

impl From<GapCheck> for GapCheckDto {
    fn from(g: GapCheck) -> Self {
        GapCheckDto {
            status: g.status,
            objective_id: g.objective_id,
            rto_gap_minutes: g.rto_gap_minutes,
            rpo_gap_minutes: g.rpo_gap_minutes,
            accepted_gap_action_item_id: g.accepted_gap_action_item_id,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub microservice_id: Uuid,
    /// Scenarios this measure covers.
    #[serde(default)]
    pub scenario_ids: Vec<Uuid>,
    /// Deprecated: first of `scenarioIds` (older clients and plan snapshots).
    #[serde(default)]
    pub scenario_id: Option<Uuid>,
    #[serde(rename = "type")]
    #[serde_as(as = "DisplayFromStr")]
    pub strategy_type: StrategyType,
    pub title: Option<String>,
    pub description: Option<String>,
    pub estimated_rto_minutes: u32,
    pub estimated_rpo_minutes: u32,
    pub cost_notes: Option<String>,
    pub prerequisites: Vec<String>,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(default = "not_implemented")]
    pub implementation_status: ImplementationStatus,
    #[serde(default)]
    pub last_tested_at: Option<DateTime<Utc>>,
    pub is_selected: bool,
    pub gap_justification: Option<String>,
    #[serde(default)]
    pub gap_check: Option<GapCheckDto>,
    #[serde(default)]
    pub accepted_gap_action_item_id: Option<Uuid>,
}

impl StrategyDto {
    pub fn new(s: RecoveryStrategy, gap: Option<GapCheck>) -> Self {
        StrategyDto {
            meta: s.meta.into(),
            provenance: s.provenance.into(),
            microservice_id: s.microservice_id,
            scenario_id: s.scenario_ids.first().copied(),
            scenario_ids: s.scenario_ids,
            strategy_type: s.strategy_type,
            title: s.title,
            description: s.description,
            estimated_rto_minutes: s.estimated_rto.get(),
            estimated_rpo_minutes: s.estimated_rpo.get(),
            cost_notes: s.cost_notes,
            prerequisites: s.prerequisites,
            implementation_status: s.implementation_status,
            last_tested_at: s.last_tested_at,
            is_selected: s.is_selected,
            gap_justification: s.gap_justification,
            gap_check: gap.map(GapCheckDto::from),
            accepted_gap_action_item_id: s.accepted_gap_action_item_id,
        }
    }
}

impl TryFrom<StrategyDto> for RecoveryStrategy {
    type Error = AppError;

    fn try_from(d: StrategyDto) -> AppResult<Self> {
        Ok(RecoveryStrategy {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            scenario_ids: if d.scenario_ids.is_empty() {
                d.scenario_id.into_iter().collect()
            } else {
                d.scenario_ids
            },
            strategy_type: d.strategy_type,
            title: d.title,
            description: d.description,
            estimated_rto: Minutes::new(d.estimated_rto_minutes)?,
            estimated_rpo: Minutes::new(d.estimated_rpo_minutes)?,
            cost_notes: d.cost_notes,
            prerequisites: d.prerequisites,
            implementation_status: d.implementation_status,
            last_tested_at: d.last_tested_at,
            is_selected: d.is_selected,
            gap_justification: d.gap_justification,
            accepted_gap_action_item_id: d.accepted_gap_action_item_id,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrategyFieldsDto {
    /// Scenarios the measure covers (at least one).
    #[validate(length(min = 1, max = 100))]
    pub scenario_ids: Option<Vec<Uuid>>,
    /// Deprecated single-scenario form of `scenarioIds`.
    pub scenario_id: Option<Uuid>,
    #[serde(rename = "type")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub strategy_type: Option<StrategyType>,
    #[validate(length(max = 300))]
    pub title: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    #[validate(range(max = 5_270_400))]
    pub estimated_rto_minutes: Option<u32>,
    #[validate(range(max = 5_270_400))]
    pub estimated_rpo_minutes: Option<u32>,
    #[validate(length(max = 5000))]
    pub cost_notes: Option<String>,
    #[validate(length(max = 50))]
    pub prerequisites: Option<Vec<String>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub implementation_status: Option<ImplementationStatus>,
    #[serde(default, with = "nullable::field")]
    pub last_tested_at: Option<Option<DateTime<Utc>>>,
}

impl StrategyFieldsDto {
    fn into_input(self) -> AppResult<StrategyInput> {
        Ok(StrategyInput {
            scenario_ids: self.scenario_ids.or(self.scenario_id.map(|id| vec![id])),
            strategy_type: self.strategy_type,
            title: self.title,
            description: self.description,
            estimated_rto: self.estimated_rto_minutes.map(Minutes::new).transpose()?,
            estimated_rpo: self.estimated_rpo_minutes.map(Minutes::new).transpose()?,
            cost_notes: self.cost_notes,
            prerequisites: self.prerequisites,
            implementation_status: self.implementation_status,
            last_tested_at: self.last_tested_at,
        })
    }
}

#[derive(Debug, Default, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionDto {
    #[serde(default)]
    pub accept_gap: bool,
    #[validate(length(max = 5000))]
    pub gap_justification: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub scenario_id: Option<Uuid>,
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<StrategyDto>>> {
    let items = app
        .strategies
        .list(&ctx, microservice_id, q.scenario_id)
        .await?;
    Ok(Json(
        items
            .into_iter()
            .map(|(s, g)| StrategyDto::new(s, Some(g)))
            .collect(),
    ))
}

async fn list_by_service(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<StrategyDto>>> {
    let items = app.strategies.list_by_service(&ctx, service_id).await?;
    Ok(Json(
        items
            .into_iter()
            .map(|(s, g)| StrategyDto::new(s, Some(g)))
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StrategyFieldsDto>,
) -> ApiResult<Response> {
    let input = dto.into_input()?;
    let (Some(scenario_ids), Some(strategy_type), Some(rto), Some(rpo)) = (
        input.scenario_ids.clone(),
        input.strategy_type,
        input.estimated_rto,
        input.estimated_rpo,
    ) else {
        return Err(AppError::invalid(
            "REQUIRED",
            None,
            "scenarioIds, type, estimatedRtoMinutes and estimatedRpoMinutes are required",
        )
        .into());
    };
    let (s, gap) = app
        .strategies
        .create(
            &ctx,
            microservice_id,
            scenario_ids,
            strategy_type,
            rto,
            rpo,
            input,
        )
        .await?;
    let (id, version) = (s.meta.id, s.meta.version);
    Ok(created(
        format!("/recovery-strategies/{id}"),
        Some(version),
        StrategyDto::new(s, Some(gap)),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<StrategyDto>> {
    let (s, gap) = app.strategies.get(&ctx, id).await?;
    Ok(Json(StrategyDto::new(s, Some(gap))))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StrategyFieldsDto>,
) -> ApiResult<Json<StrategyDto>> {
    let (s, gap) = app.strategies.update(&ctx, id, dto.into_input()?).await?;
    Ok(Json(StrategyDto::new(s, Some(gap))))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.strategies.delete(&ctx, id).await?;
    Ok(no_content())
}

async fn select(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    OptionalJson(dto): OptionalJson<SelectionDto>,
) -> ApiResult<Json<StrategyDto>> {
    let (s, gap) = app
        .strategies
        .select(&ctx, id, dto.accept_gap, dto.gap_justification)
        .await?;
    Ok(Json(StrategyDto::new(s, Some(gap))))
}

/// Snapshots from before implementation status existed default to "not implemented".
fn not_implemented() -> ImplementationStatus {
    ImplementationStatus::NotImplemented
}
