use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use super::domain::{
    AiContext, AiProvider, AiSuggestion, AiSuggestionKind, AiSuggestionRepository,
    AiSuggestionStatus,
};
use crate::features::plans::application::PlanUseCases;
use crate::features::plans::domain::SnapshotCodec;
use crate::features::tenants::domain::TenantRepository;
use crate::shared::kernel::{AppError, AppResult, Language, Page, PageRequest, TenantContext};

/// Decision on one proposal.
pub enum ProposalDecision {
    Accept,
    Edit,
    Reject,
}

pub struct AiUseCases {
    repo: Arc<dyn AiSuggestionRepository>,
    provider: Arc<dyn AiProvider>,
    plans: Arc<PlanUseCases>,
    codec: Arc<dyn SnapshotCodec>,
    tenants: Arc<dyn TenantRepository>,
}

impl AiUseCases {
    pub fn new(
        repo: Arc<dyn AiSuggestionRepository>,
        provider: Arc<dyn AiProvider>,
        plans: Arc<PlanUseCases>,
        codec: Arc<dyn SnapshotCodec>,
        tenants: Arc<dyn TenantRepository>,
    ) -> Self {
        Self {
            repo,
            provider,
            plans,
            codec,
            tenants,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Option<Uuid>,
        status: Option<AiSuggestionStatus>,
        page: PageRequest,
    ) -> AppResult<Page<AiSuggestion>> {
        self.repo.list(ctx, service_id, status, page).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<AiSuggestion> {
        self.repo
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("AI suggestion"))
    }

    /// Asks the provider for a suggestion. The service subtree (plan snapshot format) is the
    /// context document; person contact details are removed before sending (data minimization).
    pub async fn request(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        kind: AiSuggestionKind,
        language: Language,
        context: AiContext,
    ) -> AppResult<AiSuggestion> {
        let tenant = self
            .tenants
            .get(ctx)
            .await?
            .ok_or(AppError::NotFound("tenant"))?;
        if !tenant.settings.ai_enabled {
            return Err(AppError::Unavailable(
                "AI assistance is disabled for this tenant".into(),
            ));
        }
        let mut aggregate = self.plans.loader().load(ctx, service_id).await?;
        for p in aggregate.persons.iter_mut().chain(
            aggregate
                .role_assignments
                .iter_mut()
                .filter_map(|a| a.person.as_mut()),
        ) {
            p.email = None;
            p.phone = None;
            p.alternate_contact = None;
        }
        let document = self.codec.encode(&aggregate)?;
        let answer = self.provider.suggest(kind, language, &document).await?;
        let suggestion = AiSuggestion {
            id: Uuid::new_v4(),
            service_id,
            kind,
            status: AiSuggestionStatus::Proposed,
            language,
            context,
            proposals: answer.proposals,
            summary: answer.summary,
            model: Some(answer.model),
            error: None,
            created_by: ctx.actor().to_owned(),
            created_at: Utc::now(),
            decided_by: None,
            decided_at: None,
        };
        self.repo.insert(ctx, &suggestion).await?;
        Ok(suggestion)
    }

    /// Records decisions. Rejections are always possible; applying accepted proposals to the plan
    /// is not implemented yet (it will reuse the regular use cases and their validation).
    pub async fn decide(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        decisions: &[ProposalDecision],
    ) -> AppResult<AiSuggestion> {
        let mut suggestion = self.get(ctx, id).await?;
        if suggestion.status != AiSuggestionStatus::Proposed {
            return Err(AppError::conflict(
                "the suggestion has already been decided",
            ));
        }
        if decisions
            .iter()
            .any(|d| !matches!(d, ProposalDecision::Reject))
        {
            return Err(AppError::NotImplemented(
                "applying AI proposals is not implemented yet; reject them or enter the content manually".into(),
            ));
        }
        suggestion.status = AiSuggestionStatus::Rejected;
        suggestion.decided_by = Some(ctx.actor().to_owned());
        suggestion.decided_at = Some(Utc::now());
        self.repo.update(ctx, &suggestion).await?;
        Ok(suggestion)
    }
}
