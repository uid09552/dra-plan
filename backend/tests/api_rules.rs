#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Cross-cutting API behavior: tenant isolation, validation, concurrency, error format.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::app;

#[tokio::test]
async fn tenants_are_isolated() {
    let a = app().await;
    let b = app().await;
    let service = a
        .post("/services", json!({ "name": "Secret service" }))
        .await
        .expect(StatusCode::CREATED)
        .id();

    // Another tenant cannot see, change or reference it (404, not 403: existence is not revealed).
    b.get(&format!("/services/{service}"))
        .await
        .expect(StatusCode::NOT_FOUND);
    b.patch(&format!("/services/{service}"), json!({ "name": "x" }))
        .await
        .expect(StatusCode::NOT_FOUND);
    b.post(
        &format!("/services/{service}/microservices"),
        json!({ "name": "m" }),
    )
    .await
    .expect(StatusCode::NOT_FOUND);
    let b_list = b.get("/services").await.expect(StatusCode::OK);
    assert!(b_list.body["items"].as_array().unwrap().is_empty());

    // Composite foreign keys reject cross-tenant references.
    let a_person = a
        .post("/persons", json!({ "name": "Alice" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let b_service = b
        .post("/services", json!({ "name": "B" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let cross = b
        .patch(
            &format!("/services/{b_service}"),
            json!({ "businessOwnerId": a_person }),
        )
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(cross.rule_ids(), vec!["REFERENCE_NOT_FOUND"]);

    // Each tenant only sees its own audit trail.
    let audit = b
        .get("/audit-log?entityType=it_service")
        .await
        .expect(StatusCode::OK);
    assert!(
        audit.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["entityId"] != service.as_str())
    );
}

#[tokio::test]
async fn row_level_security_blocks_unscoped_access() {
    let a = app().await;
    a.post("/services", json!({ "name": "RLS" }))
        .await
        .expect(StatusCode::CREATED);
    // A transaction without app.tenant_id sees no tenant-scoped rows at all (application role is
    // not a superuser, so the forced policy applies).
    let mut tx = a.db.begin_system("probe").await.unwrap();
    let visible: i64 = sqlx::query_scalar("select count(*) from it_service")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(visible, 0);
}

#[tokio::test]
async fn validation_errors_are_problem_details() {
    let a = app().await;
    let r = a
        .post("/services", json!({ "name": "" }))
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.headers["content-type"], "application/problem+json");
    assert_eq!(r.body["issues"][0]["field"], "/name");

    let r = a
        .post("/services", json!({ "name": "x", "unknown": 1 }))
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.body["issues"][0]["field"], "/unknown");

    let r = a
        .post(
            "/services",
            json!({ "name": "x", "impactLevel": "extreme" }),
        )
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        r.body["issues"][0]["message"]
            .as_str()
            .unwrap()
            .contains("expected one of")
    );

    let r = a
        .send(
            Method::POST,
            "/services",
            None,
            &[("content-type", "text/plain")],
        )
        .await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    a.get("/services/not-a-uuid")
        .await
        .expect(StatusCode::BAD_REQUEST);
    a.get("/services?limit=500")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn optimistic_concurrency_with_if_match() {
    let a = app().await;
    let created = a
        .post("/services", json!({ "name": "Etag" }))
        .await
        .expect(StatusCode::CREATED);
    let id = created.id();
    let ok = a
        .send(
            Method::PATCH,
            &format!("/services/{id}"),
            Some(json!({ "description": "v2" })),
            &[("if-match", "\"1\"")],
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(ok.headers["etag"], "\"2\"");
    a.send(
        Method::PATCH,
        &format!("/services/{id}"),
        Some(json!({ "description": "stale" })),
        &[("if-match", "\"1\"")],
    )
    .await
    .expect(StatusCode::PRECONDITION_FAILED);
}

#[tokio::test]
async fn referenced_records_cannot_be_deleted() {
    let a = app().await;
    let roles = a.get("/roles").await.expect(StatusCode::OK);
    let default_role = roles.body[0]["id"].as_str().unwrap().to_owned();
    a.delete(&format!("/roles/{default_role}"))
        .await
        .expect(StatusCode::CONFLICT);

    let custom = a
        .post("/roles", json!({ "name": "Network team" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let person = a
        .post("/persons", json!({ "name": "Carol" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let service = a
        .post("/services", json!({ "name": "S" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    a.post(
        &format!("/services/{service}/role-assignments"),
        json!({ "roleId": custom, "personId": person }),
    )
    .await
    .expect(StatusCode::CREATED);
    a.delete(&format!("/roles/{custom}"))
        .await
        .expect(StatusCode::CONFLICT);
    a.delete(&format!("/persons/{person}"))
        .await
        .expect(StatusCode::CONFLICT);
}

#[tokio::test]
async fn tenant_settings_and_catalog() {
    let a = app().await;
    let t = a.get("/tenant").await.expect(StatusCode::OK);
    assert_eq!(t.body["settings"]["reviewIntervalMonths"], 12);
    a.patch(
        "/tenant",
        json!({ "settings": { "reviewIntervalMonths": 6, "defaultLanguage": "de" } }),
    )
    .await
    .expect(StatusCode::OK);
    let catalog = a
        .send(
            Method::GET,
            "/catalog",
            None,
            &[("accept-language", "de-DE")],
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(catalog.body["workflowSteps"].as_array().unwrap().len(), 15);
    assert_eq!(
        catalog.body["workflowSteps"][0]["title"],
        "Dienst beschreiben"
    );
    let tenants = a.get("/tenants").await.expect(StatusCode::OK);
    assert_eq!(tenants.body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn ai_is_unavailable_without_provider() {
    let a = app().await;
    let service = a
        .post("/services", json!({ "name": "AI" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    a.post(
        "/ai/suggestions",
        json!({ "kind": "suggest_scenarios", "serviceId": service }),
    )
    .await
    .expect(StatusCode::SERVICE_UNAVAILABLE);
    a.get("/health").await.expect(StatusCode::OK);
}
