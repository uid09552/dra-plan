use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use validator::Validate;

use super::domain::{Tenant, TenantPatch, TenantSettings, TenantSettingsPatch};
use crate::app::AppState;
use crate::shared::kernel::{
    AppResult, ImpactCategory, Language, Minutes, Principal, Rating, TenantContext,
};
use crate::shared::web::{
    ApiQuery, ApiResult, IfMatch, MetaDto, PageDto, PageQuery, ValidJson, created, no_content,
    with_etag,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tenants", get(list).post(create))
        .route(
            "/tenant",
            get(get_current)
                .patch(update_current)
                .delete(delete_current),
        )
}

#[serde_as]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantSettingsDto {
    pub review_interval_months: u32,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub impact_categories: Vec<ImpactCategory>,
    pub impact_time_windows_minutes: Vec<u32>,
    pub impact_tolerance_level: u8,
    #[serde_as(as = "DisplayFromStr")]
    pub default_language: Language,
    pub ai_enabled: bool,
}

impl From<TenantSettings> for TenantSettingsDto {
    fn from(s: TenantSettings) -> Self {
        TenantSettingsDto {
            review_interval_months: s.review_interval_months,
            impact_categories: s.impact_categories,
            impact_time_windows_minutes: s
                .impact_time_windows
                .into_iter()
                .map(Minutes::get)
                .collect(),
            impact_tolerance_level: s.impact_tolerance_level.get(),
            default_language: s.default_language,
            ai_enabled: s.ai_enabled,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantDto {
    #[serde(flatten)]
    pub meta: MetaDto,
    pub name: String,
    pub slug: String,
    pub settings: TenantSettingsDto,
}

impl From<Tenant> for TenantDto {
    fn from(t: Tenant) -> Self {
        TenantDto {
            meta: t.meta.into(),
            name: t.name,
            slug: t.slug,
            settings: t.settings.into(),
        }
    }
}

#[serde_as]
#[derive(Debug, Default, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TenantSettingsInputDto {
    #[validate(range(min = 1, max = 120))]
    pub review_interval_months: Option<u32>,
    #[serde_as(as = "Option<Vec<DisplayFromStr>>")]
    #[serde(default)]
    pub impact_categories: Option<Vec<ImpactCategory>>,
    #[validate(length(min = 1, max = 20))]
    pub impact_time_windows_minutes: Option<Vec<u32>>,
    #[validate(range(min = 1, max = 4))]
    pub impact_tolerance_level: Option<u8>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub default_language: Option<Language>,
    pub ai_enabled: Option<bool>,
}

impl TenantSettingsInputDto {
    fn into_domain(self) -> AppResult<TenantSettingsPatch> {
        Ok(TenantSettingsPatch {
            review_interval_months: self.review_interval_months,
            impact_categories: self.impact_categories,
            impact_time_windows: self
                .impact_time_windows_minutes
                .map(|v| {
                    v.into_iter()
                        .map(Minutes::new)
                        .collect::<AppResult<Vec<_>>>()
                })
                .transpose()?,
            impact_tolerance_level: self
                .impact_tolerance_level
                .map(|l| Rating::new(i64::from(l)))
                .transpose()?,
            default_language: self.default_language,
            ai_enabled: self.ai_enabled,
        })
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TenantCreateDto {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub slug: String,
    #[validate(nested)]
    #[serde(default)]
    pub settings: TenantSettingsInputDto,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TenantUpdateDto {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub slug: Option<String>,
    #[validate(nested)]
    #[serde(default)]
    pub settings: TenantSettingsInputDto,
}

async fn list(
    State(app): State<AppState>,
    principal: Principal,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> ApiResult<Json<PageDto<TenantDto>>> {
    let page = app.tenants.list_for(&principal, q.to_request()?).await?;
    Ok(Json(PageDto::from_page(page, TenantDto::from)))
}

async fn create(
    State(app): State<AppState>,
    principal: Principal,
    ValidJson(dto): ValidJson<TenantCreateDto>,
) -> ApiResult<Response> {
    let tenant = app
        .tenants
        .create(&principal, dto.name, dto.slug, dto.settings.into_domain()?)
        .await?;
    Ok(created(
        "/tenant".into(),
        Some(tenant.meta.version),
        TenantDto::from(tenant),
    ))
}

async fn get_current(State(app): State<AppState>, ctx: TenantContext) -> ApiResult<Response> {
    let tenant = app.tenants.get_current(&ctx).await?;
    Ok(with_etag(tenant.meta.version, TenantDto::from(tenant)))
}

async fn update_current(
    State(app): State<AppState>,
    ctx: TenantContext,
    IfMatch(if_match): IfMatch,
    ValidJson(dto): ValidJson<TenantUpdateDto>,
) -> ApiResult<Response> {
    let patch = TenantPatch {
        name: dto.name,
        slug: dto.slug,
        settings: dto.settings.into_domain()?,
    };
    let tenant = app.tenants.update_current(&ctx, if_match, patch).await?;
    Ok(with_etag(tenant.meta.version, TenantDto::from(tenant)))
}

async fn delete_current(
    State(app): State<AppState>,
    ctx: TenantContext,
    IfMatch(if_match): IfMatch,
) -> ApiResult<Response> {
    app.tenants.delete_current(&ctx, if_match).await?;
    Ok(no_content())
}
