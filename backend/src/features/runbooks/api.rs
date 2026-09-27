use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::application::{NewStep, RunbookSummary, RunbookView};
use super::domain::{Runbook, RunbookInput, RunbookPhase, RunbookStep, StepInput};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, IfMatch, MetaDto, ProvenanceDto, ValidJson, created, no_content,
    with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/{service_id}/runbooks", get(list_for_service))
        .route(
            "/microservices/{microservice_id}/runbooks",
            get(list_for_microservice).post(create),
        )
        .route(
            "/runbooks/{runbook_id}",
            get(get_one).patch(update).delete(delete),
        )
        .route(
            "/runbooks/{runbook_id}/steps",
            get(list_steps).post(add_step),
        )
        .route("/runbooks/{runbook_id}/steps/order", put(reorder))
        .route(
            "/runbook-steps/{step_id}",
            patch(update_step).delete(delete_step),
        )
}

// ───────────────────────────── Step DTOs ─────────────────────────────

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunbookStepDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub runbook_id: Uuid,
    pub seq: u32,
    #[serde_as(as = "DisplayFromStr")]
    pub phase: RunbookPhase,
    pub title: String,
    pub instructions: Option<String>,
    pub owner_role_id: Option<Uuid>,
    pub expected_duration_minutes: Option<u32>,
    pub verification: Option<String>,
    pub depends_on_step_ids: Vec<Uuid>,
    pub is_decision_point: bool,
    pub requires_authorization_role_id: Option<Uuid>,
}

impl From<RunbookStep> for RunbookStepDto {
    fn from(s: RunbookStep) -> Self {
        RunbookStepDto {
            meta: s.meta.into(),
            provenance: s.provenance.into(),
            runbook_id: s.runbook_id,
            seq: s.seq,
            phase: s.phase,
            title: s.title,
            instructions: s.instructions,
            owner_role_id: s.owner_role_id,
            expected_duration_minutes: s.expected_duration.map(Minutes::get),
            verification: s.verification,
            depends_on_step_ids: s.depends_on,
            is_decision_point: s.is_decision_point,
            requires_authorization_role_id: s.requires_authorization_role_id,
        }
    }
}

impl TryFrom<RunbookStepDto> for RunbookStep {
    type Error = AppError;

