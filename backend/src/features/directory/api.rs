use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::domain::{Member, MemberPatch, NewMember, Person, PersonInput, Role, RoleInput};
use crate::app::AppState;
use crate::shared::kernel::{TenantContext, TenantRole};
use crate::shared::web::dto::nullable;
use crate::shared::web::{
    ApiPath, ApiQuery, ApiResult, IfMatch, MetaDto, PageDto, PageQuery, ValidJson, created,
    no_content, with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/members", get(list_members).post(add_member))
        .route(
            "/members/{member_id}",
            patch(update_member).delete(remove_member),
        )
        .route("/persons", get(list_persons).post(create_person))
        .route(
            "/persons/{person_id}",
            get(get_person).patch(update_person).delete(delete_person),
        )
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{role_id}", patch(update_role).delete(delete_role))
}

// ───────────────────────────── DTOs ─────────────────────────────

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub alternate_contact: Option<String>,
    pub team: Option<String>,
}

impl From<Person> for PersonDto {
    fn from(p: Person) -> Self {
        PersonDto {
            meta: p.meta.into(),
            name: p.name,
            email: p.email,
            phone: p.phone,
            alternate_contact: p.alternate_contact,
            team: p.team,
        }
    }
}

impl From<PersonDto> for Person {
    fn from(p: PersonDto) -> Self {
        Person {
            meta: p.meta.into(),
            name: p.name,
            email: p.email,
            phone: p.phone,
            alternate_contact: p.alternate_contact,
            team: p.team,
        }
    }
}

/// Used for create (name required) and merge-patch.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonFieldsDto {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(length(max = 100))]
    pub phone: Option<String>,
    #[validate(length(max = 500))]
    pub alternate_contact: Option<String>,
    #[validate(length(max = 200))]
    pub team: Option<String>,
}

impl From<PersonFieldsDto> for PersonInput {
    fn from(d: PersonFieldsDto) -> Self {
        PersonInput {
            name: d.name,
            email: d.email,
            phone: d.phone,
            alternate_contact: d.alternate_contact,
            team: d.team,
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub name: String,
    pub description: Option<String>,
    pub is_default: bool,
}

impl From<Role> for RoleDto {
    fn from(r: Role) -> Self {
        RoleDto {
            meta: r.meta.into(),
            name: r.name,
            description: r.description,
            is_default: r.is_default,
        }
    }
}

impl From<RoleDto> for Role {
    fn from(r: RoleDto) -> Self {
        Role {
            meta: r.meta.into(),
            name: r.name,
            description: r.description,
            is_default: r.is_default,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleFieldsDto {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(length(max = 1000))]
    pub description: Option<String>,
}

impl From<RoleFieldsDto> for RoleInput {
    fn from(d: RoleFieldsDto) -> Self {
        RoleInput {
            name: d.name,
            description: d.description,
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub user_ref: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub tenant_role: TenantRole,
    pub person_id: Option<Uuid>,
}

impl From<Member> for MemberDto {
    fn from(m: Member) -> Self {
        MemberDto {
            meta: m.meta.into(),
            user_ref: m.user_ref,
            email: m.email,
            display_name: m.display_name,
            tenant_role: m.tenant_role,
            person_id: m.person_id,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberCreateDto {
    #[validate(length(min = 1, max = 255))]
    pub user_ref: String,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(length(max = 200))]
    pub display_name: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub tenant_role: TenantRole,
    pub person_id: Option<Uuid>,
}

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberUpdateDto {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub tenant_role: Option<TenantRole>,
    #[serde(default, with = "nullable::field")]
    pub person_id: Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct PersonListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub q: Option<String>,
}

// ───────────────────────────── Handlers ─────────────────────────────

async fn list_members(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> ApiResult<Json<PageDto<MemberDto>>> {
    let page = app.directory.list_members(&ctx, q.to_request()?).await?;
    Ok(Json(PageDto::from_page(page, MemberDto::from)))
}

async fn add_member(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<MemberCreateDto>,
) -> ApiResult<Response> {
    let member = app
        .directory
        .add_member(
            &ctx,
            NewMember {
                user_ref: dto.user_ref,
                email: dto.email,
                display_name: dto.display_name,
                tenant_role: dto.tenant_role,
                person_id: dto.person_id,
            },
        )
        .await?;
    Ok(created(
        format!("/members/{}", member.meta.id),
        Some(member.meta.version),
        MemberDto::from(member),
    ))
}

async fn update_member(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<MemberUpdateDto>,
) -> ApiResult<Json<MemberDto>> {
    let patch = MemberPatch {
        tenant_role: dto.tenant_role,
        person_id: dto.person_id,
    };
    Ok(Json(
        app.directory.update_member(&ctx, id, patch).await?.into(),
    ))
}

async fn remove_member(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.directory.remove_member(&ctx, id).await?;
    Ok(no_content())
}

async fn list_persons(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiQuery(q): ApiQuery<PersonListQuery>,
) -> ApiResult<Json<PageDto<PersonDto>>> {
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let page = app
        .directory
        .list_persons(
            &ctx,
            search,
            PageQuery {
                cursor: q.cursor,
                limit: q.limit,
            }
            .to_request()?,
        )
        .await?;
    Ok(Json(PageDto::from_page(page, PersonDto::from)))
}

async fn create_person(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<PersonFieldsDto>,
) -> ApiResult<Response> {
    let person = app.directory.create_person(&ctx, dto.into()).await?;
    Ok(created(
        format!("/persons/{}", person.meta.id),
        Some(person.meta.version),
        PersonDto::from(person),
    ))
}

async fn get_person(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    let person = app.directory.get_person(&ctx, id).await?;
    Ok(with_etag(person.meta.version, PersonDto::from(person)))
}

async fn update_person(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<PersonFieldsDto>,
) -> ApiResult<Response> {
    let person = app
        .directory
        .update_person(&ctx, id, if_match, dto.into())
        .await?;
    Ok(with_etag(person.meta.version, PersonDto::from(person)))
}

async fn delete_person(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.directory.delete_person(&ctx, id).await?;
    Ok(no_content())
}

async fn list_roles(
    State(app): State<AppState>,
    ctx: TenantContext,
) -> ApiResult<Json<Vec<RoleDto>>> {
    Ok(Json(
        app.directory
            .list_roles(&ctx)
            .await?
            .into_iter()
            .map(RoleDto::from)
            .collect(),
    ))
}

async fn create_role(
    State(app): State<AppState>,
    ctx: TenantContext,
    ValidJson(dto): ValidJson<RoleFieldsDto>,
) -> ApiResult<Response> {
    let role = app.directory.create_role(&ctx, dto.into()).await?;
    Ok(created(
        format!("/roles/{}", role.meta.id),
        Some(role.meta.version),
        RoleDto::from(role),
    ))
}

async fn update_role(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<RoleFieldsDto>,
) -> ApiResult<Json<RoleDto>> {
    Ok(Json(
        app.directory
            .update_role(&ctx, id, dto.into())
            .await?
            .into(),
    ))
}

async fn delete_role(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Response> {
    app.directory.delete_role(&ctx, id).await?;
    Ok(no_content())
}
