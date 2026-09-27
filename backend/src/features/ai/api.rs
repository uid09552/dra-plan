use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::application::ProposalDecision;
use super::domain::{AiContext, AiSuggestion, AiSuggestionKind, AiSuggestionStatus};
use crate::app::AppState;
use crate::shared::kernel::{AppError, Language, TenantContext};
use crate::shared::web::{API_BASE, ApiPath, ApiQuery, ApiResult, PageDto, PageQuery, ValidJson};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ai/suggestions", get(list).post(request))
        .route("/ai/suggestions/{suggestion_id}", get(get_one))
        .route("/ai/suggestions/{suggestion_id}/decision", post(decide))
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestionDto {
    pub id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub kind: AiSuggestionKind,
    #[serde_as(as = "DisplayFromStr")]
    pub status: AiSuggestionStatus,
    pub service_id: Uuid,
    pub context: AiContextDto,
    pub proposals: Option<Box<RawValue>>,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextDto {
    #[serde_as(as = "DisplayFromStr")]
    pub kind: AiSuggestionKind,
    pub service_id: Uuid,
    pub microservice_id: Option<Uuid>,
    pub scenario_id: Option<Uuid>,
    pub runbook_id: Option<Uuid>,
    pub recovery_run_id: Option<Uuid>,
    pub runbook_step_id: Option<Uuid>,
    pub prompt: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub language: Language,
}

impl From<AiSuggestion> for AiSuggestionDto {
    fn from(s: AiSuggestion) -> Self {
        AiSuggestionDto {
            id: s.id,
            kind: s.kind,
            status: s.status,
            service_id: s.service_id,
            context: AiContextDto {
                kind: s.kind,
                service_id: s.service_id,
                microservice_id: s.context.microservice_id,
                scenario_id: s.context.scenario_id,
                runbook_id: s.context.runbook_id,
                recovery_run_id: s.context.recovery_run_id,
                runbook_step_id: s.context.runbook_step_id,
                prompt: s.context.prompt,
                language: s.language,
            },
            proposals: RawValue::from_string(s.proposals).ok(),
            summary: s.summary,
            model: s.model,
            error: s.error,
            created_by: s.created_by,
            created_at: s.created_at,
            decided_by: s.decided_by,
            decided_at: s.decided_at,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiRequestDto {
    #[serde_as(as = "DisplayFromStr")]
    pub kind: AiSuggestionKind,
    pub service_id: Uuid,
    pub microservice_id: Option<Uuid>,
    pub scenario_id: Option<Uuid>,
    pub runbook_id: Option<Uuid>,
    pub recovery_run_id: Option<Uuid>,
    pub runbook_step_id: Option<Uuid>,
    #[validate(length(max = 4000))]
    pub prompt: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub language: Option<Language>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalDecisionDto {
    #[validate(length(min = 1, max = 100))]
    pub proposal_id: String,
    pub decision: String,
    pub edited_data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiDecisionDto {
    #[validate(nested)]
    pub decisions: Vec<ProposalDecisionDto>,
    #[validate(length(max = 5000))]
    pub comment: Option<String>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub service_id: Option<Uuid>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<AiSuggestionStatus>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<PageDto<AiSuggestionDto>>> {
    let page = PageQuery {
        cursor: q.cursor,
        limit: q.limit,
    }
    .to_request()?;
    let result = app.ai.list(&ctx, q.service_id, q.status, page).await?;
    Ok(Json(PageDto::from_page(result, AiSuggestionDto::from)))
}

async fn request(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<AiRequestDto>,
) -> ApiResult<Response> {
    let context = AiContext {
        microservice_id: dto.microservice_id,
        scenario_id: dto.scenario_id,
        runbook_id: dto.runbook_id,
        recovery_run_id: dto.recovery_run_id,
        runbook_step_id: dto.runbook_step_id,
        prompt: dto.prompt,
    };
    let s = app
        .ai
        .request(
            &ctx,
            dto.service_id,
            dto.kind,
            dto.language.unwrap_or(Language::En),
            context,
        )
        .await?;
    let location = format!("{API_BASE}/ai/suggestions/{}", s.id);
    let mut response = (StatusCode::ACCEPTED, Json(AiSuggestionDto::from(s))).into_response();
    if let Ok(v) = axum::http::HeaderValue::from_str(&location) {
        response
            .headers_mut()
            .insert(axum::http::header::LOCATION, v);
    }
    Ok(response)
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<AiSuggestionDto>> {
    Ok(Json(app.ai.get(&ctx, id).await?.into()))
}

async fn decide(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<AiDecisionDto>,
) -> ApiResult<Json<serde_json::Value>> {
    let decisions = dto
        .decisions
        .iter()
        .map(|d| match d.decision.as_str() {
            "accept" => Ok(ProposalDecision::Accept),
            "edit" => Ok(ProposalDecision::Edit),
            "reject" => Ok(ProposalDecision::Reject),
            other => Err(AppError::invalid(
                "INVALID_VALUE",
                Some("/decisions"),
                format!("unknown decision `{other}`"),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let s = app.ai.decide(&ctx, id, &decisions).await?;
    Ok(Json(
        serde_json::json!({ "suggestion": AiSuggestionDto::from(s), "applied": [] }),
    ))
}
