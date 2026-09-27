//! Advisory AI suggestions. AI output is untrusted input: it is stored as a suggestion and only
//! applied after an explicit decision, through the same validation as user input.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Language, Page, PageRequest, TenantContext, str_enum,
};

str_enum! {
    pub enum AiSuggestionKind {
        DescribeService = "describe_service",
        SuggestMicroservices = "suggest_microservices",
        SuggestDependencies = "suggest_dependencies",
        CheckBia = "check_bia",
        SuggestScenarios = "suggest_scenarios",
        ConsolidateScenarios = "consolidate_scenarios",
        AssessScenarios = "assess_scenarios",
        SuggestObjectives = "suggest_objectives",
        SuggestStrategies = "suggest_strategies",
        GapCheck = "gap_check",
        DraftRunbook = "draft_runbook",
        ReviewRunbook = "review_runbook",
        CheckRoles = "check_roles",
        DraftCommunication = "draft_communication",
        ReviewPlan = "review_plan",
        TabletopScript = "tabletop_script",
        SummarizeTest = "summarize_test",
        LessonsLearned = "lessons_learned",
        TroubleshootStep = "troubleshoot_step",
        StatusUpdate = "status_update",
    }
}

str_enum! {
    pub enum AiSuggestionStatus {
        Pending = "pending",
        Proposed = "proposed",
        Accepted = "accepted",
        PartiallyAccepted = "partially_accepted",
        Rejected = "rejected",
        Failed = "failed",
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiContext {
    pub microservice_id: Option<Uuid>,
    pub scenario_id: Option<Uuid>,
    pub runbook_id: Option<Uuid>,
    pub recovery_run_id: Option<Uuid>,
    pub runbook_step_id: Option<Uuid>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiSuggestion {
    pub id: Uuid,
    pub service_id: Uuid,
    pub kind: AiSuggestionKind,
    pub status: AiSuggestionStatus,
    pub language: Language,
    pub context: AiContext,
    /// JSON array of proposals (shape documented in the OpenAPI `AiProposal` schema).
    pub proposals: String,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// What an AI provider returns.
#[derive(Debug, Clone)]
pub struct AiAnswer {
    pub proposals: String,
    pub summary: Option<String>,
    pub model: String,
}

/// Port: an AI provider. The request is minimized and never contains secrets.
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn suggest(
        &self,
        kind: AiSuggestionKind,
        language: Language,
        context_document: &str,
    ) -> AppResult<AiAnswer>;
}

/// Adapter used until a provider is chosen (docs/overview/open-questions.md): AI is unavailable,
/// all other functions keep working.
pub struct NoAiProvider;

#[async_trait]
impl AiProvider for NoAiProvider {
    async fn suggest(
        &self,
        _kind: AiSuggestionKind,
        _language: Language,
        _context: &str,
    ) -> AppResult<AiAnswer> {
        Err(AppError::Unavailable("no AI provider is configured".into()))
    }
}

#[async_trait]
pub trait AiSuggestionRepository: Send + Sync {
    async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Option<Uuid>,
        status: Option<AiSuggestionStatus>,
        page: PageRequest,
    ) -> AppResult<Page<AiSuggestion>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<AiSuggestion>>;
    async fn insert(&self, ctx: &TenantContext, s: &AiSuggestion) -> AppResult<()>;
    async fn update(&self, ctx: &TenantContext, s: &AiSuggestion) -> AppResult<()>;
}
