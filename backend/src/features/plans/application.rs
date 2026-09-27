use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    AssignmentEntry, MicroserviceBundle, PlanAggregate, PlanRenderer, PlanStatus, PlanVersion,
    PlanVersionRepository, RunbookWithSteps, SnapshotCodec,
};
use crate::features::bia::domain::BiaRepository;
use crate::features::data_protection::domain::DataProtectionRepository;
use crate::features::dependencies::domain::DependencyRepository;
use crate::features::directory::domain::{PersonRepository, RoleRepository};
use crate::features::it_services::domain::ItServiceRepository;
use crate::features::microservices::domain::MicroserviceRepository;
use crate::features::objectives::domain::ObjectiveRepository;
use crate::features::roles_comm::domain::{CommunicationRuleRepository, RoleAssignmentRepository};
use crate::features::runbooks::domain::RunbookRepository;
use crate::features::scenarios::domain::{ScenarioFilter, ScenarioRepository};
use crate::features::strategies::domain::StrategyRepository;
use crate::features::tenants::domain::TenantRepository;
use crate::features::workflow::domain::{
    StepProgress, WorkflowRepository, WorkflowStepKey, WorkflowStepStatus, design_issues,
};
use crate::shared::kernel::{AppError, AppResult, Language, TenantContext};

/// Loads the full IT service subtree through the other features' ports.
pub struct AggregateLoader {
    pub services: Arc<dyn ItServiceRepository>,
    pub bia: Arc<dyn BiaRepository>,
    pub scenarios: Arc<dyn ScenarioRepository>,
    pub microservices: Arc<dyn MicroserviceRepository>,
    pub dependencies: Arc<dyn DependencyRepository>,
    pub objectives: Arc<dyn ObjectiveRepository>,
    pub strategies: Arc<dyn StrategyRepository>,
    pub data_protection: Arc<dyn DataProtectionRepository>,
    pub runbooks: Arc<dyn RunbookRepository>,
    pub assignments: Arc<dyn RoleAssignmentRepository>,
    pub communication_rules: Arc<dyn CommunicationRuleRepository>,
    pub roles: Arc<dyn RoleRepository>,
    pub persons: Arc<dyn PersonRepository>,
}

fn group<T>(items: Vec<T>, key: impl Fn(&T) -> Uuid) -> HashMap<Uuid, Vec<T>> {
    let mut map: HashMap<Uuid, Vec<T>> = HashMap::new();
    for item in items {
        map.entry(key(&item)).or_default().push(item);
    }
    map
}

