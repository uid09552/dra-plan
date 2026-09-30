use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Scenario, ScenarioFilter, ScenarioRepository};
use crate::shared::infra::{
    Db, MetaRow, ProvenanceRow, Tx, db_enum, db_enum_opt, db_rating, write_err,
};
use crate::shared::kernel::{AppError, AppResult, TenantContext};

#[derive(FromRow)]
struct ScenarioRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    service_id: Uuid,
    title: String,
    description: Option<String>,
    category: Option<String>,
    parent_scenario_id: Option<Uuid>,
    status: String,
    merged_into_id: Option<Uuid>,
    likelihood: Option<i32>,
    impact: Option<i32>,
    priority: Option<String>,
    dr_required: Option<String>,
    decision_rationale: Option<String>,
    decided_by: Option<String>,
    decided_at: Option<DateTime<Utc>>,
}

impl ScenarioRow {
    fn into_domain(self, affected: Vec<Uuid>) -> AppResult<Scenario> {
        Ok(Scenario {
            meta: self.meta.into(),
            provenance: self.provenance.try_into()?,
            service_id: self.service_id,
            title: self.title,
            description: self.description,
            category: self.category,
            parent_scenario_id: self.parent_scenario_id,
            affected_microservice_ids: affected,
            status: db_enum(&self.status)?,
            merged_into_id: self.merged_into_id,
            likelihood: db_rating(self.likelihood)?,
            impact: db_rating(self.impact)?,
            priority: db_enum_opt(self.priority.as_deref())?,
            dr_required: db_enum_opt(self.dr_required.as_deref())?,
            decision_rationale: self.decision_rationale,
            decided_by: self.decided_by,
            decided_at: self.decided_at,
        })
    }
}

async fn with_affected(tx: &mut Tx, rows: Vec<ScenarioRow>) -> AppResult<Vec<Scenario>> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.meta.id).collect();
    let links: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "select scenario_id, microservice_id from scenario_microservice where scenario_id = any($1)
         order by microservice_id",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut by_scenario: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (s, m) in links {
        by_scenario.entry(s).or_default().push(m);
    }
    rows.into_iter()
        .map(|r| {
            let affected = by_scenario.remove(&r.meta.id).unwrap_or_default();
            r.into_domain(affected)
        })
        .collect()
}

async fn replace_affected(tx: &mut Tx, ctx: &TenantContext, s: &Scenario) -> AppResult<()> {
    sqlx::query("delete from scenario_microservice where scenario_id = $1")
        .bind(s.meta.id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "insert into scenario_microservice (tenant_id, scenario_id, microservice_id)
         select $1, $2, unnest($3::uuid[])",
    )
    .bind(ctx.tenant_id.0)
    .bind(s.meta.id)
    .bind(&s.affected_microservice_ids)
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

pub struct PgScenarioRepository(pub Db);

#[async_trait]
impl ScenarioRepository for PgScenarioRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        f: &ScenarioFilter,
    ) -> AppResult<Vec<Scenario>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ScenarioRow> = sqlx::query_as(
            "select * from scenario
             where tenant_id = $1 and service_id = $2
               and ($3::text is null or status = $3)
               and ($4::text is null or category = $4)
               and ($5::text is null or dr_required = $5)
             order by created_at, id",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .bind(f.status.map(|v| v.as_str()))
        .bind(f.category.as_deref())
        .bind(f.dr_required.map(|v| v.as_str()))
        .fetch_all(&mut *tx)
        .await?;
        let result = with_affected(&mut tx, rows).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Scenario>> {
        Ok(self.get_many(ctx, &[id]).await?.pop())
    }

    async fn get_many(&self, ctx: &TenantContext, ids: &[Uuid]) -> AppResult<Vec<Scenario>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<ScenarioRow> =
            sqlx::query_as("select * from scenario where tenant_id = $1 and id = any($2)")
                .bind(ctx.tenant_id.0)
                .bind(ids)
                .fetch_all(&mut *tx)
                .await?;
        let result = with_affected(&mut tx, rows).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn insert(&self, ctx: &TenantContext, s: &Scenario) -> AppResult<Scenario> {
        let mut tx = self.0.begin(ctx).await?;
        let row: ScenarioRow = sqlx::query_as(
            "insert into scenario (id, tenant_id, service_id, title, description, category, status, origin,
                                   ai_suggestion_id, created_by, updated_by, parent_scenario_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11) returning *",
        )
        .bind(s.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(s.service_id)
        .bind(&s.title)
        .bind(&s.description)
        .bind(s.category.as_deref())
        .bind(s.status.as_str())
        .bind(s.provenance.origin.as_str())
        .bind(s.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .bind(s.parent_scenario_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        replace_affected(&mut tx, ctx, s).await?;
        let mut saved = with_affected(&mut tx, vec![row]).await?;
        tx.commit().await?;
        saved
            .pop()
            .ok_or_else(|| AppError::internal("insert returned nothing"))
    }

    async fn update_all(
        &self,
        ctx: &TenantContext,
        scenarios: &[Scenario],
    ) -> AppResult<Vec<Scenario>> {
        let mut tx = self.0.begin(ctx).await?;
        let mut rows = Vec::with_capacity(scenarios.len());
        for s in scenarios {
            let row: Option<ScenarioRow> = sqlx::query_as(
                "update scenario set title = $4, description = $5, category = $6, status = $7, merged_into_id = $8,
                        likelihood = $9, impact = $10, priority = $11, dr_required = $12, decision_rationale = $13,
                        decided_by = $14, decided_at = $15, parent_scenario_id = $17,
                        version = version + 1, updated_at = now(), updated_by = $16
                 where tenant_id = $1 and id = $2 and version = $3 returning *",
            )
            .bind(ctx.tenant_id.0)
            .bind(s.meta.id)
            .bind(s.meta.version)
            .bind(&s.title)
            .bind(&s.description)
            .bind(s.category.as_deref())
            .bind(s.status.as_str())
            .bind(s.merged_into_id)
            .bind(s.likelihood.map(|r| i32::from(r.get())))
            .bind(s.impact.map(|r| i32::from(r.get())))
            .bind(s.priority.map(|v| v.as_str()))
            .bind(s.dr_required.map(|v| v.as_str()))
            .bind(&s.decision_rationale)
            .bind(&s.decided_by)
            .bind(s.decided_at)
            .bind(ctx.actor())
            .bind(s.parent_scenario_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(write_err)?;
            rows.push(row.ok_or(AppError::PreconditionFailed)?);
            replace_affected(&mut tx, ctx, s).await?;
        }
        let saved = with_affected(&mut tx, rows).await?;
        tx.commit().await?;
        Ok(saved)
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from scenario where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
