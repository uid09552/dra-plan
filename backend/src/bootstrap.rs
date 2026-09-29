//! Composition root: the only place that knows the concrete adapters. Builds the use cases,
//! wires them into [`AppState`] and assembles the HTTP router.

use std::sync::Arc;

use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router, middleware};
use serde_json::json;

use crate::app::AppState;
use crate::features::action_items::application::ActionItemUseCases;
use crate::features::action_items::infra::PgActionItemRepository;
use crate::features::ai::application::AiUseCases;
use crate::features::ai::domain::{AiProvider, NoAiProvider};
use crate::features::ai::infra::PgAiSuggestionRepository;
use crate::features::audit::application::AuditUseCases;
use crate::features::audit::infra::PgAuditRepository;
use crate::features::bia::application::BiaUseCases;
use crate::features::bia::infra::PgBiaRepository;
use crate::features::data_protection::application::DataProtectionUseCases;
use crate::features::data_protection::infra::PgDataProtectionRepository;
use crate::features::dependencies::application::DependencyUseCases;
use crate::features::dependencies::infra::PgDependencyRepository;
use crate::features::directory::application::DirectoryUseCases;
use crate::features::directory::infra::{PgMemberRepository, PgPersonRepository, PgRoleRepository};
use crate::features::dr_tests::application::DrTestUseCases;
use crate::features::dr_tests::infra::PgDrTestRepository;
use crate::features::it_services::application::ItServiceUseCases;
use crate::features::it_services::infra::PgItServiceRepository;
use crate::features::microservices::application::MicroserviceUseCases;
use crate::features::microservices::infra::PgMicroserviceRepository;
use crate::features::objectives::application::ObjectiveUseCases;
use crate::features::objectives::infra::PgObjectiveRepository;
use crate::features::plans::api::JsonSnapshotCodec;
use crate::features::plans::application::{AggregateLoader, PlanUseCases};
use crate::features::plans::infra::{MarkdownPlanRenderer, PgPlanVersionRepository};
use crate::features::readiness::application::ReadinessUseCases;
use crate::features::recovery_runs::application::{EventBus, RecoveryRunUseCases};
use crate::features::recovery_runs::infra::PgRecoveryRunRepository;
use crate::features::roles_comm::application::RolesCommUseCases;
use crate::features::roles_comm::infra::{
    PgCommunicationRuleRepository, PgRoleAssignmentRepository,
};
use crate::features::runbooks::application::RunbookUseCases;
use crate::features::runbooks::infra::PgRunbookRepository;
use crate::features::scenarios::application::ScenarioUseCases;
use crate::features::scenarios::infra::PgScenarioRepository;
use crate::features::strategies::application::StrategyUseCases;
use crate::features::strategies::infra::PgStrategyRepository;
use crate::features::tenants::application::TenantUseCases;
use crate::features::tenants::infra::PgTenantRepository;
use crate::features::workflow::application::WorkflowUseCases;
use crate::features::workflow::infra::PgWorkflowRepository;
use crate::features::{
    action_items, ai, audit, bia, catalog, data_protection, dependencies, directory, dr_tests,
    it_services, microservices, objectives, plans, readiness, recovery_runs, roles_comm, runbooks,
    scenarios, strategies, tenants, workflow,
};
use crate::mcp::{self, McpConfig};
use crate::shared::auth::{
    self, Authenticated, Authenticator, DEV_USER, DevAuthenticator, JwtAuthenticator,
    StaticAuthenticator,
};
use crate::shared::infra::Db;
use crate::shared::web::API_BASE;
use crate::shared::web::layers::{self, HttpSettings};

/// How requests are authenticated.
#[derive(Debug, Clone)]
pub enum AuthMode {
    /// `--dev-mode`: fixed admin `dev-user`, default tenant created on startup.
    Dev { tenant_slug: String },
    /// Validate Keycloak access tokens and enforce their API roles.
    Jwt {
        issuer: String,
        jwks_url: String,
        audience: String,
    },
    /// Fixed identity (tests).
    Static(Authenticated),
}

