//! Cross-cutting tower middleware: request ids, tracing, timeouts, limits, CORS, security headers.

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, Method, Request, StatusCode, header};
use tower::ServiceBuilder;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone)]
pub struct HttpSettings {
    pub request_timeout: Duration,
    pub body_limit_bytes: usize,
    pub max_concurrency: usize,
    /// Allowed CORS origins; empty means same-origin only (no CORS headers).
    pub cors_origins: Vec<HeaderValue>,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            body_limit_bytes: 1024 * 1024,
            max_concurrency: 512,
            cors_origins: Vec::new(),
        }
    }
}

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub fn apply(router: Router, settings: &HttpSettings) -> Router {
    let security_headers = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    let router = router
        .layer(DefaultBodyLimit::max(settings.body_limit_bytes))
        .layer(security_headers)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            settings.request_timeout,
        ))
        .layer(GlobalConcurrencyLimitLayer::new(settings.max_concurrency))
        .layer(CatchPanicLayer::new());

    let router = if settings.cors_origins.is_empty() {
        router
    } else {
        router.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(settings.cors_origins.clone()))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                ])
                .allow_headers([
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                    header::IF_MATCH,
                    HeaderName::from_static("last-event-id"),
                    HeaderName::from_static("x-dev-tenant"),
                ])
                .expose_headers([header::ETAG, header::LOCATION, REQUEST_ID])
                .max_age(Duration::from_secs(600)),
        )
    };

    // Outermost: assign a request id, then trace with it (path only; query strings may carry data).
    router
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
            let request_id = req.headers().get(&REQUEST_ID).and_then(|v| v.to_str().ok()).unwrap_or("-");
            tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), request_id = %request_id)
        }))
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
}
