use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::context::{TenantContext, TenantId};
use super::enums::str_enum;
use super::error::{AppError, AppResult};

/// A non-negative duration in whole minutes (at most ten years).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Minutes(u32);

impl Minutes {
    pub const MAX: u32 = 10 * 366 * 24 * 60;

    pub fn new(value: u32) -> AppResult<Self> {
        if value > Self::MAX {
            return Err(AppError::invalid(
                "OUT_OF_RANGE",
                None,
                format!("duration must be at most {} minutes", Self::MAX),
            ));
        }
        Ok(Self(value))
    }

    /// Clamps to the valid range (used for computed sums such as critical paths).
    pub fn saturating(value: u64) -> Self {
        Self(u32::try_from(value.min(u64::from(Self::MAX))).unwrap_or(Self::MAX))
    }

    /// For values read back from storage, where the range is guaranteed by constraints.
    pub fn from_db(value: i32) -> AppResult<Self> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| AppError::internal(format!("negative duration in storage: {value}")))
    }

    pub fn get(self) -> u32 {
        self.0
    }

    pub fn to_db(self) -> i32 {
        // Bounded by MAX, which fits into i32.
        i32::try_from(self.0).unwrap_or(i32::MAX)
    }
}

/// A 1..=4 rating (likelihood, impact, impact level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rating(u8);

impl Rating {
    pub fn new(value: i64) -> AppResult<Self> {
        if (1..=4).contains(&value) {
            Ok(Self(value as u8))
        } else {
            Err(AppError::invalid(
                "OUT_OF_RANGE",
                None,
                "rating must be between 1 and 4",
            ))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

str_enum! {
    /// Provenance of content: typed by a user, accepted from an AI suggestion, or imported.
    pub enum Origin { User = "user", AiAccepted = "ai_accepted", Import = "import" }
}

str_enum! {
    /// UI / export language.
    pub enum Language { En = "en", De = "de" }
}

str_enum! {
    /// BIA impact categories (NIST SP 800-34 BIA, BSI 200-4 Schadenskategorien).
    pub enum ImpactCategory {
        Financial = "financial",
        Operational = "operational",
        Reputational = "reputational",
        LegalRegulatory = "legal_regulatory",
        PeopleSafety = "people_safety",
    }
}

/// Common metadata of every persisted resource.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

impl Meta {
    /// Metadata for a resource that is about to be created.
    pub fn new(ctx: &TenantContext) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            version: 1,
            created_at: now,
            created_by: ctx.actor().to_owned(),
            updated_at: now,
            updated_by: ctx.actor().to_owned(),
        }
    }

    /// Checks an `If-Match` precondition against the current version.
    pub fn check_version(&self, expected: Option<i32>) -> AppResult<()> {
        match expected {
            Some(v) if v != self.version => Err(AppError::PreconditionFailed),
            _ => Ok(()),
        }
    }
}

/// Provenance fields of content resources.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub origin: Origin,
    pub ai_suggestion_id: Option<Uuid>,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            origin: Origin::User,
            ai_suggestion_id: None,
        }
    }
}

/// Offset-based paging behind an opaque cursor.
#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    pub offset: i64,
    pub limit: i64,
}

impl PageRequest {
    pub const DEFAULT_LIMIT: i64 = 50;
    pub const MAX_LIMIT: i64 = 200;

    pub fn parse(cursor: Option<&str>, limit: Option<i64>) -> AppResult<Self> {
        let limit = limit.unwrap_or(Self::DEFAULT_LIMIT);
        if !(1..=Self::MAX_LIMIT).contains(&limit) {
            return Err(AppError::invalid(
                "OUT_OF_RANGE",
                Some("limit"),
                format!("limit must be between 1 and {}", Self::MAX_LIMIT),
            ));
        }
        let offset = match cursor {
            None | Some("") => 0,
            Some(c) => c.parse::<i64>().ok().filter(|o| *o >= 0).ok_or_else(|| {
                AppError::invalid("INVALID_CURSOR", Some("cursor"), "invalid cursor")
            })?,
        };
        Ok(Self { offset, limit })
    }

    /// Repositories fetch one extra row to detect whether another page exists.
    pub fn fetch_limit(self) -> i64 {
        self.limit + 1
    }
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn from_overfetch(mut items: Vec<T>, request: PageRequest) -> Self {
        let has_more = items.len() as i64 > request.limit;
        items.truncate(request.limit as usize);
        let next_cursor = has_more.then(|| (request.offset + request.limit).to_string());
        Self { items, next_cursor }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_is_bounded() {
        assert!(Rating::new(0).is_err());
        assert_eq!(Rating::new(4).map(Rating::get).ok(), Some(4));
        assert!(Rating::new(5).is_err());
    }

    #[test]
    fn minutes_are_bounded() {
        assert!(Minutes::new(Minutes::MAX).is_ok());
        assert!(Minutes::new(Minutes::MAX + 1).is_err());
    }

    #[test]
    fn paging_detects_more_items() {
        let req = PageRequest::parse(Some("10"), Some(2)).expect("valid");
        let page = Page::from_overfetch(vec![1, 2, 3], req);
        assert_eq!(page.items, vec![1, 2]);
        assert_eq!(page.next_cursor.as_deref(), Some("12"));
        assert!(PageRequest::parse(Some("x"), None).is_err());
        assert!(PageRequest::parse(None, Some(201)).is_err());
    }
}
