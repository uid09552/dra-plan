use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{
    CommunicationAudience, CommunicationRule, CommunicationRuleInput, CommunicationTrigger,
    RoleAssignment, RoleAssignmentInput,
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
            "/services/{service_id}/role-assignments",
            get(list_assignments).post(assign),
        )
        .route(
            "/role-assignments/{assignment_id}",
            patch(update_assignment).delete(delete_assignment),
        )
        .route(
            "/services/{service_id}/communication-rules",
            get(list_rules).post(create_rule),
        )
        .route(
            "/communication-rules/{rule_id}",
            patch(update_rule).delete(delete_rule),
        )
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAssignmentDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub service_id: Uuid,
    pub role_id: Uuid,
    pub person_id: Uuid,
    pub is_deputy: bool,
    pub escalation_order: Option<u32>,
}

impl From<RoleAssignment> for RoleAssignmentDto {
    fn from(a: RoleAssignment) -> Self {
        RoleAssignmentDto {
            meta: a.meta.into(),
            service_id: a.service_id,
            role_id: a.role_id,
            person_id: a.person_id,
            is_deputy: a.is_deputy,
            escalation_order: a.escalation_order,
        }
    }
}

impl From<RoleAssignmentDto> for RoleAssignment {
    fn from(a: RoleAssignmentDto) -> Self {
        RoleAssignment {
            meta: a.meta.into(),
            service_id: a.service_id,
            role_id: a.role_id,
            person_id: a.person_id,
            is_deputy: a.is_deputy,
            escalation_order: a.escalation_order,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleAssignmentFieldsDto {
    pub role_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub is_deputy: Option<bool>,
    #[validate(range(min = 1, max = 100))]
    pub escalation_order: Option<u32>,
}

impl From<RoleAssignmentFieldsDto> for RoleAssignmentInput {
    fn from(d: RoleAssignmentFieldsDto) -> Self {
        RoleAssignmentInput {
            role_id: d.role_id,
            person_id: d.person_id,
            is_deputy: d.is_deputy,
            escalation_order: d.escalation_order,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationRuleDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    #[serde(flatten)]
    pub provenance: ProvenanceDto,
    pub service_id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    pub trigger: CommunicationTrigger,
    #[serde_as(as = "DisplayFromStr")]
    pub audience: CommunicationAudience,
    pub channel: String,
    pub frequency_minutes: Option<u32>,
    pub responsible_role_id: Uuid,
    pub authorizer_role_id: Option<Uuid>,
    pub template: Option<String>,
}

impl From<CommunicationRule> for CommunicationRuleDto {
    fn from(r: CommunicationRule) -> Self {
        CommunicationRuleDto {
            meta: r.meta.into(),
            provenance: r.provenance.into(),
            service_id: r.service_id,
            trigger: r.trigger,
            audience: r.audience,
            channel: r.channel,
            frequency_minutes: r.frequency.map(Minutes::get),
            responsible_role_id: r.responsible_role_id,
            authorizer_role_id: r.authorizer_role_id,
            template: r.template,
        }
    }
}

impl TryFrom<CommunicationRuleDto> for CommunicationRule {
    type Error = AppError;

    fn try_from(d: CommunicationRuleDto) -> AppResult<Self> {
        Ok(CommunicationRule {
            meta: d.meta.into(),
            provenance: d.provenance.into(),
            service_id: d.service_id,
            trigger: d.trigger,
            audience: d.audience,
            channel: d.channel,
            frequency: d.frequency_minutes.map(Minutes::new).transpose()?,
            responsible_role_id: d.responsible_role_id,
            authorizer_role_id: d.authorizer_role_id,
            template: d.template,
        })
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommunicationRuleFieldsDto {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub trigger: Option<CommunicationTrigger>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub audience: Option<CommunicationAudience>,
    #[validate(length(min = 1, max = 300))]
    pub channel: Option<String>,
    #[serde(default, with = "nullable::field")]
    pub frequency_minutes: Option<Option<u32>>,
    pub responsible_role_id: Option<Uuid>,
    #[serde(default, with = "nullable::field")]
    pub authorizer_role_id: Option<Option<Uuid>>,
    #[validate(length(max = 20000))]
    pub template: Option<String>,
}

impl CommunicationRuleFieldsDto {
    fn into_input(self) -> AppResult<CommunicationRuleInput> {
        Ok(CommunicationRuleInput {
            trigger: self.trigger,
            audience: self.audience,
            channel: self.channel,
            frequency: self
                .frequency_minutes
                .map(|v| v.map(Minutes::new).transpose())
                .transpose()?,
            responsible_role_id: self.responsible_role_id,
            authorizer_role_id: self.authorizer_role_id,
            template: self.template,
        })
    }
}

async fn list_assignments(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<RoleAssignmentDto>>> {
    Ok(Json(
        app.roles_comm
            .list_assignments(&ctx, service_id)
            .await?
            .into_iter()
            .map(RoleAssignmentDto::from)
            .collect(),
    ))
}

async fn assign(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<RoleAssignmentFieldsDto>,
) -> ApiResult<Response> {
    let (Some(role_id), Some(person_id)) = (dto.role_id, dto.person_id) else {
        return Err(AppError::invalid("REQUIRED", None, "roleId and personId are required").into());
    };
    let a = app
        .roles_comm
        .assign(&ctx, service_id, role_id, person_id, dto.into())
        .await?;
    Ok(created(
        format!("/role-assignments/{}", a.meta.id),
        Some(a.meta.version),
        RoleAssignmentDto::from(a),
    ))
}

async fn update_assignment(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<RoleAssignmentFieldsDto>,
) -> ApiResult<Json<RoleAssignmentDto>> {
    Ok(Json(
        app.roles_comm
            .update_assignment(&ctx, id, dto.into())
            .await?
            .into(),
    ))
}

async fn delete_assignment(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.roles_comm.delete_assignment(&ctx, id).await?;
    Ok(no_content())
}

async fn list_rules(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<Vec<CommunicationRuleDto>>> {
    Ok(Json(
        app.roles_comm
            .list_rules(&ctx, service_id)
            .await?
            .into_iter()
            .map(CommunicationRuleDto::from)
            .collect(),
    ))
}

async fn create_rule(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<CommunicationRuleFieldsDto>,
) -> ApiResult<Response> {
    let input = dto.into_input()?;
    let (Some(trigger), Some(audience), Some(responsible)) =
        (input.trigger, input.audience, input.responsible_role_id)
    else {
        return Err(AppError::invalid(
            "REQUIRED",
            None,
            "trigger, audience, channel and responsibleRoleId are required",
        )
        .into());
    };
    let r = app
        .roles_comm
        .create_rule(&ctx, service_id, trigger, audience, responsible, input)
        .await?;
    Ok(created(
        format!("/communication-rules/{}", r.meta.id),
        Some(r.meta.version),
        CommunicationRuleDto::from(r),
    ))
}

async fn update_rule(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<CommunicationRuleFieldsDto>,
) -> ApiResult<Json<CommunicationRuleDto>> {
    Ok(Json(
        app.roles_comm
            .update_rule(&ctx, id, dto.into_input()?)
            .await?
            .into(),
    ))
}

async fn delete_rule(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.roles_comm.delete_rule(&ctx, id).await?;
    Ok(no_content())
}
