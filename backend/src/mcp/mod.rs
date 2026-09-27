//! MCP (Model Context Protocol) inbound adapter at `/mcp`.
//!
//! Transport: Streamable HTTP, stateless — each `POST /mcp` carries one JSON-RPC 2.0 message and
//! gets a JSON response (no server-initiated streams, so `GET /mcp` returns 405). Tools are
//! **use-case based** (see [`tools`]) and call the same application use cases as the REST API,
//! so validation, tenant isolation and audit behave identically.

pub mod tools;

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::shared::kernel::{AppError, TenantContext};

pub const PATH: &str = "/mcp";
const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const LATEST_VERSION: &str = "2025-06-18";

/// Allowed `Origin` values (DNS-rebinding protection, required by the MCP transport spec).
#[derive(Debug, Clone, Default)]
pub struct McpConfig {
    pub allowed_origins: Vec<HeaderValue>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route(PATH, post(handle).get(no_stream).delete(no_stream))
}

async fn no_stream() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn rpc_error(id: Value, code: i64, message: &str) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
        .into_response()
}

async fn handle(
    State(app): State<AppState>,
    Extension(config): Extension<McpConfig>,
    ctx: TenantContext,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !config.allowed_origins.contains(origin) {
            return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
        }
    }
    let request: RpcRequest = match serde_json::from_slice::<Value>(&body) {
        Ok(Value::Array(_)) => {
            return rpc_error(Value::Null, -32600, "batch requests are not supported");
        }
        Ok(v) => match serde_json::from_value(v) {
            Ok(r) => r,
            Err(e) => return rpc_error(Value::Null, -32600, &format!("invalid request: {e}")),
        },
        Err(e) => return rpc_error(Value::Null, -32700, &format!("parse error: {e}")),
    };
    if request.jsonrpc != "2.0" {
        return rpc_error(
            request.id.unwrap_or(Value::Null),
            -32600,
            "jsonrpc must be \"2.0\"",
        );
    }
    // Notifications and responses from the client need no answer.
    let Some(id) = request.id else {
        return StatusCode::ACCEPTED.into_response();
    };

    match request.method.as_str() {
        "initialize" => {
            let requested = request.params["protocolVersion"]
                .as_str()
                .unwrap_or(LATEST_VERSION);
            let version = if SUPPORTED_VERSIONS.contains(&requested) {
                requested
            } else {
                LATEST_VERSION
            };
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "dra-workflow", "title": "dra-workflow DR planning", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": tools::INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let Some(name) = request.params["name"].as_str() else {
                return rpc_error(id, -32602, "params.name is required");
            };
            if !tools::exists(name) {
                return rpc_error(id, -32602, &format!("unknown tool `{name}`"));
            }
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            rpc_result(id, tool_result(tools::call(&app, &ctx, name, args).await))
        }
        other => rpc_error(id, -32601, &format!("method not found: {other}")),
    }
}

/// Tool errors are reported inside the result (`isError`) so the model can react to them.
fn tool_result(result: Result<tools::ToolOutput, AppError>) -> Value {
    match result {
        Ok(tools::ToolOutput::Json(value)) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
            "structuredContent": value,
            "isError": false,
        }),
        Ok(tools::ToolOutput::Text(text)) => json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false,
        }),
        Err(e) => {
            let mut text = e.to_string();
            if let AppError::Validation(issues) = &e {
                for i in issues {
                    text.push_str(&format!(
                        "\n- [{}] {}{}",
                        i.rule_id,
                        i.message,
                        i.field
                            .as_deref()
                            .map(|f| format!(" ({f})"))
                            .unwrap_or_default()
                    ));
                }
            }
            if let AppError::Internal(detail) = &e {
                tracing::error!(%detail, "internal error in MCP tool");
                text = "internal error".into();
            }
            json!({ "content": [{ "type": "text", "text": text }], "isError": true })
        }
    }
}
