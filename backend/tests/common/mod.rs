//! Test harness: runs the real router (all layers) against PostgreSQL.
//! Requires `DRA_TEST_DATABASE_URL` (see `make test`).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use dra_server::bootstrap::{self, AuthMode};
use dra_server::features::tenants::domain::TenantSettingsPatch;
use dra_server::shared::auth::Authenticated;
use dra_server::shared::infra::Db;
use dra_server::shared::kernel::{Principal, TenantId, TenantRole};
use dra_server::shared::web::layers::HttpSettings;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

pub struct TestApp {
    pub router: Router,
    pub tenant_id: TenantId,
    pub db: Db,
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
    pub text: String,
}

impl Reply {
    pub fn id(&self) -> String {
        self.body["id"]
            .as_str()
            .unwrap_or_else(|| panic!("no id in {}", self.text))
            .to_owned()
    }

    #[track_caller]
    pub fn expect(self, status: StatusCode) -> Self {
        assert_eq!(
            self.status, status,
            "unexpected status, body: {}",
            self.text
        );
        self
    }

    pub fn rule_ids(&self) -> Vec<String> {
        self.body["issues"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|i| i["ruleId"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub async fn db() -> Db {
    let url = std::env::var("DRA_TEST_DATABASE_URL").expect(
        "set DRA_TEST_DATABASE_URL (e.g. postgres://dra:dra@localhost:5434/dra) or run `make test`",
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to test database");
    let db = Db::from_pool(pool);
    db.migrate().await.expect("migrate");
    db
}

/// A fresh tenant with a router authenticated as its admin.
pub async fn app() -> TestApp {
    let db = db().await;
    let user = format!("test-{}", Uuid::new_v4());
    let principal = Principal {
        user: user.clone(),
        role: TenantRole::Admin,
    };
    let slug = format!("t-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let tenant = bootstrap::build_state(db.clone())
        .tenants
        .create(
            &principal,
            "Test tenant".into(),
            slug,
            TenantSettingsPatch::default(),
        )
        .await
        .expect("create tenant");
    let identity = Authenticated {
        principal,
        tenant_id: Some(tenant.id()),
    };
    let router = bootstrap::build_router(
        db.clone(),
        AuthMode::Static(identity),
        &HttpSettings::default(),
    )
    .await
    .expect("router");
    TestApp {
        router,
        tenant_id: tenant.id(),
        db,
    }
}

impl TestApp {
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Reply {
        let mut builder = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        let request = match body {
            Some(b)
                if headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type")) =>
            {
                builder.body(Body::from(b.to_string()))
            }
            Some(b) => builder
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => builder.body(Body::empty()),
        }
        .expect("request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("response");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply {
            status,
            headers,
            body,
            text,
        }
    }

    pub async fn get(&self, path: &str) -> Reply {
        self.send(Method::GET, path, None, &[]).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Reply {
        self.send(Method::POST, path, Some(body), &[]).await
    }

    pub async fn put(&self, path: &str, body: Value) -> Reply {
        self.send(Method::PUT, path, Some(body), &[]).await
    }

    pub async fn patch(&self, path: &str, body: Value) -> Reply {
        self.send(
            Method::PATCH,
            path,
            Some(body),
            &[("content-type", "application/merge-patch+json")],
        )
        .await
    }

    pub async fn delete(&self, path: &str) -> Reply {
        self.send(Method::DELETE, path, None, &[]).await
    }
}
