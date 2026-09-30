//! Shared application state handed to all axum handlers.

use std::sync::Arc;

use crate::features::action_items::application::ActionItemUseCases;
use crate::features::ai::application::AiUseCases;
use crate::features::audit::application::AuditUseCases;
use crate::features::bia::application::BiaUseCases;
use crate::features::categories::application::CategoryUseCases;
use crate::features::data_protection::application::DataProtectionUseCases;
use crate::features::dependencies::application::DependencyUseCases;
use crate::features::directory::application::DirectoryUseCases;
use crate::features::dr_tests::application::DrTestUseCases;
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::objectives::application::ObjectiveUseCases;
use crate::features::plans::application::PlanUseCases;
use crate::features::readiness::application::ReadinessUseCases;
use crate::features::recovery_runs::application::RecoveryRunUseCases;
use crate::features::roles_comm::application::RolesCommUseCases;
use crate::features::runbooks::application::RunbookUseCases;
use crate::features::scenarios::application::ScenarioUseCases;
use crate::features::strategies::application::StrategyUseCases;
use crate::features::tenants::application::TenantUseCases;
use crate::features::workflow::application::WorkflowUseCases;
use crate::shared::infra::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub tenants: Arc<TenantUseCases>,
    pub directory: Arc<DirectoryUseCases>,
    pub services: Arc<ItServiceUseCases>,
    pub bia: Arc<BiaUseCases>,
    pub categories: Arc<CategoryUseCases>,
    pub scenarios: Arc<ScenarioUseCases>,
    pub microservices: Arc<MicroserviceUseCases>,
    pub dependencies: Arc<DependencyUseCases>,
    pub objectives: Arc<ObjectiveUseCases>,
    pub strategies: Arc<StrategyUseCases>,
    pub data_protection: Arc<DataProtectionUseCases>,
    pub runbooks: Arc<RunbookUseCases>,
    pub roles_comm: Arc<RolesCommUseCases>,
    pub action_items: Arc<ActionItemUseCases>,
    pub plans: Arc<PlanUseCases>,
    pub workflow: Arc<WorkflowUseCases>,
    pub readiness: Arc<ReadinessUseCases>,
    pub dr_tests: Arc<DrTestUseCases>,
    pub recovery_runs: Arc<RecoveryRunUseCases>,
    pub ai: Arc<AiUseCases>,
    pub audit: Arc<AuditUseCases>,
}
