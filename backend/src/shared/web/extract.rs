use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

use super::error::ApiError;
use crate::shared::kernel::{AppError, Issue, PageRequest};

/// Path extractor that renders rejections as problem+json.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

/// Query extractor that renders rejections as problem+json.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

/// JSON body that is deserialized strictly (DTOs use `deny_unknown_fields`) and validated.
/// Accepts `application/json` and `+json` media types (e.g. `application/merge-patch+json`).
pub struct ValidJson<T>(pub T);

/// Like [`ValidJson`] for optional request bodies: an empty body yields `T::default()`.
pub struct OptionalJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        require_json(&req)?;
        let bytes = read_body(req, state).await?;
        parse_and_validate(&bytes).map(ValidJson)
    }
}

impl<S, T> FromRequest<S> for OptionalJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate + Default,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let has_content_type = req.headers().contains_key(header::CONTENT_TYPE);
        if has_content_type {
            require_json(&req)?;
        }
        let bytes = read_body(req, state).await?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(OptionalJson(T::default()));
        }
        parse_and_validate(&bytes).map(OptionalJson)
    }
}

fn require_json(req: &Request) -> Result<(), ApiError> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essence == "application/json"
        || (essence.starts_with("application/") && essence.ends_with("+json"))
    {
        Ok(())
    } else {
        Err(ApiError::Http {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            detail: "expected a JSON request body (application/json)".into(),
        })
    }
}

async fn read_body<S: Send + Sync>(req: Request, state: &S) -> Result<Bytes, ApiError> {
    Bytes::from_request(req, state)
        .await
        .map_err(|r| ApiError::Http {
            status: r.status(),
            detail: r.body_text(),
        })
}

fn parse_and_validate<T: DeserializeOwned + Validate>(bytes: &[u8]) -> Result<T, ApiError> {
    let de = &mut serde_json::Deserializer::from_slice(bytes);
    let value: T = serde_path_to_error::deserialize(de).map_err(|e| {
        let pointer = json_pointer(&e.path().to_string());
        let inner = e.into_inner();
        if inner.is_data() {
            let mut issue = Issue::blocking("INVALID_INPUT", inner.to_string());
            if !pointer.is_empty() {
                issue = issue.field(&pointer);
            }
            ApiError::App(AppError::Validation(vec![issue]))
        } else {
            ApiError::bad_request(format!("malformed JSON: {inner}"))
        }
    })?;
    value
        .validate()
        .map_err(|e| ApiError::App(AppError::Validation(validation_issues(&e, ""))))?;
    Ok(value)
}

/// Converts a serde path (`a.b[0].c`) into a JSON pointer (`/a/b/0/c`).
fn json_pointer(path: &str) -> String {
    if path == "." {
        return String::new();
    }
    path.replace('[', ".")
        .replace(']', "")
        .split('.')
        .filter(|s| !s.is_empty())
        .fold(String::new(), |mut acc, s| {
            acc.push('/');
            acc.push_str(s);
            acc
        })
}

fn camel_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper = false;
    for c in snake.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn validation_issues(errors: &ValidationErrors, prefix: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (field, kind) in errors.errors() {
        let pointer = format!("{prefix}/{}", camel_case(field));
        match kind {
            ValidationErrorsKind::Field(errs) => {
                for err in errs {
                    let message =
                        err.message
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| {
                                let params: Vec<String> = err
                                    .params
                                    .iter()
                                    .filter(|(k, _)| *k != "value")
                                    .map(|(k, v)| format!("{k}={v}"))
                                    .collect();
                                format!(
                                    "invalid value ({}{})",
                                    err.code,
                                    if params.is_empty() {
                                        String::new()
                                    } else {
                                        format!(": {}", params.join(", "))
                                    }
                                )
                            });
                    issues.push(Issue::blocking("INVALID_INPUT", message).field(&pointer));
                }
            }
            ValidationErrorsKind::Struct(inner) => {
                issues.extend(validation_issues(inner, &pointer))
            }
            ValidationErrorsKind::List(items) => {
                for (index, inner) in items {
                    issues.extend(validation_issues(inner, &format!("{pointer}/{index}")));
                }
            }
        }
    }
    issues
}

/// Optimistic-concurrency precondition from the `If-Match` header (`"3"`, `W/"3"` or `*`).
pub struct IfMatch(pub Option<i32>);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, ApiError> {
        let Some(value) = parts.headers.get(header::IF_MATCH) else {
            return Ok(IfMatch(None));
        };
        let raw = value
            .to_str()
            .map_err(|_| ApiError::bad_request("invalid If-Match header"))?
            .trim();
        if raw == "*" {
            return Ok(IfMatch(None));
        }
        let tag = raw.trim_start_matches("W/").trim_matches('"');
        tag.parse::<i32>().map(|v| IfMatch(Some(v))).map_err(|_| {
            ApiError::bad_request("invalid If-Match header: expected an ETag returned by this API")
        })
    }
}

/// `cursor` / `limit` query parameters.
#[derive(Debug, Deserialize, Default)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

impl PageQuery {
    pub fn to_request(&self) -> Result<PageRequest, ApiError> {
        PageRequest::parse(self.cursor.as_deref(), self.limit).map_err(ApiError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_paths_to_json_pointers() {
        assert_eq!(json_pointer("steps[2].title"), "/steps/2/title");
        assert_eq!(json_pointer("."), "");
    }

    #[test]
    fn converts_field_names_to_camel_case() {
        assert_eq!(camel_case("service_rto_minutes"), "serviceRtoMinutes");
    }
}
