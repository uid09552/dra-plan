use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{DataProtection, DataProtectionInput, DataProtectionMethod};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiResult, MetaDto, ProvenanceDto, ValidJson, created, no_content,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/microservices/{microservice_id}/data-protection",
            get(list).post(create),
        )
        .route(
            "/data-protection/{data_protection_id}",
            patch(update).delete(delete),
        )
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataProtectionDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub microservice_id: Uuid,
    pub data_store: String,
    #[serde_as(as = "DisplayFromStr")]
    pub method: DataProtectionMethod,
    pub frequency_minutes: u32,
    pub retention_days: Option<u32>,
    pub offsite: bool,
    pub immutable: bool,
    pub encrypted: bool,
    pub last_restore_test_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub supports_rpo: Option<bool>,
}

impl DataProtectionDto {
    pub fn new(d: DataProtection, supports_rpo: Option<bool>) -> Self {
        DataProtectionDto {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            data_store: d.data_store,
            method: d.method,
            frequency_minutes: d.frequency.get(),
            retention_days: d.retention_days,
            offsite: d.offsite,
            immutable: d.immutable,
            encrypted: d.encrypted,
            last_restore_test_at: d.last_restore_test_at,
            supports_rpo,
        }
    }
}

impl TryFrom<DataProtectionDto> for DataProtection {
    type Error = AppError;

    fn try_from(d: DataProtectionDto) -> AppResult<Self> {
        Ok(DataProtection {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            data_store: d.data_store,
            method: d.method,
            frequency: Minutes::new(d.frequency_minutes)?,
            retention_days: d.retention_days,
            offsite: d.offsite,
            immutable: d.immutable,
            encrypted: d.encrypted,
            last_restore_test_at: d.last_restore_test_at,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataProtectionFieldsDto {
    #[validate(length(min = 1, max = 300))]
    pub data_store: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub method: Option<DataProtectionMethod>,
    #[validate(range(max = 5_270_400))]
    pub frequency_minutes: Option<u32>,
    #[validate(range(max = 36500))]
    pub retention_days: Option<u32>,
    pub offsite: Option<bool>,
    pub immutable: Option<bool>,
    pub encrypted: Option<bool>,
    #[serde(default, with = "nullable::field")]
    pub last_restore_test_at: Option<Option<DateTime<Utc>>>,
}

impl DataProtectionFieldsDto {
    fn into_input(self) -> AppResult<DataProtectionInput> {
        Ok(DataProtectionInput {
            data_store: self.data_store,
            method: self.method,
            frequency: self.frequency_minutes.map(Minutes::new).transpose()?,
            retention_days: self.retention_days,
            offsite: self.offsite,
            immutable: self.immutable,
            encrypted: self.encrypted,
            last_restore_test_at: self.last_restore_test_at,
        })
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<DataProtectionDto>>> {
    let items = app.data_protection.list(&ctx, microservice_id).await?;
    Ok(Json(
        items
            .into_iter()
            .map(|(d, s)| DataProtectionDto::new(d, s))
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DataProtectionFieldsDto>,
) -> ApiResult<Response> {
    let input = dto.into_input()?;
    let (Some(method), Some(frequency)) = (input.method, input.frequency) else {
        return Err(AppError::invalid(
            "REQUIRED",
            None,
            "dataStore, method and frequencyMinutes are required",
        )
        .into());
    };
    let (d, supports) = app
        .data_protection
        .create(&ctx, microservice_id, method, frequency, input)
        .await?;
    let (id, version) = (d.meta.id, d.meta.version);
    Ok(created(
        format!("/data-protection/{id}"),
        Some(version),
        DataProtectionDto::new(d, supports),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<DataProtectionFieldsDto>,
) -> ApiResult<Json<DataProtectionDto>> {
    let (d, supports) = app
        .data_protection
        .update(&ctx, id, dto.into_input()?)
        .await?;
    Ok(Json(DataProtectionDto::new(d, supports)))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.data_protection.delete(&ctx, id).await?;
    Ok(no_content())
}
