use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{
    Decision, DrRequired, Priority, Scenario, ScenarioDecision, ScenarioFilter, ScenarioInput,
    ScenarioStatus,
};
use crate::app::AppState;
use crate::features::catalog::api::language_from;
use crate::shared::kernel::{AppError, AppResult, Rating, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, IfMatch, MetaDto, ProvenanceDto, ValidJson, created, no_content,
    with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/{service_id}/scenarios", get(list).post(create))
        .route(
            "/services/{service_id}/scenario-suggestions",
            get(suggestions),
        )
        .route(
            "/scenarios/{scenario_id}",
            get(get_one).patch(update).delete(delete),
        )
        .route("/scenarios/{scenario_id}/merge", post(merge))
        .route("/scenarios/{scenario_id}/decision", post(decide))
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub service_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub parent_scenario_id: Option<Uuid>,
    pub affected_microservice_ids: Vec<Uuid>,
    #[serde_as(as = "DisplayFromStr")]
    pub status: ScenarioStatus,
    pub merged_into_id: Option<Uuid>,
    pub likelihood: Option<u8>,
    pub impact: Option<u8>,
    #[serde(default)]
    pub risk_score: Option<u8>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub dr_required: Option<DrRequired>,
    pub decision_rationale: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
}

impl From<Scenario> for ScenarioDto {
    fn from(s: Scenario) -> Self {
        let risk_score = s.risk_score();
        ScenarioDto {
            meta: s.meta.into(),
            provenance: s.provenance.into(),
            service_id: s.service_id,
            title: s.title,
            description: s.description,
            category: s.category,
            parent_scenario_id: s.parent_scenario_id,
            affected_microservice_ids: s.affected_microservice_ids,
            status: s.status,
            merged_into_id: s.merged_into_id,
            likelihood: s.likelihood.map(Rating::get),
            impact: s.impact.map(Rating::get),
            risk_score,
            priority: s.priority,
            dr_required: s.dr_required,
            decision_rationale: s.decision_rationale,
            decided_by: s.decided_by,
            decided_at: s.decided_at,
        }
    }
}

fn rating(v: Option<u8>) -> AppResult<Option<Rating>> {
    v.map(|r| Rating::new(i64::from(r))).transpose()
}

impl TryFrom<ScenarioDto> for Scenario {
    type Error = AppError;

