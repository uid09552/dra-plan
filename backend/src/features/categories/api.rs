use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use super::domain::{CustomCategory, CustomCategoryInput};
use crate::app::AppState;
use crate::shared::kernel::TenantContext;
use crate::shared::web::{ApiPath, ApiResult, MetaDto, ValidJson, created, no_content};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/categories", get(list).post(create))
        .route(
            "/categories/{category_id}",
            axum::routing::delete(delete_one),
        )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub key: String,
    pub label: String,
}

impl From<CustomCategory> for CategoryDto {
    fn from(c: CustomCategory) -> Self {
        CategoryDto {
            meta: c.meta.into(),
            key: c.key,
            label: c.label,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryFieldsDto {
    #[validate(length(min = 2, max = 40))]
    pub key: String,
    #[validate(length(min = 1, max = 80))]
    pub label: String,
}

impl From<CategoryFieldsDto> for CustomCategoryInput {
    fn from(d: CategoryFieldsDto) -> Self {
        CustomCategoryInput {
            key: Some(d.key),
            label: Some(d.label),
        }
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
) -> ApiResult<Json<Vec<CategoryDto>>> {
    Ok(Json(
        app.categories
            .list(&ctx)
            .await?
            .into_iter()
            .map(CategoryDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<CategoryFieldsDto>,
) -> ApiResult<Response> {
    let category = app.categories.create(&ctx, dto.into()).await?;
    Ok(created(
        format!("/categories/{}", category.meta.id),
        Some(category.meta.version),
        CategoryDto::from(category),
    ))
}

async fn delete_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(category_id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.categories.delete(&ctx, category_id).await?;
    Ok(no_content())
}
