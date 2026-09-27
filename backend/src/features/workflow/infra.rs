use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{StepProgress, WorkflowRepository};
use crate::shared::infra::{Db, db_enum};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct ProgressRow {
    step_key: String,
    status: String,
    comment: Option<String>,
    completed_by: Option<String>,
    completed_at: Option<DateTime<Utc>>,
}

impl TryFrom<ProgressRow> for StepProgress {
    type Error = AppError;

    fn try_from(r: ProgressRow) -> AppResult<Self> {
        Ok(StepProgress {
            key: db_enum(&r.step_key)?,
            status: db_enum(&r.status)?,
            comment: r.comment,
            completed_by: r.completed_by,
            completed_at: r.completed_at,
        })
    }
}

pub struct PgWorkflowRepository(pub Db);

#[async_trait]
impl WorkflowRepository for PgWorkflowRepository {
    async fn list(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Vec<StepProgress>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ProgressRow> = sqlx::query_as(
            "select step_key, status, comment, completed_by, completed_at from workflow_progress
             where tenant_id = $1 and service_id = $2",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(StepProgress::try_from).collect()
    }

    async fn save(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        steps: &[StepProgress],
    ) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        for s in steps {
            sqlx::query(
                "insert into workflow_progress (tenant_id, service_id, step_key, status, comment, completed_by, completed_at)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (service_id, step_key) do update set status = excluded.status, comment = excluded.comment,
                    completed_by = excluded.completed_by, completed_at = excluded.completed_at, updated_at = now()",
            )
            .bind(ctx.tenant_id.0)
            .bind(service_id)
            .bind(s.key.as_str())
            .bind(s.status.as_str())
            .bind(&s.comment)
            .bind(&s.completed_by)
            .bind(s.completed_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
