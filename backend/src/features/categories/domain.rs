//! Tenant-defined scenario categories, in addition to the 15 built-in ones
//! (docs/requirements/README.md §2). Lets a tenant extend scenario brainstorming categories
//! (Settings > Categories in the UI) without a code change.

use async_trait::async_trait;
use uuid::Uuid;

use crate::features::scenarios::domain::ScenarioCategory;
use crate::shared::kernel::{AppResult, Issues, Meta, TenantContext};

#[derive(Debug, Clone, PartialEq)]
pub struct CustomCategory {
    pub meta: Meta,
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct CustomCategoryInput {
    pub key: Option<String>,
    pub label: Option<String>,
}

impl CustomCategory {
    pub fn create(ctx: &TenantContext, input: CustomCategoryInput) -> AppResult<Self> {
        let mut c = CustomCategory {
            meta: Meta::new(ctx),
            key: String::new(),
            label: String::new(),
        };
        c.apply(input)?;
        Ok(c)
    }

    pub fn apply(&mut self, p: CustomCategoryInput) -> AppResult<()> {
        if let Some(v) = p.key {
            self.key = v.trim().to_ascii_lowercase();
        }
        if let Some(v) = p.label {
            self.label = v.trim().to_owned();
        }
        let mut issues = Issues::new();
        issues.check(
            !self.label.is_empty(),
            "REQUIRED",
            "/label",
            "label is required",
        );
        issues.check(
            is_valid_key(&self.key),
            "INVALID_VALUE",
            "/key",
            "key must be 2-40 lowercase letters, digits, `_` or `-`",
        );
        issues.check(
            ScenarioCategory::ALL.iter().all(|c| c.as_str() != self.key),
            "DUPLICATE",
            "/key",
            "key collides with a built-in category",
        );
        issues.into_result()
    }
}

pub fn is_valid_key(key: &str) -> bool {
    (2..=40).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[async_trait]
pub trait CustomCategoryRepository: Send + Sync {
    async fn list(&self, ctx: &TenantContext) -> AppResult<Vec<CustomCategory>>;
    async fn insert(&self, ctx: &TenantContext, c: &CustomCategory) -> AppResult<CustomCategory>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::kernel::{Principal, TenantId, TenantRole};

    fn ctx() -> TenantContext {
        TenantContext {
            tenant_id: TenantId(Uuid::nil()),
            principal: Principal {
                user: "u".into(),
                role: TenantRole::Admin,
            },
        }
    }

    #[test]
    fn rejects_blank_label_and_invalid_key() {
        let err = CustomCategory::create(
            &ctx(),
            CustomCategoryInput {
                key: Some("ab".into()),
                label: Some(String::new()),
            },
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("label"));

        let err = CustomCategory::create(
            &ctx(),
            CustomCategoryInput {
                key: Some("A B".into()),
                label: Some("Vendor lock-in".into()),
            },
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("key"));
    }

    #[test]
    fn rejects_key_colliding_with_a_built_in_category() {
        let err = CustomCategory::create(
            &ctx(),
            CustomCategoryInput {
                key: Some("network".into()),
                label: Some("Network".into()),
            },
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("key"));
    }

    #[test]
    fn accepts_a_valid_custom_category() {
        let c = CustomCategory::create(
            &ctx(),
            CustomCategoryInput {
                key: Some("vendor-lock-in".into()),
                label: Some("Vendor lock-in".into()),
            },
        )
        .expect("valid");
        assert_eq!(c.key, "vendor-lock-in");
    }
}
