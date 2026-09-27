use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;
use validator::Validate;

use super::application::{Closing, Declaration, RunView, StepView};
use super::domain::{
    NewRunEvent, RunEvent, RunEventType, RunMode, RunOutcome, RunStatus, RunStepStatus, Transition,
};
use crate::app::AppState;
use crate::features::dr_tests::api::ResultInputDto;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::{ApiPath, ApiQuery, ApiResult, ValidJson, created};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/{service_id}/recovery-runs",
            get(list).post(declare),
        )
        .route("/recovery-runs/{run_id}", get(get_one))
        .route("/recovery-runs/{run_id}/steps", get(steps))
        .route(
            "/recovery-runs/{run_id}/steps/{step_id}/transition",
            post(transition),
        )
        .route("/recovery-runs/{run_id}/status", post(change_status))
        .route("/recovery-runs/{run_id}/close", post(close))
        .route(
            "/recovery-runs/{run_id}/events",
            get(events).post(add_event),
        )
        .route("/recovery-runs/{run_id}/stream", get(stream))
}

// ───────────────────────────── DTOs ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockDto {
    pub elapsed_minutes: i64,
    pub service_rto_minutes: Option<u32>,
    pub mtpd_minutes: Option<u32>,
    pub remaining_critical_path_minutes: u32,
    pub rto_at_risk: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    pub total: usize,
    pub pending: usize,
    pub blocked: usize,
    pub in_progress: usize,
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRunDto {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub service_id: Option<Uuid>,
    pub plan_version_id: Uuid,
    pub scenario_id: Option<Uuid>,
    pub dr_test_id: Option<Uuid>,
    #[serde_as(as = "DisplayFromStr")]
    pub mode: RunMode,
    #[serde_as(as = "DisplayFromStr")]
    pub status: RunStatus,
    pub microservice_ids: Vec<Uuid>,
    pub declared_by: String,
    pub declared_at: DateTime<Utc>,
    pub recovered_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub outcome: Option<RunOutcome>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub clock: ClockDto,
    pub progress: ProgressDto,
    pub next_status_update_due_at: Option<DateTime<Utc>>,
}

