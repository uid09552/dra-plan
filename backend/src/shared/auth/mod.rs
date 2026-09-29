//! Authentication middleware and the `TenantContext` / `Principal` extractors.
//!
//! The [`Authenticator`] port has a development mock and a production Keycloak JWT adapter.
//! Handlers see only the principal and tenant extractors below.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::shared::kernel::{AppError, AppResult, Principal, TenantContext, TenantId, TenantRole};
use crate::shared::web::ApiError;

/// Result of authenticating a request.
#[derive(Debug, Clone)]
pub struct Authenticated {
    pub principal: Principal,
    pub access_role: AccessRole,
    /// Active tenant (JWT `tenant_id` claim). `None` for tokens without a tenant.
    pub tenant_id: Option<TenantId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRole {
    Viewer,
    Admin,
}

impl AccessRole {
    fn allows(self, method: &Method) -> bool {
        match self {
            Self::Admin => true,
            Self::Viewer => matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS),
        }
    }
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

/// Validates Keycloak access tokens and derives the API access role from realm roles.
pub struct JwtAuthenticator {
    issuer: String,
    audience: String,
    jwks_url: String,
    http: reqwest::Client,
    jwks: RwLock<JwkSet>,
    tenant_lookup: Arc<dyn TenantLookup>,
}

#[derive(Debug, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    tenant: HashMap<String, OrganizationClaim>,
    realm_access: RealmAccessClaim,
}

#[derive(Debug, Deserialize)]
struct OrganizationClaim {
    #[serde(rename = "tenant_id")]
    tenant_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RealmAccessClaim {
    roles: Vec<String>,
}

impl JwtAuthenticator {
    pub async fn new(
        issuer: String,
        jwks_url: String,
        audience: String,
        tenant_lookup: Arc<dyn TenantLookup>,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let jwks = http
            .get(&jwks_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(Self {
            issuer,
            audience,
            jwks_url,
            http,
            jwks: RwLock::new(jwks),
            tenant_lookup,
        })
    }

    async fn decoding_key(&self, kid: &str) -> AppResult<DecodingKey> {
        {
            let jwks = self.jwks.read().await;
            if let Some(jwk) = jwks.find(kid) {
                return DecodingKey::from_jwk(jwk)
                    .map_err(|_| AppError::Unauthorized("invalid token signing key".into()));
            }
        }

        let refreshed = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| AppError::Unauthorized("unable to refresh token signing keys".into()))?
            .json::<JwkSet>()
            .await
            .map_err(|_| AppError::Unauthorized("unable to refresh token signing keys".into()))?;
        let mut jwks = self.jwks.write().await;
        *jwks = refreshed;
        jwks.find(kid)
            .ok_or_else(|| AppError::Unauthorized("unknown token signing key".into()))
            .and_then(|jwk| {
                DecodingKey::from_jwk(jwk)
                    .map_err(|_| AppError::Unauthorized("invalid token signing key".into()))
            })
    }
}

#[async_trait]
impl Authenticator for JwtAuthenticator {
    async fn authenticate(&self, headers: &HeaderMap) -> AppResult<Authenticated> {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::Unauthorized("missing bearer token".into()))?;
        let header = decode_header(token)
            .map_err(|_| AppError::Unauthorized("invalid bearer token".into()))?;
        if header.alg != Algorithm::RS256 {
            return Err(AppError::Unauthorized("unsupported token algorithm".into()));
        }
        let kid = header
            .kid
            .ok_or_else(|| AppError::Unauthorized("token has no signing key id".into()))?;
        let key = self.decoding_key(&kid).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<AccessTokenClaims>(token, &key, &validation)
            .map_err(|_| AppError::Unauthorized("invalid bearer token".into()))?
            .claims;

        let (access_role, tenant_role) =
            if claims.realm_access.roles.iter().any(|r| r == "dra-admin") {
                (AccessRole::Admin, TenantRole::Admin)
            } else if claims.realm_access.roles.iter().any(|r| r == "dra-viewer") {
                (AccessRole::Viewer, TenantRole::Auditor)
            } else {
                return Err(AppError::Forbidden(
                    "token has no supported API role".into(),
                ));
            };

        let tenant_slugs: HashSet<&str> = claims
            .tenant
            .values()
            .flat_map(|organization| organization.tenant_ids.iter().map(String::as_str))
            .collect();
        if tenant_slugs.len() != 1 {
            return Err(AppError::Forbidden(
                "token must identify exactly one active tenant".into(),
            ));
        }
        let tenant_slug = *tenant_slugs
            .iter()
            .next()
            .ok_or_else(|| AppError::Forbidden("token has no tenant".into()))?;
        let tenant_id = self
            .tenant_lookup
            .find_tenant_id(tenant_slug)
            .await?
            .ok_or_else(|| AppError::Forbidden("token tenant is unknown".into()))?;

        Ok(Authenticated {
            principal: Principal {
                user: claims.sub,
                role: tenant_role,
            },
            access_role,
            tenant_id: Some(tenant_id),
        })
    }
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
            access_role: AccessRole::Admin,
            tenant_id: Some(tenant_id),
        })
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
            if !authenticated.access_role.allows(req.method()) {
                return ApiError::from(AppError::Forbidden(
                    "the dra-viewer role is read-only".into(),
                ))
                .into_response();
            }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::any;
    use tower::ServiceExt;

    async fn status_for(role: AccessRole, method: Method) -> StatusCode {
        let auth: Arc<dyn Authenticator> = Arc::new(StaticAuthenticator(Authenticated {
            principal: Principal {
                user: "test-user".into(),
                role: if role == AccessRole::Admin {
                    TenantRole::Admin
                } else {
                    TenantRole::Auditor
                },
            },
            access_role: role,
            tenant_id: None,
        }));
        let router = Router::new()
            .route("/", any(|| async { StatusCode::OK }))
            .layer(middleware::from_fn_with_state(auth, authenticate));
        router
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[test]
    fn viewer_is_read_only_and_admin_can_write() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(AccessRole::Viewer.allows(&method));
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!AccessRole::Viewer.allows(&method));
            assert!(AccessRole::Admin.allows(&method));
        }
    }

    #[tokio::test]
    async fn backend_middleware_enforces_viewer_role() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert_eq!(status_for(AccessRole::Viewer, method).await, StatusCode::OK);
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert_eq!(
                status_for(AccessRole::Viewer, method.clone()).await,
                StatusCode::FORBIDDEN
            );
            assert_eq!(status_for(AccessRole::Admin, method).await, StatusCode::OK);
        }
    }
}
