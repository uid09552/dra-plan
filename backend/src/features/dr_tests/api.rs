use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::application::ResultInput;
use super::domain::{DrTest, DrTestInput, DrTestOutcome, DrTestResult, DrTestType};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{ApiPath, ApiResult, MetaDto, ValidJson, created, no_content};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/{service_id}/dr-tests", get(list).post(create))
        .route(
            "/dr-tests/{dr_test_id}",
            get(get_one).patch(update).delete(delete),
        )
        .route(
            "/dr-tests/{dr_test_id}/results",
            get(results).put(record_results),
        )
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrTestDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub service_id: Uuid,
    #[serde(rename = "type")]
    #[serde_as(as = "DisplayFromStr")]
    pub test_type: DrTestType,
    pub scenario_id: Uuid,
    pub plan_version_id: Option<Uuid>,
    pub planned_at: DateTime<Utc>,
    pub executed_at: Option<DateTime<Utc>>,
    pub participant_ids: Vec<Uuid>,
    #[serde_as(as = "DisplayFromStr")]
    pub outcome: DrTestOutcome,
    pub report: Option<String>,
    pub recovery_run_id: Option<Uuid>,
}

impl From<DrTest> for DrTestDto {
    fn from(t: DrTest) -> Self {
        DrTestDto {
            meta: t.meta.into(),
            service_id: t.service_id,
            test_type: t.test_type,
            scenario_id: t.scenario_id,
            plan_version_id: t.plan_version_id,
            planned_at: t.planned_at,
            executed_at: t.executed_at,
            participant_ids: t.participant_ids,
            outcome: t.outcome,
            report: t.report,
            recovery_run_id: t.recovery_run_id,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrTestFieldsDto {
    #[serde(rename = "type")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub test_type: Option<DrTestType>,
    pub scenario_id: Option<Uuid>,
    pub plan_version_id: Option<Uuid>,
    pub planned_at: Option<DateTime<Utc>>,
    #[serde(default, with = "nullable::field")]
    pub executed_at: Option<Option<DateTime<Utc>>>,
    #[validate(length(max = 200))]
    pub participant_ids: Option<Vec<Uuid>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub outcome: Option<DrTestOutcome>,
    #[validate(length(max = 50000))]
    pub report: Option<String>,
}

impl From<DrTestFieldsDto> for DrTestInput {
    fn from(d: DrTestFieldsDto) -> Self {
        DrTestInput {
            test_type: d.test_type,
            scenario_id: d.scenario_id,
            plan_version_id: d.plan_version_id,
            planned_at: d.planned_at,
            executed_at: d.executed_at,
            participant_ids: d.participant_ids,
            outcome: d.outcome,
            report: d.report,
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrTestResultDto {
    pub microservice_id: Uuid,
    pub achieved_rto_minutes: Option<u32>,
    pub achieved_rpo_minutes: Option<u32>,
    pub notes: Option<String>,
    pub target_rto_minutes: Option<u32>,
    pub target_rpo_minutes: Option<u32>,
    pub rto_met: Option<bool>,
    pub rpo_met: Option<bool>,
    pub passed: bool,
}

impl From<DrTestResult> for DrTestResultDto {
    fn from(r: DrTestResult) -> Self {
        DrTestResultDto {
            microservice_id: r.microservice_id,
            achieved_rto_minutes: r.achieved_rto.map(Minutes::get),
            achieved_rpo_minutes: r.achieved_rpo.map(Minutes::get),
            target_rto_minutes: r.target_rto.map(Minutes::get),
            target_rpo_minutes: r.target_rpo.map(Minutes::get),
            rto_met: r.rto_met(),
            rpo_met: r.rpo_met(),
            passed: r.passed(),
            notes: r.notes,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultInputDto {
    pub microservice_id: Uuid,
    #[validate(range(max = 5_270_400))]
    pub achieved_rto_minutes: Option<u32>,
    #[validate(range(max = 5_270_400))]
    pub achieved_rpo_minutes: Option<u32>,
    #[validate(length(max = 5000))]
    pub notes: Option<String>,
}

impl ResultInputDto {
    pub fn into_input(self) -> AppResult<ResultInput> {
        Ok(ResultInput {
            microservice_id: self.microservice_id,
            achieved_rto: self.achieved_rto_minutes.map(Minutes::new).transpose()?,
            achieved_rpo: self.achieved_rpo_minutes.map(Minutes::new).transpose()?,
            notes: self.notes,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct ResultsDto(pub Vec<ResultInputDto>);

impl Validate for ResultsDto {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        self.0.iter().try_for_each(Validate::validate)
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<DrTestDto>>> {
    Ok(Json(
        app.dr_tests
            .list(&ctx, service_id)
            .await?
            .into_iter()
            .map(DrTestDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DrTestFieldsDto>,
) -> ApiResult<Response> {
    let (Some(test_type), Some(scenario_id), Some(planned_at)) =
        (dto.test_type, dto.scenario_id, dto.planned_at)
    else {
        return Err(AppError::invalid(
            "REQUIRED",
            None,
            "type, scenarioId and plannedAt are required",
        )
        .into());
    };
    let t = app
        .dr_tests
        .create(
            &ctx,
            service_id,
            test_type,
            scenario_id,
            planned_at,
            dto.into(),
        )
        .await?;
    Ok(created(
        format!("/dr-tests/{}", t.meta.id),
        Some(t.meta.version),
        DrTestDto::from(t),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<DrTestDto>> {
    Ok(Json(app.dr_tests.get(&ctx, id).await?.into()))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DrTestFieldsDto>,
) -> ApiResult<Json<DrTestDto>> {
    Ok(Json(
        app.dr_tests.update(&ctx, id, dto.into()).await?.into(),
    ))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.dr_tests.delete(&ctx, id).await?;
    Ok(no_content())
}

async fn results(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<DrTestResultDto>>> {
    Ok(Json(
        app.dr_tests
            .results(&ctx, id)
            .await?
            .into_iter()
            .map(DrTestResultDto::from)
            .collect(),
    ))
}

async fn record_results(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ResultsDto>,
) -> ApiResult<Json<Vec<DrTestResultDto>>> {
    let inputs = dto
        .0
        .into_iter()
        .map(ResultInputDto::into_input)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Json(
        app.dr_tests
            .record_results(&ctx, id, inputs)
            .await?
            .into_iter()
            .map(DrTestResultDto::from)
            .collect(),
    ))
}
