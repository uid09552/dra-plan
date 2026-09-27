use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use uuid::Uuid;
use validator::Validate;

use super::domain::{ObjectiveInput, RecoveryObjective};
use crate::app::AppState;
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiResult, MetaDto, ProvenanceDto, ValidJson, created, no_content,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/microservices/{microservice_id}/recovery-objectives",
            get(list).post(create),
        )
        .route(
            "/recovery-objectives/{objective_id}",
            patch(update).delete(delete),
        )
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub microservice_id: Uuid,
    pub scenario_id: Option<Uuid>,
    pub rto_minutes: u32,
    pub rpo_minutes: u32,
    pub mttr_target_minutes: Option<u32>,
    pub restore_priority: Option<u32>,
    pub first_functions: Vec<String>,
}

impl From<RecoveryObjective> for ObjectiveDto {
    fn from(o: RecoveryObjective) -> Self {
        ObjectiveDto {
            meta: o.meta.into(),
            provenance: o.provenance.into(),
            microservice_id: o.microservice_id,
            scenario_id: o.scenario_id,
            rto_minutes: o.rto.get(),
            rpo_minutes: o.rpo.get(),
            mttr_target_minutes: o.mttr_target.map(Minutes::get),
            restore_priority: o.restore_priority,
            first_functions: o.first_functions,
        }
    }
}

impl TryFrom<ObjectiveDto> for RecoveryObjective {
    type Error = AppError;

    fn try_from(d: ObjectiveDto) -> AppResult<Self> {
        Ok(RecoveryObjective {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            microservice_id: d.microservice_id,
            scenario_id: d.scenario_id,
            rto: Minutes::new(d.rto_minutes)?,
            rpo: Minutes::new(d.rpo_minutes)?,
            mttr_target: d.mttr_target_minutes.map(Minutes::new).transpose()?,
            restore_priority: d.restore_priority,
            first_functions: d.first_functions,
        })
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectiveFieldsDto {
    #[serde(default, with = "nullable::field")]
    pub scenario_id: Option<Option<Uuid>>,
    #[validate(range(max = 5_270_400))]
    pub rto_minutes: Option<u32>,
    #[validate(range(max = 5_270_400))]
    pub rpo_minutes: Option<u32>,
    #[serde(default, with = "nullable::field")]
    pub mttr_target_minutes: Option<Option<u32>>,
    #[validate(range(min = 1, max = 10000))]
    pub restore_priority: Option<u32>,
    #[validate(length(max = 50))]
    pub first_functions: Option<Vec<String>>,
}

impl ObjectiveFieldsDto {
    fn into_input(self) -> AppResult<(ObjectiveInput, Option<Minutes>, Option<Minutes>)> {
        let rto = self.rto_minutes.map(Minutes::new).transpose()?;
        let rpo = self.rpo_minutes.map(Minutes::new).transpose()?;
        let input = ObjectiveInput {
            scenario_id: self.scenario_id,
            rto,
            rpo,
            mttr_target: self
                .mttr_target_minutes
                .map(|v| v.map(Minutes::new).transpose())
                .transpose()?,
            restore_priority: self.restore_priority,
            first_functions: self.first_functions,
        };
        Ok((input, rto, rpo))
    }
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<ObjectiveDto>>> {
    Ok(Json(
        app.objectives
            .list(&ctx, microservice_id)
            .await?
            .into_iter()
            .map(ObjectiveDto::from)
            .collect(),
    ))
}

async fn create(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(microservice_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ObjectiveFieldsDto>,
) -> ApiResult<Response> {
    let (input, rto, rpo) = dto.into_input()?;
    let (Some(rto), Some(rpo)) = (rto, rpo) else {
        return Err(AppError::invalid(
            "REQUIRED",
            Some("/rtoMinutes"),
            "rtoMinutes and rpoMinutes are required",
        )
        .into());
    };
    let o = app
        .objectives
        .create(&ctx, microservice_id, rto, rpo, input)
        .await?;
    Ok(created(
        format!("/recovery-objectives/{}", o.meta.id),
        Some(o.meta.version),
        ObjectiveDto::from(o),
    ))
}

async fn update(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<ObjectiveFieldsDto>,
) -> ApiResult<Json<ObjectiveDto>> {
    let (input, _, _) = dto.into_input()?;
    Ok(Json(app.objectives.update(&ctx, id, input).await?.into()))
}

async fn delete(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.objectives.delete(&ctx, id).await?;
    Ok(no_content())
}
