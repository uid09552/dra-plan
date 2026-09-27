#![allow(clippy::unwrap_used, clippy::expect_used)]
//! MCP endpoint: protocol handshake and use-case tools.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{TestApp, app};

async fn rpc(app: &TestApp, body: Value, origin: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(o) = origin {
        builder = builder.header("origin", o);
    }
    let response = app
        .router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn tool(app: &TestApp, name: &str, arguments: Value) -> Value {
    let (status, body) = rpc(app, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }), None).await;
    assert_eq!(status, StatusCode::OK);
    body["result"].clone()
}

#[tokio::test]
async fn protocol_handshake() {
    let app = app().await;
    let (_, init) = rpc(&app, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }), None).await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "dra-workflow");

    let (status, _) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (_, list) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        None,
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"add_mitigation"));
    assert!(names.contains(&"declare_recovery"));

    let (_, unknown) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }),
        None,
    )
    .await;
    assert_eq!(unknown["error"]["code"], -32601);

    let (status, _) = rpc(
        &app,
        json!({ "jsonrpc": "2.0", "id": 4, "method": "ping" }),
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "unknown origins are rejected"
    );
}

#[tokio::test]
async fn mitigation_use_case_via_tools() {
    let app = app().await;
    let created = tool(
        &app,
        "create_service",
        json!({
            "name": "Billing", "microservices": [{ "name": "billing-db", "restoreOrder": 1 }]
        }),
    )
    .await;
    assert_eq!(created["isError"], false);
    let service = created["structuredContent"]["serviceId"]
        .as_str()
        .unwrap()
        .to_owned();
    let db = created["structuredContent"]["microservices"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let bia = tool(&app, "record_business_impact", json!({ "serviceId": service, "mtpdMinutes": 60, "serviceRtoMinutes": 120, "serviceRpoMinutes": 5 })).await;
    assert_eq!(
        bia["isError"], true,
        "RTO above MTPD is reported as a tool error"
    );
    assert!(
        bia["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("SERVICE_RTO_EXCEEDS_MTPD")
    );
    tool(&app, "record_business_impact", json!({ "serviceId": service, "mtpdMinutes": 480, "serviceRtoMinutes": 120, "serviceRpoMinutes": 5 })).await;

    let scenarios = tool(
        &app,
        "add_scenarios",
        json!({ "serviceId": service, "scenarios": [{ "catalogTemplateId": "ransomware" }] }),
    )
    .await;
    let scenario = scenarios["structuredContent"]["created"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tool(&app, "decide_scenario", json!({ "scenarioId": scenario, "decision": "selected", "drRequired": "yes", "rationale": "must recover", "affectedMicroserviceIds": [db] })).await;
    tool(
        &app,
        "set_recovery_objective",
        json!({ "microserviceId": db, "rtoMinutes": 90, "rpoMinutes": 5 }),
    )
    .await;

    let mitigation = tool(&app, "add_mitigation", json!({
        "microserviceId": db, "scenarioId": scenario,
        "strategy": { "type": "iac_rebuild", "title": "Clean rebuild + restore immutable backup", "estimatedRtoMinutes": 60, "estimatedRpoMinutes": 5 },
        "runbook": { "title": "Ransomware recovery", "steps": [
            { "phase": "activation", "title": "Isolate network", "ownerRole": "incident commander", "expectedDurationMinutes": 10, "verification": "segments isolated" },
            { "phase": "recovery", "title": "Rebuild from IaC", "ownerRole": "Platform", "expectedDurationMinutes": 30, "verification": "cluster healthy" },
            { "phase": "recovery", "title": "Restore immutable backup", "ownerRole": "DBA", "expectedDurationMinutes": 15, "verification": "checksums ok" }
        ]}
    })).await;
    assert_eq!(mitigation["isError"], false, "{mitigation}");
    let m = &mitigation["structuredContent"];
    assert_eq!(m["gapCheck"]["status"], "meets");
    assert_eq!(m["criticalPathMinutes"], 55);
    assert_eq!(m["steps"].as_array().unwrap().len(), 3);

    let bad_role = tool(&app, "add_mitigation", json!({
        "microserviceId": db, "scenarioId": scenario,
        "strategy": { "type": "backup_restore", "estimatedRtoMinutes": 60, "estimatedRpoMinutes": 5 },
        "runbook": { "title": "x", "steps": [{ "phase": "recovery", "title": "y", "ownerRole": "Wizard" }] }
    })).await;
    assert_eq!(bad_role["isError"], true);
    assert!(
        bad_role["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("UNKNOWN_ROLE")
    );

    let overview = tool(
        &app,
        "get_service_overview",
        json!({ "serviceId": service }),
    )
    .await;
    let db_overview = overview["structuredContent"]["microservices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == db)
        .expect("db component in overview");
    assert_eq!(db_overview["runbooks"], 1);
    assert_eq!(
        overview["structuredContent"]["workflow"]["currentStep"],
        "define_service"
    );
}