/// Wires all adapters into the use cases.
pub fn build_state(db: Db) -> AppState {
    // Adapters (driven side).
    let tenant_repo = Arc::new(PgTenantRepository(db.clone()));
    let person_repo = Arc::new(PgPersonRepository(db.clone()));
    let role_repo = Arc::new(PgRoleRepository(db.clone()));
    let service_repo = Arc::new(PgItServiceRepository(db.clone()));
    let bia_repo = Arc::new(PgBiaRepository(db.clone()));
    let scenario_repo = Arc::new(PgScenarioRepository(db.clone()));
    let microservice_repo = Arc::new(PgMicroserviceRepository(db.clone()));
    let dependency_repo = Arc::new(PgDependencyRepository(db.clone()));
    let objective_repo = Arc::new(PgObjectiveRepository(db.clone()));
    let strategy_repo = Arc::new(PgStrategyRepository(db.clone()));
    let protection_repo = Arc::new(PgDataProtectionRepository(db.clone()));
    let runbook_repo = Arc::new(PgRunbookRepository(db.clone()));
    let assignment_repo = Arc::new(PgRoleAssignmentRepository(db.clone()));
    let rule_repo = Arc::new(PgCommunicationRuleRepository(db.clone()));
    let action_repo = Arc::new(PgActionItemRepository(db.clone()));
    let workflow_repo = Arc::new(PgWorkflowRepository(db.clone()));
    let dr_test_repo = Arc::new(PgDrTestRepository(db.clone()));
    let codec = Arc::new(JsonSnapshotCodec);
    let ai_provider: Arc<dyn AiProvider> = Arc::new(NoAiProvider);

    // Use cases.
    let services = Arc::new(ItServiceUseCases::new(service_repo.clone()));
    let microservices = Arc::new(MicroserviceUseCases::new(
        microservice_repo.clone(),
        services.clone(),
    ));
    let loader = Arc::new(AggregateLoader {
        services: service_repo.clone(),
        bia: bia_repo.clone(),
        scenarios: scenario_repo.clone(),
        microservices: microservice_repo.clone(),
        dependencies: dependency_repo.clone(),
        objectives: objective_repo.clone(),
        strategies: strategy_repo.clone(),
        data_protection: protection_repo.clone(),
        runbooks: runbook_repo.clone(),
        assignments: assignment_repo.clone(),
        communication_rules: rule_repo.clone(),
        roles: role_repo.clone(),
        persons: person_repo.clone(),
    });
    let plans = Arc::new(PlanUseCases::new(
        loader,
        Arc::new(PgPlanVersionRepository(db.clone())),
        codec.clone(),
        Arc::new(MarkdownPlanRenderer),
        tenant_repo.clone(),
        workflow_repo.clone(),
    ));
    let dr_tests = Arc::new(DrTestUseCases::new(
        dr_test_repo.clone(),
        plans.clone(),
        scenario_repo.clone(),
        microservice_repo.clone(),
        objective_repo.clone(),
    ));
    let workflow = Arc::new(WorkflowUseCases::new(
        workflow_repo,
        plans.clone(),
        dr_test_repo.clone(),
        action_repo.clone(),
    ));

    AppState {
        db: db.clone(),
        tenants: Arc::new(TenantUseCases::new(tenant_repo.clone())),
        directory: Arc::new(DirectoryUseCases::new(
            person_repo,
            role_repo,
            Arc::new(PgMemberRepository(db.clone())),
        )),
        bia: Arc::new(BiaUseCases::new(
            bia_repo.clone(),
            services.clone(),
            tenant_repo.clone(),
        )),
        scenarios: Arc::new(ScenarioUseCases::new(
            scenario_repo.clone(),
            services.clone(),
            microservice_repo.clone(),
            dependency_repo.clone(),
        )),
        dependencies: Arc::new(DependencyUseCases::new(
            dependency_repo,
            services.clone(),
            microservices.clone(),
            microservice_repo.clone(),
            objective_repo.clone(),
        )),
        objectives: Arc::new(ObjectiveUseCases::new(
            objective_repo.clone(),
            microservices.clone(),
            scenario_repo.clone(),
            bia_repo,
        )),
        strategies: Arc::new(StrategyUseCases::new(
            strategy_repo.clone(),
            microservices.clone(),
            scenario_repo.clone(),
            objective_repo.clone(),
        )),
        data_protection: Arc::new(DataProtectionUseCases::new(
            protection_repo,
            microservices.clone(),
            objective_repo.clone(),
        )),
        runbooks: Arc::new(RunbookUseCases::new(
            runbook_repo,
            services.clone(),
            microservices.clone(),
            scenario_repo,
            strategy_repo,
            objective_repo,
        )),
        roles_comm: Arc::new(RolesCommUseCases::new(
            assignment_repo,
            rule_repo,
            services.clone(),
        )),
        action_items: Arc::new(ActionItemUseCases::new(
            action_repo.clone(),
            services.clone(),
        )),
        readiness: Arc::new(ReadinessUseCases::new(workflow.clone(), service_repo)),
        workflow,
        recovery_runs: Arc::new(RecoveryRunUseCases::new(
            Arc::new(PgRecoveryRunRepository(db.clone())),
            plans.clone(),
            dr_tests.clone(),
            Arc::new(EventBus::new(1024)),
        )),
        ai: Arc::new(AiUseCases::new(
            Arc::new(PgAiSuggestionRepository(db.clone())),
            ai_provider,
            plans.clone(),
            codec,
            tenant_repo,
        )),
        audit: Arc::new(AuditUseCases::new(Arc::new(PgAuditRepository(db)))),
        dr_tests,
        plans,
        microservices,
        services,
    }
}