impl From<RunView> for RecoveryRunDto {
    fn from(v: RunView) -> Self {
        let r = v.run;
        RecoveryRunDto {
            id: r.id,
            tenant_id: r.tenant_id.0,
            service_id: r.service_id,
            plan_version_id: r.plan_version_id,
            scenario_id: r.scenario_id,
            dr_test_id: r.dr_test_id,
            mode: r.mode,
            status: r.status,
            microservice_ids: r.microservice_ids,
            declared_by: r.declared_by,
            declared_at: r.declared_at,
            recovered_at: r.recovered_at,
            closed_at: r.closed_at,
            outcome: r.outcome,
            summary: r.summary,
            note: r.note,
            clock: ClockDto {
                elapsed_minutes: v.clock.elapsed_minutes,
                service_rto_minutes: v.clock.service_rto.map(Minutes::get),
                mtpd_minutes: v.clock.mtpd.map(Minutes::get),
                remaining_critical_path_minutes: v.clock.remaining_critical_path.get(),
                rto_at_risk: v.clock.rto_at_risk,
            },
            progress: ProgressDto {
                total: v.progress.total,
                pending: v.progress.pending,
                blocked: v.progress.blocked,
                in_progress: v.progress.in_progress,
                done: v.progress.done,
                skipped: v.progress.skipped,
                failed: v.progress.failed,
            },
            next_status_update_due_at: v.next_status_update_due_at,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepStateDto {
    pub runbook_step_id: Uuid,
    pub runbook_id: Uuid,
    pub microservice_id: Uuid,
    pub order: u32,
    pub seq: u32,
    pub phase: String,
    pub title: String,
    pub instructions: Option<String>,
    pub verification: Option<String>,
    pub owner_role_id: Option<Uuid>,
    pub expected_duration_minutes: Option<u32>,
    pub is_decision_point: bool,
    pub requires_authorization_role_id: Option<Uuid>,
    #[serde_as(as = "DisplayFromStr")]
    pub status: RunStepStatus,
    pub blocked_by: Vec<Uuid>,
    pub assignee_person_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}

impl From<StepView> for RunStepStateDto {
    fn from(v: StepView) -> Self {
        let s = v.state;
        RunStepStateDto {
            runbook_step_id: s.runbook_step_id,
            runbook_id: s.runbook_id,
            microservice_id: s.microservice_id,
            order: s.ord,
            seq: s.seq,
            phase: s.phase.to_string(),
            title: s.title,
            instructions: s.instructions,
            verification: s.verification,
            owner_role_id: s.owner_role_id,
            expected_duration_minutes: s.expected_duration.map(Minutes::get),
            is_decision_point: s.is_decision_point,
            requires_authorization_role_id: s.requires_authorization_role_id,
            status: v.display_status,
            blocked_by: v.blocked_by,
            assignee_person_id: s.assignee_person_id,
            started_at: s.started_at,
            finished_at: s.finished_at,
            note: s.note,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEventDto {
    pub id: String,
    pub run_id: Uuid,
    pub at: DateTime<Utc>,
    pub actor: String,
    #[serde(rename = "type")]
    #[serde_as(as = "DisplayFromStr")]
    pub event_type: RunEventType,
    pub message: Option<String>,
    pub payload: Option<Box<RawValue>>,
}

impl From<RunEvent> for RunEventDto {
    fn from(e: RunEvent) -> Self {
        RunEventDto {
            id: e.id.to_string(),
            run_id: e.run_id,
            at: e.at,
            actor: e.actor,
            event_type: e.event_type,
            message: e.message,
            payload: e.payload.and_then(|p| RawValue::from_string(p).ok()),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeclareDto {
    pub scenario_id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub mode: RunMode,
    pub plan_version_id: Option<Uuid>,
    pub dr_test_id: Option<Uuid>,
    #[validate(length(max = 500))]
    pub microservice_ids: Option<Vec<Uuid>>,
    #[validate(length(max = 5000))]
    pub note: Option<String>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionDto {
    #[serde_as(as = "DisplayFromStr")]
    pub to: RunStepStatus,
    #[validate(length(max = 5000))]
    pub note: Option<String>,
    pub verification_passed: Option<bool>,
    pub assignee_person_id: Option<Uuid>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusChangeDto {
    #[serde_as(as = "DisplayFromStr")]
    pub to: RunStatus,
    #[validate(length(max = 5000))]
    pub note: Option<String>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseDto {
    #[serde_as(as = "DisplayFromStr")]
    pub outcome: RunOutcome,
    #[validate(length(max = 20000))]
    pub summary: Option<String>,
    #[validate(nested)]
    #[serde(default)]
    pub achieved: Vec<ResultInputDto>,
    #[validate(length(max = 100))]
    #[serde(default)]
    pub lessons_learned: Vec<String>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventCreateDto {
    #[serde(rename = "type")]
    #[serde_as(as = "DisplayFromStr")]
    pub event_type: RunEventType,
    #[validate(length(min = 1, max = 10000))]
    pub message: String,
    pub payload: Option<serde_json::Map<String, serde_json::Value>>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub active: Option<bool>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub mode: Option<RunMode>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepsQuery {
    pub microservice_id: Option<Uuid>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<RunStepStatus>,
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub since: Option<DateTime<Utc>>,
}

// ───────────────────────────── Handlers ─────────────────────────────

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<RecoveryRunDto>>> {
    Ok(Json(
        app.recovery_runs
            .list(&ctx, service_id, q.active, q.mode)
            .await?
            .into_iter()
            .map(RecoveryRunDto::from)
            .collect(),
    ))
}

async fn declare(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DeclareDto>,
) -> ApiResult<Response> {
    let view = app
        .recovery_runs
        .declare(
            &ctx,
            service_id,
            Declaration {
                scenario_id: dto.scenario_id,
                mode: dto.mode,
                plan_version_id: dto.plan_version_id,
                dr_test_id: dto.dr_test_id,
                microservice_ids: dto.microservice_ids,
                note: dto.note,
            },
        )
        .await?;
    Ok(created(
        format!("/recovery-runs/{}", view.run.id),
        None,
        RecoveryRunDto::from(view),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<RecoveryRunDto>> {
    Ok(Json(app.recovery_runs.get(&ctx, id).await?.into()))
}

async fn steps(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<StepsQuery>,
) -> ApiResult<Json<Vec<RunStepStateDto>>> {
    Ok(Json(
        app.recovery_runs
            .steps(&ctx, id, q.microservice_id, q.status)
            .await?
            .into_iter()
            .map(RunStepStateDto::from)
            .collect(),
    ))
}

async fn transition(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath((run_id, step_id)): ApiPath<(Uuid, Uuid)>,
    ValidJson(dto): ValidJson<TransitionDto>,
) -> ApiResult<Json<RunStepStateDto>> {
    let t = Transition {
        to: dto.to,
        note: dto.note,
        verification_passed: dto.verification_passed,
        assignee_person_id: dto.assignee_person_id,
    };
    Ok(Json(
        app.recovery_runs
            .transition(&ctx, run_id, step_id, t)
            .await?
            .into(),
    ))
}

async fn change_status(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<StatusChangeDto>,
) -> ApiResult<Json<RecoveryRunDto>> {
    Ok(Json(
        app.recovery_runs
            .change_status(&ctx, id, dto.to, dto.note)
            .await?
            .into(),
    ))
}

async fn close(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<CloseDto>,
) -> ApiResult<Json<RecoveryRunDto>> {
    let achieved = dto
        .achieved
        .into_iter()
        .map(ResultInputDto::into_input)
        .collect::<AppResult<Vec<_>>>()?;
    let closing = Closing {
        outcome: dto.outcome,
        summary: dto.summary,
        achieved,
        lessons_learned: dto.lessons_learned,
    };
    Ok(Json(
        app.recovery_runs.close(&ctx, id, closing).await?.into(),
    ))
}

async fn events(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<EventsQuery>,
) -> ApiResult<Json<Vec<RunEventDto>>> {
    Ok(Json(
        app.recovery_runs
            .events(&ctx, id, None, q.since)
            .await?
            .into_iter()
            .map(RunEventDto::from)
            .collect(),
    ))
}

async fn add_event(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<EventCreateDto>,
) -> ApiResult<Response> {
    if !matches!(
        dto.event_type,
        RunEventType::Note | RunEventType::Decision | RunEventType::CommunicationSent
    ) {
        return Err(AppError::invalid(
            "INVALID_VALUE",
            Some("/type"),
            "only note, decision and communication_sent can be added",
        )
        .into());
    }
    let payload = dto
        .payload
        .map(|p| serde_json::Value::Object(p).to_string());
    let event = app
        .recovery_runs
        .add_event(
            &ctx,
            id,
            NewRunEvent {
                event_type: dto.event_type,
                message: Some(dto.message),
                payload,
            },
        )
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(RunEventDto::from(event)),
    )
        .into_response())
}

fn sse_event(e: RunEvent) -> Event {
    let id = e.id.to_string();
    Event::default()
        .id(id)
        .event("run-event")
        .json_data(RunEventDto::from(e))
        .unwrap_or_else(|_| Event::default().comment("encoding error"))
}

/// Live updates via Server-Sent Events. Replays events after `Last-Event-ID`, then streams new
/// ones. Subscribes before reading the backlog so nothing is lost in between.
async fn stream(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());
    let receiver = app.recovery_runs.bus().subscribe();
    let backlog = app.recovery_runs.events(&ctx, id, last_id, None).await?;
    let mut watermark = backlog.last().map(|e| e.id).or(last_id).unwrap_or(0);
    let tenant = ctx.tenant_id;
    let replay = stream::iter(backlog.into_iter().map(|e| Ok(sse_event(e))));
    let live = BroadcastStream::new(receiver).filter_map(move |msg| {
        let item = match msg {
            Ok((t, e)) if t == tenant && e.run_id == id && e.id > watermark => {
                watermark = e.id;
                Some(Ok(sse_event(e)))
            }
            // Lagged subscribers are told to reconnect (clients resume with Last-Event-ID).
            Err(_) => Some(Ok(Event::default()
                .event("resync")
                .data("reconnect to resume"))),
            _ => None,
        };
        futures::future::ready(item)
    });
    Ok(Sse::new(replay.chain(live)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
