//! Framework-free building blocks shared by all feature domains: identifiers, value objects,
//! the error type and the tenant context. No web, database or serialization frameworks here.

mod context;
mod enums;
mod error;
mod values;

pub use context::{Principal, TenantContext, TenantId, TenantRole};
pub use enums::ParseEnumError;
pub(crate) use enums::str_enum;
pub use error::{AppError, AppResult, Issue, Issues, Severity};
pub use values::{
    ImpactCategory, Language, Meta, Minutes, Origin, Page, PageRequest, Provenance, Rating,
};

/// Parses an optional enum value, turning an unknown value into a validation issue for `field`.
pub fn parse_enum<T: std::str::FromStr<Err = ParseEnumError>>(
    value: &str,
    field: &str,
) -> AppResult<T> {
    value
        .parse::<T>()
        .map_err(|e| AppError::invalid("INVALID_VALUE", Some(field), e.to_string()))
}

/// Trims a string and maps empty strings to `None`.
pub fn non_blank(value: Option<String>) -> Option<String> {
    value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}
