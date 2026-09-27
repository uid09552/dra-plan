use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{AiContext, AiSuggestion, AiSuggestionRepository, AiSuggestionStatus};
use crate::shared::infra::{Db, db_enum, write_err};
use crate::shared::kernel::{AppError, AppResult, Page, PageRequest, TenantContext};

#[derive(FromRow)]
struct SuggestionRow {
    id: Uuid,
    service_id: Uuid,
    kind: String,
    status: String,
    microservice_id: Option<Uuid>,
    scenario_id: Option<Uuid>,
    runbook_id: Option<Uuid>,
    recovery_run_id: Option<Uuid>,
    runbook_step_id: Option<Uuid>,
    prompt: Option<String>,
    language: String,
    proposals: String,
    summary: Option<String>,
    model: Option<String>,
    error: Option<String>,
    created_by: String,
    created_at: DateTime<Utc>,
    decided_by: Option<String>,
    decided_at: Option<DateTime<Utc>>,
}

impl TryFrom<SuggestionRow> for AiSuggestion {
    type Error = AppError;

    fn try_from(r: SuggestionRow) -> AppResult<Self> {
        Ok(AiSuggestion {
            id: r.id,
            service_id: r.service_id,
            kind: db_enum(&r.kind)?,
            status: db_enum(&r.status)?,
            language: db_enum(&r.language)?,
            context: AiContext {
                microservice_id: r.microservice_id,
                scenario_id: r.scenario_id,
                runbook_id: r.runbook_id,
                recovery_run_id: r.recovery_run_id,
                runbook_step_id: r.runbook_step_id,
                prompt: r.prompt,
            },
            proposals: r.proposals,
            summary: r.summary,
            model: r.model,
            error: r.error,
            created_by: r.created_by,
            created_at: r.created_at,
            decided_by: r.decided_by,
            decided_at: r.decided_at,
        })
    }
}

const SELECT: &str = "select id, service_id, kind, status, microservice_id, scenario_id, runbook_id, recovery_run_id,
        runbook_step_id, prompt, language, proposals::text as proposals, summary, model, error, created_by, created_at,
        decided_by, decided_at
    from ai_suggestion";

pub struct PgAiSuggestionRepository(pub Db);

#[async_trait]
impl AiSuggestionRepository for PgAiSuggestionRepository {
    async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Option<Uuid>,
        status: Option<AiSuggestionStatus>,
        page: PageRequest,
    ) -> AppResult<Page<AiSuggestion>> {
        let sql = format!(
            "{SELECT} where tenant_id = $1 and ($2::uuid is null or service_id = $2) and ($3::text is null or status = $3)
             order by created_at desc, id limit $4 offset $5"
        );
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<SuggestionRow> = sqlx::query_as(&sql)
            .bind(ctx.tenant_id.0)
            .bind(service_id)
            .bind(status.map(|s| s.as_str()))
            .bind(page.fetch_limit())
            .bind(page.offset)
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        let items = rows
            .into_iter()
            .map(AiSuggestion::try_from)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Page::from_overfetch(items, page))
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<AiSuggestion>> {
        let sql = format!("{SELECT} where tenant_id = $1 and id = $2");
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<SuggestionRow> = sqlx::query_as(&sql)
            .bind(ctx.tenant_id.0)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        row.map(AiSuggestion::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, s: &AiSuggestion) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query(
            "insert into ai_suggestion (id, tenant_id, service_id, kind, status, microservice_id, scenario_id, runbook_id,
                    recovery_run_id, runbook_step_id, prompt, language, proposals, summary, model, error, created_by, created_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::jsonb, $14, $15, $16, $17, $18)",
        )
        .bind(s.id)
        .bind(ctx.tenant_id.0)
        .bind(s.service_id)
        .bind(s.kind.as_str())
        .bind(s.status.as_str())
        .bind(s.context.microservice_id)
        .bind(s.context.scenario_id)
        .bind(s.context.runbook_id)
        .bind(s.context.recovery_run_id)
        .bind(s.context.runbook_step_id)
        .bind(&s.context.prompt)
        .bind(s.language.as_str())
        .bind(&s.proposals)
        .bind(&s.summary)
        .bind(&s.model)
        .bind(&s.error)
        .bind(&s.created_by)
        .bind(s.created_at)
        .execute(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(())
    }

    async fn update(&self, ctx: &TenantContext, s: &AiSuggestion) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("update ai_suggestion set status = $3, decided_by = $4, decided_at = $5 where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(s.id)
            .bind(s.status.as_str())
            .bind(&s.decided_by)
            .bind(s.decided_at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
