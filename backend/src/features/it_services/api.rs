use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{
    ImpactLevel, ItService, ItServiceInput, LifecycleStatus, ProtectionRequirement, ServiceFilter,
    ServiceSummary,
};
use crate::app::AppState;
use crate::features::workflow::domain::WorkflowStepKey;
use crate::shared::kernel::TenantContext;
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, IfMatch, MetaDto, PageDto, PageQuery, ProvenanceDto, ValidJson,
    created, no_content, with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services", get(list).post(create))
        .route(
            "/services/{service_id}",
            get(get_one).patch(update).delete(delete),
        )
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummaryDto {
    pub microservice_count: i64,
    pub selected_scenario_count: i64,
    pub workflow_completion_percent: i64,
    pub current_plan_version_id: Option<Uuid>,
    pub current_plan_status: Option<String>,
    pub next_review_due: Option<NaiveDate>,
    pub last_tested_at: Option<DateTime<Utc>>,
    pub open_action_item_count: i64,
    pub active_recovery_run_id: Option<Uuid>,
}

impl From<ServiceSummary> for ServiceSummaryDto {
    fn from(s: ServiceSummary) -> Self {
        let steps = WorkflowStepKey::ALL.len() as i64;
        ServiceSummaryDto {
            microservice_count: s.microservice_count,
            selected_scenario_count: s.selected_scenario_count,
            workflow_completion_percent: (s.completed_workflow_steps * 100 / steps).min(100),
            current_plan_version_id: s.current_plan_version_id,
            current_plan_status: s.current_plan_status,
            next_review_due: s.next_review_due,
            last_tested_at: s.last_tested_at,
            open_action_item_count: s.open_action_item_count,
            active_recovery_run_id: s.active_recovery_run_id,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItServiceDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub name: String,
    pub description: Option<String>,
    pub business_owner_id: Option<Uuid>,
    pub technical_owner_id: Option<Uuid>,
    pub consumers: Vec<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub protection_requirement_availability: Option<ProtectionRequirement>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub impact_level: Option<ImpactLevel>,
    #[serde_as(as = "DisplayFromStr")]
    pub lifecycle_status: LifecycleStatus,
    #[serde(default)]
    pub summary: Option<ServiceSummaryDto>,
}

impl ItServiceDto {
    pub fn new(s: ItService, summary: Option<ServiceSummary>) -> Self {
        ItServiceDto {
            meta: s.meta.into(),
            provenance: s.provenance.into(),
            name: s.name,
            description: s.description,
            business_owner_id: s.business_owner_id,
            technical_owner_id: s.technical_owner_id,
            consumers: s.consumers,
            protection_requirement_availability: s.protection_requirement_availability,
            impact_level: s.impact_level,
            lifecycle_status: s.lifecycle_status,
            summary: summary.map(ServiceSummaryDto::from),
        }
    }
}

impl From<ItServiceDto> for ItService {
    fn from(d: ItServiceDto) -> Self {
        ItService {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            name: d.name,
            description: d.description,
            business_owner_id: d.business_owner_id,
            technical_owner_id: d.technical_owner_id,
            consumers: d.consumers,
            protection_requirement_availability: d.protection_requirement_availability,
            impact_level: d.impact_level,
            lifecycle_status: d.lifecycle_status,
        }
    }
}

/// Create and merge-patch body (`name` is required on create; checked by the domain).
#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItServiceFieldsDto {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    #[serde(default, with = "nullable::field")]
    pub business_owner_id: Option<Option<Uuid>>,
    #[serde(default, with = "nullable::field")]
    pub technical_owner_id: Option<Option<Uuid>>,
    #[validate(length(max = 100))]
    pub consumers: Option<Vec<String>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub protection_requirement_availability: Option<ProtectionRequirement>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub impact_level: Option<ImpactLevel>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub lifecycle_status: Option<LifecycleStatus>,
}

impl From<ItServiceFieldsDto> for ItServiceInput {
    fn from(d: ItServiceFieldsDto) -> Self {
        ItServiceInput {
            name: d.name,
            description: d.description,
            business_owner_id: d.business_owner_id,
            technical_owner_id: d.technical_owner_id,
            consumers: d.consumers,
            protection_requirement_availability: d.protection_requirement_availability,
            impact_level: d.impact_level,
            lifecycle_status: d.lifecycle_status,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub q: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub lifecycle_status: Option<LifecycleStatus>,
    pub review_overdue: Option<bool>,
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<PageDto<ItServiceDto>>> {
    let page = PageQuery {
        cursor: q.cursor,
        limit: q.limit,
    }
    .to_request()?;
    let filter = ServiceFilter {
        query: q.q.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
        lifecycle_status: q.lifecycle_status,
        review_overdue: q.review_overdue,
    };
    let page = app.services.list(&ctx, &filter, page).await?;
    Ok(Json(PageDto::from_page(page, |(s, summary)| {
        ItServiceDto::new(s, Some(summary))
    })))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<ItServiceFieldsDto>,
) -> ApiResult<Response> {
    let service = app.services.create(&ctx, dto.into()).await?;
    let version = service.meta.version;
    Ok(created(
        format!("/services/{}", service.meta.id),
        Some(version),
        ItServiceDto::new(service, None),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let (service, summary) = app.services.get_with_summary(&ctx, id).await?;
    Ok(with_etag(
        service.meta.version,
        ItServiceDto::new(service, Some(summary)),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<ItServiceFieldsDto>,
) -> ApiResult<Response> {
    let service = app.services.update(&ctx, id, if_match, dto.into()).await?;
    Ok(with_etag(
        service.meta.version,
        ItServiceDto::new(service, None),
    ))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
) -> ApiResult<Response> {
    app.services.delete(&ctx, id, if_match).await?;
    Ok(no_content())
}
