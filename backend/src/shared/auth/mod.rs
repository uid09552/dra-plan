//! Authentication middleware and the `TenantContext` / `Principal` extractors.
//!
//! **Authentication and authorization are mocked for now.** The [`Authenticator`] port has two
//! adapters: [`DevAuthenticator`] (`--dev-mode`: fixed dev user, default tenant `demo`) and
//! [`DisabledAuthenticator`] (fails closed with 401). A JWT/OIDC adapter replaces them later
//! without touching handlers, because handlers only see the extractors below.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::shared::kernel::{AppError, AppResult, Principal, TenantContext, TenantId, TenantRole};
use crate::shared::web::ApiError;

/// Result of authenticating a request.
#[derive(Debug, Clone)]
pub struct Authenticated {
    pub principal: Principal,
    /// Active tenant (JWT `tenant_id` claim). `None` for tokens without a tenant.
    pub tenant_id: Option<TenantId>,
}

/// Port: authenticates a request from its headers.
#[async_trait]
pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, headers: &HeaderMap) -> AppResult<Authenticated>;
}

/// Port used by the dev authenticator to resolve `X-Dev-Tenant` (slug or id).
#[async_trait]
pub trait TenantLookup: Send + Sync {
    async fn find_tenant_id(&self, slug_or_id: &str) -> AppResult<Option<TenantId>>;
}

/// Development mock: every request is the admin user `dev-user` in the default tenant.
/// The header `X-Dev-Tenant: <slug|id>` switches the tenant (dev only).
pub struct DevAuthenticator {
    pub default_tenant: TenantId,
    pub lookup: Arc<dyn TenantLookup>,
}

pub const DEV_USER: &str = "dev-user";
pub const DEV_TENANT_HEADER: &str = "x-dev-tenant";

#[async_trait]
impl Authenticator for DevAuthenticator {
    async fn authenticate(&self, headers: &HeaderMap) -> AppResult<Authenticated> {
        let tenant_id = match headers.get(DEV_TENANT_HEADER).and_then(|v| v.to_str().ok()) {
            Some(requested) => self
                .lookup
                .find_tenant_id(requested.trim())
                .await?
                .ok_or_else(|| AppError::Forbidden(format!("unknown tenant `{requested}`")))?,
            None => self.default_tenant,
        };
        Ok(Authenticated {
            principal: Principal {
                user: DEV_USER.to_owned(),
                role: TenantRole::Admin,
            },
            tenant_id: Some(tenant_id),
        })
    }
}

/// Production placeholder until JWT validation exists: rejects every request (fail closed).
pub struct DisabledAuthenticator;

#[async_trait]
impl Authenticator for DisabledAuthenticator {
    async fn authenticate(&self, _headers: &HeaderMap) -> AppResult<Authenticated> {
        Err(AppError::Unauthorized(
            "authentication is not implemented yet; start the server with --dev-mode for local use"
                .into(),
        ))
    }
}

/// Fixed identity, used by tests.
pub struct StaticAuthenticator(pub Authenticated);

#[async_trait]
impl Authenticator for StaticAuthenticator {
    async fn authenticate(&self, _headers: &HeaderMap) -> AppResult<Authenticated> {
        Ok(self.0.clone())
    }
}

/// Middleware: authenticates and stores [`Authenticated`] in the request extensions.
pub async fn authenticate(
    State(auth): State<Arc<dyn Authenticator>>,
    mut req: Request,
    next: Next,
) -> Response {
    match auth.authenticate(req.headers()).await {
        Ok(authenticated) => {
            req.extensions_mut().insert(authenticated);
            next.run(req).await
        }
        Err(e) => ApiError::from(e).into_response(),
    }
}

fn authenticated(parts: &Parts) -> Result<&Authenticated, ApiError> {
    parts
        .extensions
        .get::<Authenticated>()
        .ok_or_else(|| ApiError::from(AppError::Unauthorized("missing authentication".into())))
}

impl<S: Send + Sync> FromRequestParts<S> for TenantContext {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, ApiError> {
        let auth = authenticated(parts)?;
        let tenant_id = auth.tenant_id.ok_or_else(|| {
            ApiError::from(AppError::Forbidden(
                "the access token has no valid tenant_id claim".into(),
            ))
        })?;
        Ok(TenantContext {
            tenant_id,
            principal: auth.principal.clone(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, ApiError> {
        Ok(authenticated(parts)?.principal.clone())
    }
}
