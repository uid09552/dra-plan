use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{
    ActionItem, ActionItemFilter, ActionItemInput, ActionItemSource, ActionItemStatus,
    RelatedEntity,
};
use crate::app::AppState;
use crate::shared::kernel::TenantContext;
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, MetaDto, ProvenanceDto, ValidJson, created,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/{service_id}/action-items",
            get(list).post(create),
        )
        .route("/action-items/{action_item_id}", patch(update))
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelatedEntityDto {
    #[serde(rename = "type")]
    #[validate(length(min = 1, max = 100))]
    pub entity_type: String,
    pub id: Uuid,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItemDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub service_id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub source: ActionItemSource,
    pub source_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub owner_person_id: Option<Uuid>,
    pub due_date: Option<NaiveDate>,
    #[serde_as(as = "DisplayFromStr")]
    pub status: ActionItemStatus,
    pub related_entity: Option<RelatedEntityDto>,
}

impl From<ActionItem> for ActionItemDto {
    fn from(a: ActionItem) -> Self {
        ActionItemDto {
            meta: a.meta.into(),
            provenance: a.provenance.into(),
            service_id: a.service_id,
            source: a.source,
            source_id: a.source_id,
            title: a.title,
            description: a.description,
            owner_person_id: a.owner_person_id,
            due_date: a.due_date,
            status: a.status,
            related_entity: a.related_entity.map(|r| RelatedEntityDto {
                entity_type: r.entity_type,
                id: r.id,
            }),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionItemFieldsDto {
    #[validate(length(min = 1, max = 300))]
    pub title: Option<String>,
    #[validate(length(max = 10000))]
    pub description: Option<String>,
    #[serde(default, with = "nullable::field")]
    pub owner_person_id: Option<Option<Uuid>>,
    #[serde(default, with = "nullable::field")]
    pub due_date: Option<Option<NaiveDate>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<ActionItemStatus>,
    #[serde(default, with = "nullable::field")]
    pub related_entity: Option<Option<RelatedEntityDto>>,
}

impl From<ActionItemFieldsDto> for ActionItemInput {
    fn from(d: ActionItemFieldsDto) -> Self {
        ActionItemInput {
            title: d.title,
            description: d.description,
            owner_person_id: d.owner_person_id,
            due_date: d.due_date,
            status: d.status,
            related_entity: d.related_entity.map(|o| {
                o.map(|r| RelatedEntity {
                    entity_type: r.entity_type,
                    id: r.id,
                })
            }),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<ActionItemStatus>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub source: Option<ActionItemSource>,
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<ActionItemDto>>> {
    let filter = ActionItemFilter {
        status: q.status,
        source: q.source,
    };
    Ok(Json(
        app.action_items
            .list(&ctx, service_id, &filter)
            .await?
            .into_iter()
            .map(ActionItemDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ActionItemFieldsDto>,
) -> ApiResult<Response> {
    let item = app
        .action_items
        .create(&ctx, service_id, dto.into())
        .await?;
    Ok(created(
        format!("/action-items/{}", item.meta.id),
        Some(item.meta.version),
        ActionItemDto::from(item),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ActionItemFieldsDto>,
) -> ApiResult<Json<ActionItemDto>> {
    Ok(Json(
        app.action_items.update(&ctx, id, dto.into()).await?.into(),
    ))
}