    fn try_from(d: RunbookStepDto) -> AppResult<Self> {
        Ok(RunbookStep {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            runbook_id: d.runbook_id,
            seq: d.seq,
            phase: d.phase,
            title: d.title,
            instructions: d.instructions,
            owner_role_id: d.owner_role_id,
            expected_duration: d.expected_duration_minutes.map(Minutes::new).transpose()?,
            verification: d.verification,
            depends_on: d.depends_on_step_ids,
            is_decision_point: d.is_decision_point,
            requires_authorization_role_id: d.requires_authorization_role_id,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepFieldsDto {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub phase: Option<RunbookPhase>,
    #[validate(length(min = 1, max = 300))]
    pub title: Option<String>,
    #[validate(length(max = 20000))]
    pub instructions: Option<String>,
    #[serde(default, with = "nullable::field")]
    pub owner_role_id: Option<Option<Uuid>>,
    #[serde(default, with = "nullable::field")]
    pub expected_duration_minutes: Option<Option<u32>>,
    #[validate(length(max = 5000))]
    pub verification: Option<String>,
    #[validate(length(max = 100))]
    pub depends_on_step_ids: Option<Vec<Uuid>>,
    pub is_decision_point: Option<bool>,
    #[serde(default, with = "nullable::field")]
    pub requires_authorization_role_id: Option<Option<Uuid>>,
    /// Create only: 1-based insert position (default: end).
    #[validate(range(min = 1, max = 10000))]
    pub position: Option<u32>,
}

impl StepFieldsDto {
    fn into_input(self) -> AppResult<(StepInput, Option<RunbookPhase>, Option<u32>)> {
        let input = StepInput {
            phase: self.phase,
            title: self.title,
            instructions: self.instructions,
            owner_role_id: self.owner_role_id,
            expected_duration: self
                .expected_duration_minutes
                .map(|v| v.map(Minutes::new).transpose())
                .transpose()?,
            verification: self.verification,
            depends_on: self.depends_on_step_ids,
            is_decision_point: self.is_decision_point,
            requires_authorization_role_id: self.requires_authorization_role_id,
        };
        Ok((input, self.phase, self.position))
    }

    fn into_new_step(self) -> AppResult<NewStep> {
        let (input, phase, position) = self.into_input()?;
        let phase = phase
            .ok_or_else(|| AppError::invalid("REQUIRED", Some("/phase"), "phase is required"))?;
        if input.title.is_none() {
            return Err(AppError::invalid(
                "REQUIRED",
                Some("/title"),
                "title is required",
            ));
        }
        Ok(NewStep {
            phase,
            position,
            input,
        })
    }
}

// ───────────────────────────── Runbook DTOs ─────────────────────────────

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunbookDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub microservice_id: Uuid,
    pub scenario_id: Uuid,
    pub strategy_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub step_count: Option<usize>,
    #[serde(default)]
    pub critical_path_minutes: Option<u32>,
    #[serde(default)]
    pub exceeds_rto: Option<bool>,
    #[serde(default)]
    pub steps: Option<Vec<RunbookStepDto>>,
}

impl RunbookDto {
    pub fn new(
        r: Runbook,
        summary: Option<RunbookSummary>,
        steps: Option<Vec<RunbookStep>>,
    ) -> Self {
        RunbookDto {
            meta: r.meta.into(),
            provenance: r.provenance.into(),
            microservice_id: r.microservice_id,
            scenario_id: r.scenario_id,
            strategy_id: r.strategy_id,
            title: r.title,
            description: r.description,
            step_count: summary.as_ref().map(|s| s.step_count),
            critical_path_minutes: summary.as_ref().map(|s| s.critical_path.get()),
            exceeds_rto: summary.and_then(|s| s.exceeds_rto),
            steps: steps.map(|s| s.into_iter().map(RunbookStepDto::from).collect()),
        }
    }

    fn summary(v: RunbookView) -> Self {
        RunbookDto::new(v.runbook, Some(v.summary), None)
    }

    fn detail(v: RunbookView) -> Self {
        RunbookDto::new(v.runbook, Some(v.summary), Some(v.steps))
    }

    /// Splits into domain runbook and steps (plan snapshots).
    pub fn into_domain(self) -> AppResult<(Runbook, Vec<RunbookStep>)> {
        let steps = self
            .steps
            .unwrap_or_default()
            .into_iter()
            .map(RunbookStep::try_from)
            .collect::<AppResult<_>>()?;
        Ok((
            Runbook {
                meta: self.meta.into(),
                provenance: self.provenance.into(),
                microservice_id: self.microservice_id,
                scenario_id: self.scenario_id,
                strategy_id: self.strategy_id,
                title: self.title,
                description: self.description,
            },
            steps,
        ))
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunbookFieldsDto {
    pub scenario_id: Option<Uuid>,
    #[serde(default, with = "nullable::field")]
    pub strategy_id: Option<Option<Uuid>>,
    #[validate(length(min = 1, max = 300))]
    pub title: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    /// Create only: initial steps in order.
    #[validate(nested)]
    pub steps: Option<Vec<StepFieldsDto>>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepOrderDto {
    #[validate(length(max = 500))]
    pub step_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub scenario_id: Option<Uuid>,
}

// ───────────────────────────── Handlers ─────────────────────────────

async fn list_for_service(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<RunbookDto>>> {
    let views = app
        .runbooks
        .list_for_service(&ctx, service_id, q.scenario_id)
        .await?;
    Ok(Json(views.into_iter().map(RunbookDto::summary).collect()))
}

async fn list_for_microservice(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<RunbookDto>>> {
    let views = app
        .runbooks
        .list_for_microservice(&ctx, microservice_id, q.scenario_id)
        .await?;
    Ok(Json(views.into_iter().map(RunbookDto::summary).collect()))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<RunbookFieldsDto>,
) -> ApiResult<Response> {
    let scenario_id = dto.scenario_id.ok_or_else(|| {
        AppError::invalid("REQUIRED", Some("/scenarioId"), "scenarioId is required")
    })?;
    let steps = dto
        .steps
        .unwrap_or_default()
        .into_iter()
        .map(StepFieldsDto::into_new_step)
        .collect::<AppResult<Vec<_>>>()?;
    let input = RunbookInput {
        scenario_id: None,
        strategy_id: dto.strategy_id,
        title: dto.title,
        description: dto.description,
    };
    let view = app
        .runbooks
        .create(&ctx, microservice_id, scenario_id, input, steps)
        .await?;
    let (id, version) = (view.runbook.meta.id, view.runbook.meta.version);
    Ok(created(
        format!("/runbooks/{id}"),
        Some(version),
        RunbookDto::detail(view),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let view = app.runbooks.get(&ctx, id).await?;
    Ok(with_etag(
        view.runbook.meta.version,
        RunbookDto::detail(view),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<RunbookFieldsDto>,
) -> ApiResult<Response> {
    if dto.steps.is_some() {
        return Err(AppError::invalid(
            "NOT_ALLOWED",
            Some("/steps"),
            "use the step endpoints to change steps",
        )
        .into());
    }
    let input = RunbookInput {
        scenario_id: dto.scenario_id,
        strategy_id: dto.strategy_id,
        title: dto.title,
        description: dto.description,
    };
    let view = app.runbooks.update(&ctx, id, if_match, input).await?;
    Ok(with_etag(
        view.runbook.meta.version,
        RunbookDto::summary(view),
    ))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.runbooks.delete(&ctx, id).await?;
    Ok(no_content())
}

async fn list_steps(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<RunbookStepDto>>> {
    Ok(Json(
        app.runbooks
            .steps(&ctx, id)
            .await?
            .into_iter()
            .map(RunbookStepDto::from)
            .collect(),
    ))
}

async fn add_step(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(runbook_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StepFieldsDto>,
) -> ApiResult<Response> {
    let step = app
        .runbooks
        .add_step(&ctx, runbook_id, dto.into_new_step()?)
        .await?;
    let (id, version) = (step.meta.id, step.meta.version);
    Ok(created(
        format!("/runbook-steps/{id}"),
        Some(version),
        RunbookStepDto::from(step),
    ))
}

async fn reorder(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(runbook_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StepOrderDto>,
) -> ApiResult<Json<Vec<RunbookStepDto>>> {
    let steps = app
        .runbooks
        .reorder(&ctx, runbook_id, &dto.step_ids)
        .await?;
    Ok(Json(steps.into_iter().map(RunbookStepDto::from).collect()))
}

async fn update_step(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StepFieldsDto>,
) -> ApiResult<Json<RunbookStepDto>> {
    let (input, _, position) = dto.into_input()?;
    if position.is_some() {
        return Err(AppError::invalid(
            "NOT_ALLOWED",
            Some("/position"),
            "use PUT /runbooks/{id}/steps/order to move steps",
        )
        .into());
    }
    Ok(Json(
        app.runbooks.update_step(&ctx, id, input).await?.into(),
    ))
}

async fn delete_step(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.runbooks.delete_step(&ctx, id).await?;
    Ok(no_content())
}
