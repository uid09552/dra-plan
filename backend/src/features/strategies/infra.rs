use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{RecoveryStrategy, StrategyRepository};
use crate::features::action_items::domain::ActionItem;
use crate::features::action_items::infra::insert_action_item;
use crate::shared::infra::{Db, MetaRow, ProvenanceRow, Tx, db_enum, require_updated, write_err};
use crate::shared::kernel::{AppError, AppResult, Minutes, TenantContext};

#[derive(FromRow)]
struct StrategyRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    #[sqlx(flatten)]
    provenance: ProvenanceRow,
    microservice_id: Uuid,
    #[sqlx(rename = "type")]
    strategy_type: String,
    title: Option<String>,
    description: Option<String>,
    estimated_rto_minutes: i32,
    estimated_rpo_minutes: i32,
    cost_notes: Option<String>,
    prerequisites: Vec<String>,
    implementation_status: String,
    last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    is_selected: bool,
    gap_justification: Option<String>,
    accepted_gap_action_item_id: Option<Uuid>,
}

impl StrategyRow {
    fn into_domain(self, scenario_ids: Vec<Uuid>) -> AppResult<RecoveryStrategy> {
        let r = self;
        Ok(RecoveryStrategy {
            meta: r.meta.into(),
            provenance: r.provenance.try_into()?,
            microservice_id: r.microservice_id,
            scenario_ids,
            strategy_type: db_enum(&r.strategy_type)?,
            title: r.title,
            description: r.description,
            estimated_rto: Minutes::from_db(r.estimated_rto_minutes)?,
            estimated_rpo: Minutes::from_db(r.estimated_rpo_minutes)?,
            cost_notes: r.cost_notes,
            prerequisites: r.prerequisites,
            implementation_status: db_enum(&r.implementation_status)?,
            last_tested_at: r.last_tested_at,
            is_selected: r.is_selected,
            gap_justification: r.gap_justification,
            accepted_gap_action_item_id: r.accepted_gap_action_item_id,
        })
    }
}

const UPDATE: &str = "update recovery_strategy set type = $4, title = $5, description = $6,
        estimated_rto_minutes = $7, estimated_rpo_minutes = $8, cost_notes = $9, prerequisites = $10,
        is_selected = $11, gap_justification = $12, accepted_gap_action_item_id = $13,
        implementation_status = $15, last_tested_at = $16, version = version + 1, updated_at = now(), updated_by = $14
    where tenant_id = $1 and id = $2 and version = $3 returning *";

/// Loads the scenario links of the rows and builds the domain objects (row order is kept).
async fn with_scenarios(tx: &mut Tx, rows: Vec<StrategyRow>) -> AppResult<Vec<RecoveryStrategy>> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.meta.id).collect();
    let links: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "select strategy_id, scenario_id from recovery_strategy_scenario
         where strategy_id = any($1) order by scenario_id",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut by_strategy: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (strategy, scenario) in links {
        by_strategy.entry(strategy).or_default().push(scenario);
    }
    rows.into_iter()
        .map(|r| {
            let scenarios = by_strategy.remove(&r.meta.id).unwrap_or_default();
            r.into_domain(scenarios)
        })
        .collect()
}

async fn one(tx: &mut Tx, row: StrategyRow) -> AppResult<RecoveryStrategy> {
    with_scenarios(tx, vec![row])
        .await?
        .pop()
        .ok_or_else(|| AppError::internal("strategy row vanished"))
}

