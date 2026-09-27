use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{Microservice, MicroserviceCriticality, MicroserviceInput};
use crate::app::AppState;
use crate::shared::kernel::TenantContext;
use crate::shared::web::{
    ApiPath, ApiResult, IfMatch, MetaDto, ProvenanceDto, ValidJson, created, no_content, with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/{service_id}/microservices",
            get(list).post(create),
        )
        .route(
            "/microservices/{microservice_id}",
            get(get_one).patch(update).delete(delete),
        )
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroserviceDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub service_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_team: Option<String>,
    pub platform: Option<String>,
    pub hosting_location: Option<String>,
    pub data_stores: Vec<String>,
    pub restore_order: Option<u32>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub criticality_within_service: Option<MicroserviceCriticality>,
    /// The default component that stands for the whole service (read-only).
    #[serde(default)]
    pub is_default: bool,
}

impl From<Microservice> for MicroserviceDto {
    fn from(m: Microservice) -> Self {
        MicroserviceDto {
            meta: m.meta.into(),
            provenance: m.provenance.into(),
            service_id: m.service_id,
            name: m.name,
            description: m.description,
            owner_team: m.owner_team,
            platform: m.platform,
            hosting_location: m.hosting_location,
            data_stores: m.data_stores,
            restore_order: m.restore_order,
            criticality_within_service: m.criticality_within_service,
            is_default: m.is_default,
        }
    }
}

impl From<MicroserviceDto> for Microservice {
    fn from(d: MicroserviceDto) -> Self {
        Microservice {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            service_id: d.service_id,
            name: d.name,
            description: d.description,
            owner_team: d.owner_team,
            platform: d.platform,
            hosting_location: d.hosting_location,
            data_stores: d.data_stores,
            restore_order: d.restore_order,
            criticality_within_service: d.criticality_within_service,
            is_default: d.is_default,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MicroserviceFieldsDto {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    #[validate(length(max = 200))]
    pub owner_team: Option<String>,
    #[validate(length(max = 200))]
    pub platform: Option<String>,
    #[validate(length(max = 200))]
    pub hosting_location: Option<String>,
    #[validate(length(max = 50))]
    pub data_stores: Option<Vec<String>>,
    #[validate(range(min = 1, max = 10000))]
    pub restore_order: Option<u32>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub criticality_within_service: Option<MicroserviceCriticality>,
}

impl From<MicroserviceFieldsDto> for MicroserviceInput {
    fn from(d: MicroserviceFieldsDto) -> Self {
        MicroserviceInput {
            name: d.name,
            description: d.description,
            owner_team: d.owner_team,
            platform: d.platform,
            hosting_location: d.hosting_location,
            data_stores: d.data_stores,
            restore_order: d.restore_order,
            criticality_within_service: d.criticality_within_service,
        }
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<MicroserviceDto>>> {
    Ok(Json(
        app.microservices
            .list(&ctx, service_id)
            .await?
            .into_iter()
            .map(MicroserviceDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<MicroserviceFieldsDto>,
) -> ApiResult<Response> {
    let m = app
        .microservices
        .create(&ctx, service_id, dto.into())
        .await?;
    Ok(created(
        format!("/microservices/{}", m.meta.id),
        Some(m.meta.version),
        MicroserviceDto::from(m),
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let m = app.microservices.get(&ctx, id).await?;
    Ok(with_etag(m.meta.version, MicroserviceDto::from(m)))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<MicroserviceFieldsDto>,
) -> ApiResult<Response> {
    let m = app
        .microservices
        .update(&ctx, id, if_match, dto.into())
        .await?;
    Ok(with_etag(m.meta.version, MicroserviceDto::from(m)))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
) -> ApiResult<Response> {
    app.microservices.delete(&ctx, id, if_match).await?;
    Ok(no_content())
}
