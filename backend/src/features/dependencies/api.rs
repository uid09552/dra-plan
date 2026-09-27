use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::application::DependencyView;
use super::domain::{
    Dependency, DependencyCriticality, DependencyDirection, DependencyGraph, DependencyInput,
    DependencyKind,
};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiResult, MetaDto, ProvenanceDto, ValidJson, created, no_content,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/microservices/{microservice_id}/dependencies",
            get(list).post(create),
        )
        .route(
            "/dependencies/{dependency_id}",
            patch(update).delete(delete),
        )
        .route("/services/{service_id}/dependency-graph", get(graph))
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub microservice_id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub kind: DependencyKind,
    pub target_microservice_id: Option<Uuid>,
    pub target_name: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub direction: DependencyDirection,
    #[serde_as(as = "DisplayFromStr")]
    pub criticality: DependencyCriticality,
    pub dependency_rto_minutes: Option<u32>,
    pub has_own_dr_plan: bool,
    pub notes: Option<String>,
    #[serde(default)]
    pub rto_conflict: Option<bool>,
}

impl DependencyDto {
    pub fn new(d: Dependency, rto_conflict: Option<bool>) -> Self {
        DependencyDto {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            kind: d.kind,
            target_microservice_id: d.target_microservice_id,
            target_name: d.target_name,
            direction: d.direction,
            criticality: d.criticality,
            dependency_rto_minutes: d.dependency_rto.map(Minutes::get),
            has_own_dr_plan: d.has_own_dr_plan,
            notes: d.notes,
            rto_conflict,
        }
    }
}

impl From<DependencyView> for DependencyDto {
    fn from(v: DependencyView) -> Self {
        DependencyDto::new(v.dependency, Some(v.rto_conflict))
    }
}

impl TryFrom<DependencyDto> for Dependency {
    type Error = AppError;

    fn try_from(d: DependencyDto) -> AppResult<Self> {
        Ok(Dependency {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            kind: d.kind,
            target_microservice_id: d.target_microservice_id,
            target_name: d.target_name,
            direction: d.direction,
            criticality: d.criticality,
            dependency_rto: d.dependency_rto_minutes.map(Minutes::new).transpose()?,
            has_own_dr_plan: d.has_own_dr_plan,
            notes: d.notes,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyFieldsDto {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub kind: Option<DependencyKind>,
    #[serde(default, with = "nullable::field")]
    pub target_microservice_id: Option<Option<Uuid>>,
    #[validate(length(max = 300))]
    pub target_name: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub direction: Option<DependencyDirection>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub criticality: Option<DependencyCriticality>,
    #[serde(default, with = "nullable::field")]
    pub dependency_rto_minutes: Option<Option<u32>>,
    pub has_own_dr_plan: Option<bool>,
    #[validate(length(max = 5000))]
    pub notes: Option<String>,
}

impl DependencyFieldsDto {
    fn into_input(self) -> AppResult<DependencyInput> {
        Ok(DependencyInput {
            kind: self.kind,
            target_microservice_id: self.target_microservice_id,
            target_name: self.target_name,
            direction: self.direction,
            criticality: self.criticality,
            dependency_rto: self
                .dependency_rto_minutes
                .map(|v| v.map(Minutes::new).transpose())
                .transpose()?,
            has_own_dr_plan: self.has_own_dr_plan,
            notes: self.notes,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeDto {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: String,
    pub rto_minutes: Option<u32>,
    /// `shared_critical_dependency` or `critical_without_dr_plan` when the node is a SPOF.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_point_of_failure: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeDto {
    pub from: String,
    pub to: String,
    pub dependency_id: Uuid,
    pub criticality: String,
    pub rto_conflict: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
    pub suggested_restore_order: Vec<Uuid>,
    pub cycles: Vec<Vec<String>>,
}

impl From<DependencyGraph> for DependencyGraphDto {
    fn from(g: DependencyGraph) -> Self {
        DependencyGraphDto {
            nodes: g
                .nodes
                .into_iter()
                .map(|n| GraphNodeDto {
                    id: n.id,
                    node_type: n.node_type,
                    label: n.label,
                    rto_minutes: n.rto.map(Minutes::get),
                    single_point_of_failure: n.single_point_of_failure.map(|r| r.as_str()),
                })
                .collect(),
            edges: g
                .edges
                .into_iter()
                .map(|e| GraphEdgeDto {
                    from: e.from,
                    to: e.to,
                    dependency_id: e.dependency_id,
                    criticality: e.criticality.to_string(),
                    rto_conflict: e.rto_conflict,
                })
                .collect(),
            suggested_restore_order: g.suggested_restore_order,
            cycles: g.cycles,
        }
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<DependencyDto>>> {
    Ok(Json(
        app.dependencies
            .list(&ctx, microservice_id)
            .await?
            .into_iter()
            .map(DependencyDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DependencyFieldsDto>,
) -> ApiResult<Response> {
    let (Some(kind), Some(direction), Some(criticality)) =
        (dto.kind, dto.direction, dto.criticality)
    else {
        return Err(AppError::invalid(
            "REQUIRED",
            None,
            "kind, direction and criticality are required",
        )
        .into());
    };
    let view = app
        .dependencies
        .create(
            &ctx,
            microservice_id,
            kind,
            direction,
            criticality,
            dto.into_input()?,
        )
        .await?;
    let (id, version) = (view.dependency.meta.id, view.dependency.meta.version);
    Ok(created(
        format!("/dependencies/{id}"),
        Some(version),
        DependencyDto::from(view),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DependencyFieldsDto>,
) -> ApiResult<Json<DependencyDto>> {
    Ok(Json(
        app.dependencies
            .update(&ctx, id, dto.into_input()?)
            .await?
            .into(),
    ))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.dependencies.delete(&ctx, id).await?;
    Ok(no_content())
}

async fn graph(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<DependencyGraphDto>> {
    Ok(Json(app.dependencies.graph(&ctx, service_id).await?.into()))
}
