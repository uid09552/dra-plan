//! HTTP edge helpers shared by all feature `api` modules.

pub mod dto;
mod error;
mod extract;
pub mod layers;

pub use dto::{MetaDto, ProvenanceDto};
pub use error::{ApiError, ApiResult, IssueDto};
pub use extract::{ApiPath, ApiQuery, IfMatch, OptionalJson, PageQuery, ValidJson};

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::shared::kernel::Page;

/// Base path the API router is nested under.
pub const API_BASE: &str = "/api/v1";

/// `201 Created` with `Location` (relative to [`API_BASE`]) and `ETag`.
pub fn created<T: Serialize>(path: String, version: Option<i32>, body: T) -> Response {
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    if let Ok(location) = HeaderValue::from_str(&format!("{API_BASE}{path}")) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    if let Some(v) = version {
        insert_etag(&mut response, v);
    }
    response
}

/// `200 OK` with an `ETag` derived from the resource version.
pub fn with_etag<T: Serialize>(version: i32, body: T) -> Response {
    let mut response = Json(body).into_response();
    insert_etag(&mut response, version);
    response
}

fn insert_etag(response: &mut Response, version: i32) {
    if let Ok(v) = HeaderValue::from_str(&format!("\"{version}\"")) {
        response.headers_mut().insert(header::ETAG, v);
    }
}

pub fn no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

/// `{ items, nextCursor }` list envelope.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> PageDto<T> {
    pub fn from_page<D>(page: Page<D>, f: impl FnMut(D) -> T) -> Self {
        let page = page.map(f);
        PageDto {
            items: page.items,
            next_cursor: page.next_cursor,
        }
    }
}
