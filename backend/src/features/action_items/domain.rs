//! Improvement actions from tests, runs, reviews and gap checks (workflow step 15).

use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::shared::kernel::{
    AppResult, Issues, Meta, Provenance, TenantContext, non_blank, str_enum,
};

str_enum! {
    pub enum ActionItemSource { Test = "test", Run = "run", Review = "review", AiGapCheck = "ai_gap_check", Manual = "manual" }
}

str_enum! {
    pub enum ActionItemStatus { Open = "open", InProgress = "in_progress", Done = "done", WontFix = "wont_fix" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelatedEntity {
    pub entity_type: String,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionItem {
    pub meta: Meta,
    pub provenance: Provenance,
    pub service_id: Uuid,
    pub source: ActionItemSource,
    pub source_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub owner_person_id: Option<Uuid>,
    pub due_date: Option<NaiveDate>,
    pub status: ActionItemStatus,
    pub related_entity: Option<RelatedEntity>,
}

#[derive(Debug, Clone, Default)]
pub struct ActionItemInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub owner_person_id: Option<Option<Uuid>>,
    pub due_date: Option<Option<NaiveDate>>,
    pub status: Option<ActionItemStatus>,
    pub related_entity: Option<Option<RelatedEntity>>,
}

impl ActionItem {
    pub fn create(
        ctx: &TenantContext,
        service_id: Uuid,
        source: ActionItemSource,
        source_id: Option<Uuid>,
        input: ActionItemInput,
    ) -> AppResult<Self> {
        let mut item = ActionItem {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            service_id,
            source,
            source_id,
            title: String::new(),
            description: None,
            owner_person_id: None,
            due_date: None,
            status: ActionItemStatus::Open,
            related_entity: None,
        };
        item.apply(input)?;
        Ok(item)
    }

    pub fn apply(&mut self, p: ActionItemInput) -> AppResult<()> {
        if let Some(v) = p.title {
            self.title = v.trim().to_owned();
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        if let Some(v) = p.owner_person_id {
            self.owner_person_id = v;
        }
        if let Some(v) = p.due_date {
            self.due_date = v;
        }
        if let Some(v) = p.status {
            self.status = v;
        }
        if let Some(v) = p.related_entity {
            self.related_entity = v;
        }
        let mut issues = Issues::new();
        issues.check(
            !self.title.is_empty(),
            "REQUIRED",
            "/title",
            "title is required",
        );
        issues.into_result()
    }

    pub fn is_open(&self) -> bool {
        matches!(
            self.status,
            ActionItemStatus::Open | ActionItemStatus::InProgress
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActionItemFilter {
    pub status: Option<ActionItemStatus>,
    pub source: Option<ActionItemSource>,
}

#[async_trait]
pub trait ActionItemRepository: Send + Sync {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        filter: &ActionItemFilter,
    ) -> AppResult<Vec<ActionItem>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<ActionItem>>;
    async fn insert(&self, ctx: &TenantContext, item: &ActionItem) -> AppResult<ActionItem>;
    async fn update(&self, ctx: &TenantContext, item: &ActionItem) -> AppResult<ActionItem>;
}