/// Replaces the scenario links of a strategy.
async fn save_scenarios(tx: &mut Tx, ctx: &TenantContext, s: &RecoveryStrategy) -> AppResult<()> {
    sqlx::query(
        "delete from recovery_strategy_scenario
         where tenant_id = $1 and strategy_id = $2 and not (scenario_id = any($3))",
    )
    .bind(ctx.tenant_id.0)
    .bind(s.meta.id)
    .bind(&s.scenario_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "insert into recovery_strategy_scenario (tenant_id, strategy_id, scenario_id)
         select $1, $2, unnest($3::uuid[]) on conflict do nothing",
    )
    .bind(ctx.tenant_id.0)
    .bind(s.meta.id)
    .bind(&s.scenario_ids)
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

pub struct PgStrategyRepository(pub Db);

impl PgStrategyRepository {
    async fn update_in(
        tx: &mut Tx,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
    ) -> AppResult<RecoveryStrategy> {
        let row: Option<StrategyRow> = sqlx::query_as(UPDATE)
            .bind(ctx.tenant_id.0)
            .bind(s.meta.id)
            .bind(s.meta.version)
            .bind(s.strategy_type.as_str())
            .bind(&s.title)
            .bind(&s.description)
            .bind(s.estimated_rto.to_db())
            .bind(s.estimated_rpo.to_db())
            .bind(&s.cost_notes)
            .bind(&s.prerequisites)
            .bind(s.is_selected)
            .bind(&s.gap_justification)
            .bind(s.accepted_gap_action_item_id)
            .bind(ctx.actor())
            .bind(s.implementation_status.as_str())
            .bind(s.last_tested_at)
            .fetch_optional(&mut **tx)
            .await
            .map_err(write_err)?;
        let row = require_updated(row)?;
        save_scenarios(tx, ctx, s).await?;
        one(tx, row).await
    }
}

#[async_trait]
impl StrategyRepository for PgStrategyRepository {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<RecoveryStrategy>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<StrategyRow> = sqlx::query_as(
            "select s.* from recovery_strategy s where s.tenant_id = $1 and s.microservice_id = $2
               and ($3::uuid is null or exists (
                   select 1 from recovery_strategy_scenario l where l.strategy_id = s.id and l.scenario_id = $3))
             order by s.is_selected desc, s.created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(microservice_id)
        .bind(scenario_id)
        .fetch_all(&mut *tx)
        .await?;
        let strategies = with_scenarios(&mut tx, rows).await?;
        tx.commit().await?;
        Ok(strategies)
    }

    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RecoveryStrategy>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<StrategyRow> = sqlx::query_as(
            "select s.* from recovery_strategy s join microservice m on m.id = s.microservice_id
             where s.tenant_id = $1 and m.service_id = $2 order by s.microservice_id, s.created_at",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_all(&mut *tx)
        .await?;
        let strategies = with_scenarios(&mut tx, rows).await?;
        tx.commit().await?;
        Ok(strategies)
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryStrategy>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<StrategyRow> =
            sqlx::query_as("select * from recovery_strategy where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let strategy = match row {
            Some(row) => Some(one(&mut tx, row).await?),
            None => None,
        };
        tx.commit().await?;
        Ok(strategy)
    }

    async fn insert(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
    ) -> AppResult<RecoveryStrategy> {
        let mut tx = self.0.begin(ctx).await?;
        let row: StrategyRow = sqlx::query_as(
            "insert into recovery_strategy (id, tenant_id, microservice_id, type, title, description,
                    estimated_rto_minutes, estimated_rpo_minutes, cost_notes, prerequisites, origin, ai_suggestion_id,
                    created_by, updated_by, implementation_status, last_tested_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13, $14, $15) returning *",
        )
        .bind(s.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(s.microservice_id)
        .bind(s.strategy_type.as_str())
        .bind(&s.title)
        .bind(&s.description)
        .bind(s.estimated_rto.to_db())
        .bind(s.estimated_rpo.to_db())
        .bind(&s.cost_notes)
        .bind(&s.prerequisites)
        .bind(s.provenance.origin.as_str())
        .bind(s.provenance.ai_suggestion_id)
        .bind(ctx.actor())
        .bind(s.implementation_status.as_str())
        .bind(s.last_tested_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        save_scenarios(&mut tx, ctx, s).await?;
        let saved = one(&mut tx, row).await?;
        tx.commit().await?;
        Ok(saved)
    }

    async fn update(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
    ) -> AppResult<RecoveryStrategy> {
        let mut tx = self.0.begin(ctx).await?;
        let saved = Self::update_in(&mut tx, ctx, s).await?;
        tx.commit().await?;
        Ok(saved)
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from recovery_strategy where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn select(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
        gap_item: Option<&ActionItem>,
    ) -> AppResult<RecoveryStrategy> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query(
            "update recovery_strategy o set is_selected = false, version = version + 1, updated_at = now(), updated_by = $4
             where o.tenant_id = $1 and o.microservice_id = $2 and o.is_selected and o.id <> $5
               and exists (select 1 from recovery_strategy_scenario l
                           where l.strategy_id = o.id and l.scenario_id = any($3))",
        )
        .bind(ctx.tenant_id.0)
        .bind(s.microservice_id)
        .bind(&s.scenario_ids)
        .bind(ctx.actor())
        .bind(s.meta.id)
        .execute(&mut *tx)
        .await?;
        if let Some(item) = gap_item {
            insert_action_item(&mut tx, ctx, item).await?;
        }
        let saved = Self::update_in(&mut tx, ctx, s).await?;
        tx.commit().await?;
        Ok(saved)
    }
}
