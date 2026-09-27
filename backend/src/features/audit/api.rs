use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use uuid::Uuid;

use super::domain::{AuditEntry, AuditFilter};
use crate::app::AppState;
use crate::shared::kernel::TenantContext;
use crate::shared::web::{ApiQuery, ApiResult, PageDto, PageQuery};

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit-log", get(list))
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryDto {
    pub id: String,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub diff: Option<Box<RawValue>>,
}

impl From<AuditEntry> for AuditEntryDto {
    fn from(e: AuditEntry) -> Self {
        AuditEntryDto {
            id: e.id.to_string(),
            at: e.at,
            actor: e.actor,
            action: e.action,
            entity_type: e.entity_type,
            entity_id: e.entity_id,
            diff: e.diff.and_then(|d| RawValue::from_string(d).ok()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> ApiResult<Json<PageDto<AuditEntryDto>>> {
    let page = PageQuery {
        cursor: q.cursor,
        limit: q.limit,
    }
    .to_request()?;
    let filter = AuditFilter {
        entity_type: q.entity_type,
        entity_id: q.entity_id,
        from: q.from,
        to: q.to,
    };
    Ok(Json(PageDto::from_page(
        app.audit.list(&ctx, &filter, page).await?,
        AuditEntryDto::from,
    )))
}