/// All feature routes (tenant-scoped and tenant-independent).
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(tenants::api::routes())
        .merge(directory::api::routes())
        .merge(catalog::api::routes())
        .merge(it_services::api::routes())
        .merge(bia::api::routes())
        .merge(scenarios::api::routes())
        .merge(microservices::api::routes())
        .merge(dependencies::api::routes())
        .merge(objectives::api::routes())
        .merge(strategies::api::routes())
        .merge(data_protection::api::routes())
        .merge(runbooks::api::routes())
        .merge(roles_comm::api::routes())
        .merge(workflow::api::routes())
        .merge(plans::api::routes())
        .merge(readiness::api::routes())
        .merge(dr_tests::api::routes())
        .merge(action_items::api::routes())
        .merge(ai::api::routes())
        .merge(recovery_runs::api::routes())
        .merge(audit::api::routes())
}

pub async fn build_router(
    db: Db,
    auth_mode: AuthMode,
    http: &HttpSettings,
) -> anyhow::Result<Router> {
    let state = build_state(db.clone());

    let authenticator: Arc<dyn Authenticator> = match auth_mode {
        AuthMode::Dev { tenant_slug } => {
            let tenant_id = state
                .tenants
                .ensure_dev_tenant(&tenant_slug, DEV_USER)
                .await?;
            tracing::warn!(tenant = %tenant_slug, "DEV MODE: authentication and authorization are mocked");
            Arc::new(DevAuthenticator {
                default_tenant: tenant_id,
                lookup: Arc::new(PgTenantRepository(db)),
            })
        }
        AuthMode::Jwt {
            issuer,
            jwks_url,
            audience,
        } => Arc::new(
            JwtAuthenticator::new(issuer, jwks_url, audience, Arc::new(PgTenantRepository(db)))
                .await
                .context("cannot initialize Keycloak JWT authentication")?,
        ),
        AuthMode::Static(identity) => Arc::new(StaticAuthenticator(identity)),
    };

    let auth_layer = middleware::from_fn_with_state(authenticator, auth::authenticate);
    let protected = api_routes().layer(auth_layer.clone());

    let api = Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state.clone());

    // MCP endpoint: same authentication; Origin allowlist = CORS origins (DNS-rebinding protection).
    let mcp = mcp::routes()
        .layer(auth_layer)
        .layer(Extension(McpConfig {
            allowed_origins: http.cors_origins.clone(),
        }))
        .with_state(state);

    Ok(layers::apply(
        Router::new().nest(API_BASE, api).merge(mcp),
        http,
    ))
}

async fn health(State(app): State<AppState>) -> impl IntoResponse {
    match app.db.ping().await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "status": "ok", "components": { "database": "ok" } })),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "degraded", "components": { "database": "down" } })),
        ),
    }
}
