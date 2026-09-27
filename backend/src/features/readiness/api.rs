use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_with::skip_serializing_none;
use uuid::Uuid;

use super::application::{ComplianceItem, ServiceReadiness, TenantReadiness};
use super::domain::{NextAction, Readiness};
use crate::app::AppState;
use crate::features::catalog::api::language_from;
use crate::shared::kernel::{Language, TenantContext};
use crate::shared::web::{ApiPath, ApiResult};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/readiness", get(tenant_readiness))
        .route("/services/{service_id}/readiness", get(service_readiness))
        .route("/services/{service_id}/compliance", get(compliance))
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextActionDto {
    pub service_id: Option<Uuid>,
    pub service_name: Option<String>,
    pub step: String,
    pub step_number: u8,
    pub severity: String,
    pub rule_id: String,
    pub message: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
}

impl NextActionDto {
    fn new(a: NextAction, service: Option<(Uuid, String)>) -> Self {
        let (service_id, service_name) =
            service.map_or((None, None), |(id, n)| (Some(id), Some(n)));
        NextActionDto {
            service_id,
            service_name,
            step: a.step.to_string(),
            step_number: a.step_number,
            severity: a.severity.to_string(),
            rule_id: a.rule_id,
            message: a.message,
            entity_type: a.entity_type,
            entity_id: a.entity_id,
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReadinessDto {
    pub service_id: Uuid,
    pub name: String,
    pub score: u8,
    pub critical: usize,
    pub attention: usize,
    pub completed: usize,
    pub total: usize,
    pub last_exercise_at: Option<DateTime<Utc>>,
    pub rto_compliance: Option<u8>,
    pub review_due: bool,
    pub untested_scenarios: usize,
    pub next_actions: Vec<NextActionDto>,
}

impl From<ServiceReadiness> for ServiceReadinessDto {
    fn from(s: ServiceReadiness) -> Self {
        let Readiness {
            score,
            critical,
            attention,
            completed,
            total,
            next_actions,
            last_exercise_at,
            rto_compliance,
            review_due,
            untested_scenarios,
        } = s.readiness;
        ServiceReadinessDto {
            service_id: s.service_id,
            name: s.name,
            score,
            critical,
            attention,
            completed,
            total,
            last_exercise_at,
            rto_compliance,
            review_due,
            untested_scenarios,
            next_actions: next_actions
                .into_iter()
                .map(|a| NextActionDto::new(a, None))
                .collect(),
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantReadinessDto {
    pub score: u8,
    pub critical: usize,
    pub attention: usize,
    pub completed: usize,
    pub total: usize,
    pub last_exercise_at: Option<DateTime<Utc>>,
    pub rto_compliance: Option<u8>,
    pub plans_requiring_review: usize,
    pub untested_scenarios: usize,
    pub next_actions: Vec<NextActionDto>,
    pub services: Vec<ServiceReadinessDto>,
}

impl From<TenantReadiness> for TenantReadinessDto {
    fn from(t: TenantReadiness) -> Self {
        TenantReadinessDto {
            score: t.score,
            critical: t.critical,
            attention: t.attention,
            completed: t.completed,
            total: t.total,
            last_exercise_at: t.last_exercise_at,
            rto_compliance: t.rto_compliance,
            plans_requiring_review: t.plans_requiring_review,
            untested_scenarios: t.untested_scenarios,
            next_actions: t
                .next_actions
                .into_iter()
                .map(|(id, name, a)| NextActionDto::new(a, Some((id, name))))
                .collect(),
            services: t
                .services
                .into_iter()
                .map(ServiceReadinessDto::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceItemDto {
    pub id: &'static str,
    pub framework: &'static str,
    pub reference: &'static str,
    pub title: &'static str,
    pub what_to_do: &'static str,
    pub evidence: &'static str,
    pub steps: Vec<String>,
    pub status: String,
    pub blocking_issues: usize,
}

impl ComplianceItemDto {
    fn new(c: ComplianceItem, lang: Language) -> Self {
        let r = c.requirement;
        ComplianceItemDto {
            id: r.id,
            framework: r.framework,
            reference: r.reference,
            title: r.title.get(lang),
            what_to_do: r.what_to_do.get(lang),
            evidence: r.evidence.get(lang),
            steps: r.steps.iter().map(ToString::to_string).collect(),
            status: c.status.to_string(),
            blocking_issues: c.blocking_issues,
        }
    }
}

async fn tenant_readiness(
    State(app): State<AppState>,
    ctx: TenantContext,
) -> ApiResult<Json<TenantReadinessDto>> {
    Ok(Json(app.readiness.tenant(&ctx).await?.into()))
}

async fn service_readiness(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<ServiceReadinessDto>> {
    Ok(Json(app.readiness.service(&ctx, service_id).await?.into()))
}

async fn compliance(
    State(app): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<ComplianceItemDto>>> {
    let lang = language_from(&headers);
    let items = app.readiness.compliance(&ctx, service_id).await?;
    Ok(Json(
        items
            .into_iter()
            .map(|c| ComplianceItemDto::new(c, lang))
            .collect(),
    ))
}
