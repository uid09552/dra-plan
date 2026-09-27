//! Recovery strategies ("mitigations", measures) per component and one or more scenarios (workflow
//! step 8), including the gap check (BSI 200-4 Soll-Ist-Vergleich): estimated capability vs.
//! recovery objective.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::features::action_items::domain::{
    ActionItem, ActionItemInput, ActionItemSource, RelatedEntity,
};
use crate::features::objectives::domain::{RecoveryObjective, effective};
use crate::shared::kernel::{
    AppError, AppResult, Issue, Issues, Meta, Minutes, Provenance, TenantContext, non_blank,
    str_enum,
};

str_enum! {
    pub enum StrategyType {
        BackupRestore = "backup_restore",
        Replication = "replication",
        ActivePassive = "active_passive",
        ActiveActive = "active_active",
        CrossRegionFailover = "cross_region_failover",
        IacRebuild = "iac_rebuild",
        DegradedMode = "degraded_mode",
        ManualWorkaround = "manual_workaround",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryStrategy {
    pub meta: Meta,
    pub provenance: Provenance,
    pub microservice_id: Uuid,
    /// Scenarios this measure covers (at least one, sorted, unique).
    pub scenario_ids: Vec<Uuid>,
    pub strategy_type: StrategyType,
    pub title: Option<String>,
    pub description: Option<String>,
    pub estimated_rto: Minutes,
    pub estimated_rpo: Minutes,
    pub cost_notes: Option<String>,
    pub prerequisites: Vec<String>,
    pub implementation_status: ImplementationStatus,
    /// When the recovery capability was last tested.
    pub last_tested_at: Option<DateTime<Utc>>,
    pub is_selected: bool,
    pub gap_justification: Option<String>,
    pub accepted_gap_action_item_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct StrategyInput {
    pub scenario_ids: Option<Vec<Uuid>>,
    pub strategy_type: Option<StrategyType>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub estimated_rto: Option<Minutes>,
    pub estimated_rpo: Option<Minutes>,
    pub cost_notes: Option<String>,
    pub prerequisites: Option<Vec<String>>,
    pub implementation_status: Option<ImplementationStatus>,
    pub last_tested_at: Option<Option<DateTime<Utc>>>,
}

str_enum! {
    /// Recovery capability: is the mitigation actually in place?
    pub enum ImplementationStatus {
        NotImplemented = "not_implemented",
        InProgress = "in_progress",
        Implemented = "implemented",
    }
}

str_enum! {
    pub enum GapStatus { Meets = "meets", Gap = "gap", NoObjective = "no_objective" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GapCheck {
    pub status: GapStatus,
    pub objective_id: Option<Uuid>,
    /// Estimated minus target; positive means the strategy is too slow.
    pub rto_gap_minutes: Option<i64>,
    pub rpo_gap_minutes: Option<i64>,
    pub accepted_gap_action_item_id: Option<Uuid>,
}

impl RecoveryStrategy {
    pub fn create(
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_ids: Vec<Uuid>,
        strategy_type: StrategyType,
        estimated_rto: Minutes,
        estimated_rpo: Minutes,
        mut input: StrategyInput,
    ) -> AppResult<Self> {
        input.scenario_ids = Some(scenario_ids);
        let mut s = RecoveryStrategy {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            microservice_id,
            scenario_ids: Vec::new(),
            strategy_type,
            title: None,
            description: None,
            estimated_rto,
            estimated_rpo,
            cost_notes: None,
            prerequisites: Vec::new(),
            implementation_status: ImplementationStatus::NotImplemented,
            last_tested_at: None,
            is_selected: false,
            gap_justification: None,
            accepted_gap_action_item_id: None,
        };
        s.apply(input)?;
        Ok(s)
    }

    pub fn apply(&mut self, p: StrategyInput) -> AppResult<()> {
        if let Some(mut ids) = p.scenario_ids {
            ids.sort();
            ids.dedup();
            if ids.is_empty() {
                return Err(AppError::invalid(
                    "REQUIRED",
                    Some("/scenarioIds"),
                    "a measure must cover at least one scenario",
                ));
            }
            self.scenario_ids = ids;
        }
        if let Some(v) = p.strategy_type {
            self.strategy_type = v;
        }
        if p.title.is_some() {
            self.title = non_blank(p.title);
        }
        if p.description.is_some() {
            self.description = non_blank(p.description);
        }
        if let Some(v) = p.estimated_rto {
            self.estimated_rto = v;
        }
        if let Some(v) = p.estimated_rpo {
            self.estimated_rpo = v;
        }
        if p.cost_notes.is_some() {
            self.cost_notes = non_blank(p.cost_notes);
        }
        if let Some(v) = p.prerequisites {
            self.prerequisites = v
                .into_iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Some(v) = p.implementation_status {
            self.implementation_status = v;
        }
        if let Some(v) = p.last_tested_at {
            self.last_tested_at = v;
        }
        Ok(())
    }

    pub fn covers(&self, scenario_id: Uuid) -> bool {
        self.scenario_ids.contains(&scenario_id)
    }

    /// Soll-Ist comparison over all covered scenarios: the worst gap counts.
    pub fn gap_check(&self, objectives: &[RecoveryObjective]) -> GapCheck {
        let checks: Vec<GapCheck> = self
            .scenario_ids
            .iter()
            .map(|sid| self.gap_for(objectives, *sid))
            .filter(|g| g.status != GapStatus::NoObjective)
            .collect();
        let worst = checks.iter().max_by_key(|g| {
            (
                g.status == GapStatus::Gap,
                g.rto_gap_minutes
                    .unwrap_or(i64::MIN)
                    .max(g.rpo_gap_minutes.unwrap_or(i64::MIN)),
            )
        });
        match worst {
            Some(g) => g.clone(),
            None => self.no_objective(),
        }
    }

    fn no_objective(&self) -> GapCheck {
        GapCheck {
            status: GapStatus::NoObjective,
            objective_id: None,
            rto_gap_minutes: None,
            rpo_gap_minutes: None,
            accepted_gap_action_item_id: self.accepted_gap_action_item_id,
        }
    }

    /// Soll-Ist comparison against the effective objective of the component in one scenario.
    pub fn gap_for(&self, objectives: &[RecoveryObjective], scenario_id: Uuid) -> GapCheck {
        let Some(objective) = effective(objectives, self.microservice_id, Some(scenario_id)) else {
            return self.no_objective();
        };
        let rto_gap = i64::from(self.estimated_rto.get()) - i64::from(objective.rto.get());
        let rpo_gap = i64::from(self.estimated_rpo.get()) - i64::from(objective.rpo.get());
        GapCheck {
            status: if rto_gap > 0 || rpo_gap > 0 {
                GapStatus::Gap
            } else {
                GapStatus::Meets
            },
            objective_id: Some(objective.meta.id),
            rto_gap_minutes: Some(rto_gap),
            rpo_gap_minutes: Some(rpo_gap),
            accepted_gap_action_item_id: self.accepted_gap_action_item_id,
        }
    }

    /// Selects this strategy. A gap must be accepted explicitly with a justification; the
    /// returned action item (to be persisted atomically) tracks closing the gap.
    pub fn select(
        &mut self,
        ctx: &TenantContext,
        service_id: Uuid,
        gap: &GapCheck,
        accept_gap: bool,
        justification: Option<String>,
    ) -> AppResult<Option<ActionItem>> {
        self.is_selected = true;
        if gap.status != GapStatus::Gap {
            self.gap_justification = None;
            self.accepted_gap_action_item_id = None;
            return Ok(None);
        }
        let justification = non_blank(justification);
        let mut issues = Issues::new();
        if !accept_gap {
            issues.push(
                Issue::blocking(
                    "STRATEGY_GAP",
                    format!(
                        "the strategy misses the objective (RTO gap {} min, RPO gap {} min); set acceptGap to accept it",
                        gap.rto_gap_minutes.unwrap_or(0).max(0),
                        gap.rpo_gap_minutes.unwrap_or(0).max(0)
                    ),
                )
                .field("/acceptGap")
                .entity("recovery_strategy", self.meta.id)
                .standard("BSI 200-4 Soll-Ist-Vergleich"),
            );
        }
        issues.check(
            justification.is_some() || !accept_gap,
            "REQUIRED",
            "/gapJustification",
            "a justification is required to accept a gap",
        );
        issues.into_result()?;
        let item = ActionItem::create(
            ctx,
            service_id,
            ActionItemSource::Review,
            Some(self.meta.id),
            ActionItemInput {
                title: Some(format!(
                    "Close recovery gap: {}",
                    self.title
                        .clone()
                        .unwrap_or_else(|| self.strategy_type.to_string())
                )),
                description: justification.clone(),
                related_entity: Some(Some(RelatedEntity {
                    entity_type: "recovery_strategy".into(),
                    id: self.meta.id,
                })),
                ..Default::default()
            },
        )?;
        self.gap_justification = justification;
        self.accepted_gap_action_item_id = Some(item.meta.id);
        Ok(Some(item))
    }
}

#[async_trait]
pub trait StrategyRepository: Send + Sync {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
        scenario_id: Option<Uuid>,
    ) -> AppResult<Vec<RecoveryStrategy>>;
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RecoveryStrategy>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<RecoveryStrategy>>;
    async fn insert(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
    ) -> AppResult<RecoveryStrategy>;
    async fn update(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
    ) -> AppResult<RecoveryStrategy>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
    /// Atomically: deselect other strategies of the pair, persist the gap action item (if any),
    /// and save the selected strategy.
    async fn select(
        &self,
        ctx: &TenantContext,
        s: &RecoveryStrategy,
        gap_item: Option<&ActionItem>,
    ) -> AppResult<RecoveryStrategy>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::objectives::domain::ObjectiveInput;
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

    fn m(v: u32) -> Minutes {
        Minutes::new(v).expect("valid")
    }

    #[test]
    fn detects_and_requires_acceptance_of_gaps() {
        let (ms, sc) = (Uuid::new_v4(), Uuid::new_v4());
        let objective =
            RecoveryObjective::create(&ctx(), ms, m(60), m(15), ObjectiveInput::default())
                .expect("valid");
        let mut strategy = RecoveryStrategy::create(
            &ctx(),
            ms,
            vec![sc],
            StrategyType::BackupRestore,
            m(240),
            m(60),
            StrategyInput::default(),
        )
        .expect("valid");
        let gap = strategy.gap_check(std::slice::from_ref(&objective));
        assert_eq!(gap.status, GapStatus::Gap);
        assert_eq!(gap.rto_gap_minutes, Some(180));
        assert!(
            strategy
                .clone()
                .select(&ctx(), Uuid::nil(), &gap, false, None)
                .is_err()
        );
        assert!(
            strategy
                .clone()
                .select(&ctx(), Uuid::nil(), &gap, true, None)
                .is_err()
        );
        let item = strategy
            .select(
                &ctx(),
                Uuid::nil(),
                &gap,
                true,
                Some("budget next year".into()),
            )
            .expect("accepted");
        assert!(item.is_some());
        assert!(strategy.is_selected);
        assert_eq!(
            strategy.accepted_gap_action_item_id,
            item.map(|i| i.meta.id)
        );
    }

    #[test]
    fn covers_several_scenarios_and_reports_the_worst_gap() {
        let (ms, a, b) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        // Default objective 60 min; scenario `b` tolerates 300 min.
        let default =
            RecoveryObjective::create(&ctx(), ms, m(60), m(15), ObjectiveInput::default())
                .expect("valid");
        let loose = RecoveryObjective::create(
            &ctx(),
            ms,
            m(300),
            m(60),
            ObjectiveInput {
                scenario_id: Some(Some(b)),
                ..Default::default()
            },
        )
        .expect("valid");
        let objectives = [default, loose];
        let strategy = RecoveryStrategy::create(
            &ctx(),
            ms,
            vec![b, a, b],
            StrategyType::BackupRestore,
            m(240),
            m(15),
            StrategyInput::default(),
        )
        .expect("valid");
        assert_eq!(strategy.scenario_ids.len(), 2, "deduplicated");
        assert!(strategy.covers(a) && strategy.covers(b));
        assert_eq!(strategy.gap_for(&objectives, b).status, GapStatus::Meets);
        let worst = strategy.gap_check(&objectives);
        assert_eq!(worst.status, GapStatus::Gap);
        assert_eq!(worst.rto_gap_minutes, Some(180));
        assert!(
            RecoveryStrategy::create(
                &ctx(),
                ms,
                vec![],
                StrategyType::BackupRestore,
                m(1),
                m(1),
                StrategyInput::default()
            )
            .is_err()
        );
    }

    #[test]
    fn meets_objective_without_gap() {
        let (ms, sc) = (Uuid::new_v4(), Uuid::new_v4());
        let objective =
            RecoveryObjective::create(&ctx(), ms, m(60), m(15), ObjectiveInput::default())
                .expect("valid");
        let strategy = RecoveryStrategy::create(
            &ctx(),
            ms,
            vec![sc],
            StrategyType::ActivePassive,
            m(30),
            m(5),
            StrategyInput::default(),
        )
        .expect("valid");
        assert_eq!(strategy.gap_check(&[objective]).status, GapStatus::Meets);
        assert_eq!(strategy.gap_check(&[]).status, GapStatus::NoObjective);
    }
}