    fn try_from(d: ScenarioDto) -> AppResult<Self> {
        Ok(Scenario {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            service_id: d.service_id,
            title: d.title,
            description: d.description,
            category: d.category,
            parent_scenario_id: d.parent_scenario_id,
            affected_microservice_ids: d.affected_microservice_ids,
            status: d.status,
            merged_into_id: d.merged_into_id,
            likelihood: rating(d.likelihood)?,
            impact: rating(d.impact)?,
            priority: d.priority,
            dr_required: d.dr_required,
            decision_rationale: d.decision_rationale,
            decided_by: d.decided_by,
            decided_at: d.decided_at,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioFieldsDto {
    #[validate(length(min = 1, max = 300))]
    pub title: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    #[validate(length(max = 40))]
    #[serde(default)]
    pub category: Option<String>,
    #[validate(length(max = 500))]
    pub affected_microservice_ids: Option<Vec<Uuid>>,
    /// Parent scenario (sub-scenario in the brainstorming tree); `null` makes it top-level.
    #[serde(default, with = "nullable::field")]
    pub parent_scenario_id: Option<Option<Uuid>>,
    #[validate(range(min = 1, max = 4))]
    pub likelihood: Option<u8>,
    #[validate(range(min = 1, max = 4))]
    pub impact: Option<u8>,
    /// Create only: prefill from a catalog template.
    #[validate(length(max = 100))]
    pub catalog_template_id: Option<String>,
}

impl ScenarioFieldsDto {
    fn into_input(self) -> AppResult<(ScenarioInput, Option<String>)> {
        Ok((
            ScenarioInput {
                title: self.title,
                description: self.description,
                category: self.category,
                parent_scenario_id: self.parent_scenario_id,
                affected_microservice_ids: self.affected_microservice_ids,
                likelihood: rating(self.likelihood)?,
                impact: rating(self.impact)?,
            },
            self.catalog_template_id,
        ))
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeDto {
    #[validate(length(min = 1, max = 100))]
    pub source_scenario_ids: Vec<Uuid>,
    #[validate(length(max = 10000))]
    pub merged_description: Option<String>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionDto {
    #[serde_as(as = "DisplayFromStr")]
    pub decision: Decision,
    #[validate(range(min = 1, max = 4))]
    pub likelihood: Option<u8>,
    #[validate(range(min = 1, max = 4))]
    pub impact: Option<u8>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub dr_required: Option<DrRequired>,
    #[validate(length(min = 1, max = 5000))]
    pub decision_rationale: String,
    #[validate(length(max = 500))]
    pub affected_microservice_ids: Option<Vec<Uuid>>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<ScenarioStatus>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub dr_required: Option<DrRequired>,
}

/// A catalog template proposed for the service; accept by creating a scenario with `catalogTemplateId`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioSuggestionDto {
    pub template_id: &'static str,
    pub category: String,
    pub title: &'static str,
    pub description: &'static str,
    pub relevance: &'static str,
}

async fn suggestions(
    State(app): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<ScenarioSuggestionDto>>> {
    let lang = language_from(&headers);
    let templates = app.scenarios.suggestions(&ctx, service_id).await?;
    Ok(Json(
        templates
            .into_iter()
            .map(|t| ScenarioSuggestionDto {
                template_id: t.id,
                category: t.category.to_string(),
                title: t.title(lang),
                description: t.description(lang),
                relevance: t.relevance.as_str(),
            })
            .collect(),
    ))
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<ScenarioDto>>> {
    let filter = ScenarioFilter {
        status: q.status,
        category: q.category,
        dr_required: q.dr_required,
    };
    Ok(Json(
        app.scenarios
            .list(&ctx, service_id, &filter)
            .await?
            .into_iter()
            .map(ScenarioDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ScenarioFieldsDto>,
) -> ApiResult<Response> {
    let (input, template) = dto.into_input()?;
    let s = app
        .scenarios
        .create(&ctx, service_id, input, template.as_deref())
        .await?;
    Ok(created(
        format!("/scenarios/{}", s.meta.id),
        Some(s.meta.version),
        ScenarioDto::from(s),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let s = app.scenarios.get(&ctx, id).await?;
    Ok(with_etag(s.meta.version, ScenarioDto::from(s)))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<ScenarioFieldsDto>,
) -> ApiResult<Response> {
    let (input, template) = dto.into_input()?;
    if template.is_some() {
        return Err(AppError::invalid(
            "NOT_ALLOWED",
            Some("/catalogTemplateId"),
            "catalogTemplateId is only allowed on create",
        )
        .into());
    }
    let s = app.scenarios.update(&ctx, id, if_match, input).await?;
    Ok(with_etag(s.meta.version, ScenarioDto::from(s)))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.scenarios.delete(&ctx, id).await?;
    Ok(no_content())
}

async fn merge(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<MergeDto>,
) -> ApiResult<Json<ScenarioDto>> {
    Ok(Json(
        app.scenarios
            .merge(&ctx, id, &dto.source_scenario_ids, dto.merged_description)
            .await?
            .into(),
    ))
}

async fn decide(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DecisionDto>,
) -> ApiResult<Json<ScenarioDto>> {
    let decision = ScenarioDecision {
        decision: dto.decision,
        likelihood: rating(dto.likelihood)?,
        impact: rating(dto.impact)?,
        priority: dto.priority,
        dr_required: dto.dr_required,
        rationale: dto.decision_rationale,
        affected_microservice_ids: dto.affected_microservice_ids,
    };
    Ok(Json(app.scenarios.decide(&ctx, id, decision).await?.into()))
}