impl AggregateLoader {
    /// Ensures the IT service exists in the caller's tenant (404 otherwise).
    pub async fn require_service(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<()> {
        self.services
            .get(ctx, service_id)
            .await?
            .ok_or(AppError::NotFound("IT service"))?;
        Ok(())
    }

    pub async fn load(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<PlanAggregate> {
        let service = self
            .services
            .get(ctx, service_id)
            .await?
            .ok_or(AppError::NotFound("IT service"))?;
        let bia = self.bia.get(ctx, service_id).await?;
        let scenarios = self
            .scenarios
            .list_by_service(ctx, service_id, &ScenarioFilter::default())
            .await?;
        let microservices = self.microservices.list_by_service(ctx, service_id).await?;

        let mut deps = group(
            self.dependencies.list_by_service(ctx, service_id).await?,
            |d| d.microservice_id,
        );
        let mut objectives = group(
            self.objectives.list_by_service(ctx, service_id).await?,
            |o| o.microservice_id,
        );
        let mut strategies = group(
            self.strategies.list_by_service(ctx, service_id).await?,
            |s| s.microservice_id,
        );
        let mut protection = group(
            self.data_protection
                .list_by_service(ctx, service_id)
                .await?,
            |d| d.microservice_id,
        );
        let runbooks = self.runbooks.list_by_service(ctx, service_id, None).await?;
        let runbook_ids: Vec<Uuid> = runbooks.iter().map(|r| r.meta.id).collect();
        let mut steps = group(self.runbooks.steps(ctx, &runbook_ids).await?, |s| {
            s.runbook_id
        });
        let mut runbooks_by_ms: HashMap<Uuid, Vec<RunbookWithSteps>> = HashMap::new();
        for runbook in runbooks {
            let steps = steps.remove(&runbook.meta.id).unwrap_or_default();
            runbooks_by_ms
                .entry(runbook.microservice_id)
                .or_default()
                .push(RunbookWithSteps { runbook, steps });
        }

        let bundles = microservices
            .into_iter()
            .map(|m| {
                let id = m.meta.id;
                MicroserviceBundle {
                    microservice: m,
                    dependencies: deps.remove(&id).unwrap_or_default(),
                    objectives: objectives.remove(&id).unwrap_or_default(),
                    strategies: strategies.remove(&id).unwrap_or_default(),
                    data_protection: protection.remove(&id).unwrap_or_default(),
                    runbooks: runbooks_by_ms.remove(&id).unwrap_or_default(),
                }
            })
            .collect();

        let roles = self.roles.list(ctx).await?;
        let assignments = self.assignments.list_by_service(ctx, service_id).await?;
        let mut person_ids: HashSet<Uuid> = assignments.iter().map(|a| a.person_id).collect();
        person_ids.extend(service.business_owner_id);
        person_ids.extend(service.technical_owner_id);
        let persons = self
            .persons
            .get_many(ctx, &person_ids.into_iter().collect::<Vec<_>>())
            .await?;
        let role_assignments = assignments
            .into_iter()
            .map(|a| AssignmentEntry {
                role_name: roles
                    .iter()
                    .find(|r| r.meta.id == a.role_id)
                    .map(|r| r.name.clone())
                    .unwrap_or_default(),
                person: persons.iter().find(|p| p.meta.id == a.person_id).cloned(),
                assignment: a,
            })
            .collect();

        Ok(PlanAggregate {
            service,
            bia,
            scenarios,
            microservices: bundles,
            role_assignments,
            communication_rules: self
                .communication_rules
                .list_by_service(ctx, service_id)
                .await?,
            roles,
            persons,
        })
    }
}

pub struct PlanUseCases {
    loader: Arc<AggregateLoader>,
    versions: Arc<dyn PlanVersionRepository>,
    codec: Arc<dyn SnapshotCodec>,
    renderer: Arc<dyn PlanRenderer>,
    tenants: Arc<dyn TenantRepository>,
    workflow: Arc<dyn WorkflowRepository>,
}

pub enum Export {
    Markdown(String),
    Json(PlanVersion),
}

impl PlanUseCases {
    pub fn new(
        loader: Arc<AggregateLoader>,
        versions: Arc<dyn PlanVersionRepository>,
        codec: Arc<dyn SnapshotCodec>,
        renderer: Arc<dyn PlanRenderer>,
        tenants: Arc<dyn TenantRepository>,
        workflow: Arc<dyn WorkflowRepository>,
    ) -> Self {
        Self {
            loader,
            versions,
            codec,
            renderer,
            tenants,
            workflow,
        }
    }

    pub async fn list(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        status: Option<PlanStatus>,
    ) -> AppResult<Vec<PlanVersion>> {
        self.loader.require_service(ctx, service_id).await?;
        self.versions.list_by_service(ctx, service_id, status).await
    }

    pub async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<PlanVersion> {
        self.versions
            .get(ctx, id, true)
            .await?
            .ok_or(AppError::NotFound("plan version"))
    }

    /// Snapshots the current subtree and submits it for review. All design gates (steps 1–11)
    /// must be free of blocking issues.
    pub async fn submit(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        comment: Option<String>,
    ) -> AppResult<PlanVersion> {
        let aggregate = self.loader.load(ctx, service_id).await?;
        let blocking: Vec<_> = design_issues(&aggregate)
            .into_iter()
            .filter(|i| i.is_blocking())
            .collect();
        if !blocking.is_empty() {
            return Err(AppError::Validation(blocking));
        }
        if !self
            .versions
            .list_by_service(ctx, service_id, Some(PlanStatus::InReview))
            .await?
            .is_empty()
        {
            return Err(AppError::conflict(
                "another plan version is already in review",
            ));
        }
        let snapshot = self.codec.encode(&aggregate)?;
        let number = self.versions.next_plan_number(ctx, service_id).await?;
        let version = PlanVersion::submit(ctx, service_id, number, snapshot, comment);
        self.versions.insert(ctx, &version).await?;
        self.get(ctx, version.id).await
    }

    pub async fn approve(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        comment: Option<String>,
        next_review_due: Option<chrono::NaiveDate>,
    ) -> AppResult<PlanVersion> {
        let mut version = self.get(ctx, id).await?;
        let tenant = self
            .tenants
            .get(ctx)
            .await?
            .ok_or(AppError::NotFound("tenant"))?;
        version.approve(
            ctx.actor(),
            comment,
            next_review_due,
            tenant.settings.review_interval_months,
        )?;
        self.versions.save_status(ctx, &version).await?;
        if let Some(service_id) = version.service_id {
            self.workflow
                .save(
                    ctx,
                    service_id,
                    &[StepProgress::completed(
                        WorkflowStepKey::ReviewApprove,
                        ctx.actor(),
                        Some("plan approved".into()),
                    )],
                )
                .await?;
        }
        self.get(ctx, id).await
    }

    pub async fn reject(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        comment: String,
    ) -> AppResult<PlanVersion> {
        let mut version = self.get(ctx, id).await?;
        version.reject(comment)?;
        self.versions.save_status(ctx, &version).await?;
        if let Some(service_id) = version.service_id {
            let step = StepProgress {
                status: WorkflowStepStatus::NeedsReview,
                ..StepProgress::new(WorkflowStepKey::ReviewApprove)
            };
            self.workflow.save(ctx, service_id, &[step]).await?;
        }
        self.get(ctx, id).await
    }

    pub async fn export(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        markdown: bool,
        lang: Language,
    ) -> AppResult<Export> {
        let version = self.get(ctx, id).await?;
        if !markdown {
            return Ok(Export::Json(version));
        }
        let aggregate = self.decode(&version)?;
        Ok(Export::Markdown(self.renderer.markdown(
            Some(&version),
            &aggregate,
            lang,
        )))
    }

    /// The emergency handbook rendered from the current working data (not approved).
    pub async fn draft_handbook(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        lang: Language,
    ) -> AppResult<String> {
        let aggregate = self.loader.load(ctx, service_id).await?;
        Ok(self.renderer.markdown(None, &aggregate, lang))
    }

    /// Decodes the pinned snapshot of a plan version.
    pub fn decode(&self, version: &PlanVersion) -> AppResult<PlanAggregate> {
        let snapshot = version
            .snapshot
            .as_deref()
            .ok_or_else(|| AppError::internal("snapshot not loaded"))?;
        self.codec.decode(snapshot)
    }

    pub async fn current_approved(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Option<PlanVersion>> {
        match self.versions.current_approved(ctx, service_id).await? {
            Some(v) => Ok(Some(self.get(ctx, v.id).await?)),
            None => Ok(None),
        }
    }

    pub fn loader(&self) -> &AggregateLoader {
        &self.loader
    }
}
