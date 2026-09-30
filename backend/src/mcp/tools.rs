//! Use-case based MCP tools. Each tool covers a step of the DR workflow (often several REST calls)
//! and delegates to the application use cases.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::app::AppState;
use crate::features::bia::domain::{BiaInput, ImpactRating};
use crate::features::catalog::domain::{SCENARIO_TEMPLATES, STRATEGY_TYPES};
use crate::features::dependencies::domain::DependencyInput;
use crate::features::directory::domain::Role;
use crate::features::it_services::api::ItServiceDto;
use crate::features::it_services::domain::{ItServiceInput, ServiceFilter};
use crate::features::microservices::domain::MicroserviceInput;
use crate::features::objectives::domain::ObjectiveInput;
use crate::features::plans::application::Export;
use crate::features::readiness::api::{ServiceReadinessDto, TenantReadinessDto};
use crate::features::recovery_runs::api::{RecoveryRunDto, RunStepStateDto};
use crate::features::recovery_runs::application::{Closing, Declaration};
use crate::features::recovery_runs::domain::{RunStatus, RunStepStatus, Transition};
use crate::features::runbooks::application::NewStep;
use crate::features::runbooks::domain::{RunbookInput, StepInput};
use crate::features::scenarios::domain::{ScenarioDecision, ScenarioInput};
use crate::features::strategies::domain::StrategyInput;
use crate::features::workflow::domain::STEP_DEFINITIONS;
use crate::shared::kernel::{
    AppError, AppResult, Language, Minutes, PageRequest, Rating, TenantContext, parse_enum,
};

pub const INSTRUCTIONS: &str = "Tools for IT service disaster recovery planning (NIST SP 800-34, BSI 200-4). \
Typical flow: create_service → add_dependencies → record_business_impact → add_scenarios → decide_scenario → \
set_recovery_objective → add_mitigation (strategy + runbook steps) → assign_roles/communication via the UI → \
complete_workflow_step for each step → submit_plan → approve_plan. During a disaster: declare_recovery → \
get_recovery_status → update_recovery_step … → set_recovery_phase(recovered) → close_recovery. \
Call get_readiness for the overall DR readiness and next actions, suggest_scenarios for brainstorming \
help and get_service_overview to see open gates (blocking issues) and the next workflow step. Components \
(microservices) are optional: every service has a default component (isDefault) that stands for the whole service. \
Durations are minutes.";

pub enum ToolOutput {
    Json(Value),
    Text(String),
}

struct ToolDef {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    read_only: bool,
}

const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "get_catalog",
        title: "Get catalog",
        description: "Scenario templates, recovery strategy types and the 15 workflow steps.",
        schema: schema_catalog,
        read_only: true,
    },
    ToolDef {
        name: "list_services",
        title: "List IT services",
        description: "IT services with DR readiness summary (workflow completion, plan status, open actions).",
        schema: schema_list_services,
        read_only: true,
    },
    ToolDef {
        name: "get_service_overview",
        title: "Service overview",
        description: "Service, components (microservices), scenarios and the workflow with blocking issues per step; shows what to do next.",
        schema: schema_service_id,
        read_only: true,
    },
    ToolDef {
        name: "get_readiness",
        title: "DR readiness",
        description: "How ready are we? Readiness score, critical gaps, next actions, last exercise, RTO compliance, plans requiring review and untested scenarios; for one service or the whole tenant.",
        schema: schema_readiness,
        read_only: true,
    },
    ToolDef {
        name: "suggest_scenarios",
        title: "Suggest scenarios",
        description: "Catalog scenarios relevant for the service (based on data stores, hosting, dependencies and protection requirement) that are not captured yet. Add them with add_scenarios and catalogTemplateId.",
        schema: schema_suggest,
        read_only: true,
    },
    ToolDef {
        name: "create_service",
        title: "Create IT service",
        description: "Create an IT service, optionally with components (microservices). Every service has a default component that stands for the whole service (workflow step 1).",
        schema: schema_create_service,
        read_only: false,
    },
    ToolDef {
        name: "add_dependencies",
        title: "Add dependencies",
        description: "Record dependencies of a component (microservice; use the default component for the whole service) (workflow step 2).",
        schema: schema_add_dependencies,
        read_only: false,
    },
    ToolDef {
        name: "record_business_impact",
        title: "Record BIA",
        description: "Create or replace the business impact analysis: MTPD, service RTO/RPO, minimum operating level (workflow step 3).",
        schema: schema_bia,
        read_only: false,
    },
    ToolDef {
        name: "add_scenarios",
        title: "Add scenarios",
        description: "Add brainstormed disaster scenarios, optionally from catalog templates (workflow step 4).",
        schema: schema_add_scenarios,
        read_only: false,
    },
    ToolDef {
        name: "decide_scenario",
        title: "Decide scenario",
        description: "Select or reject a scenario with risk rating and rationale (workflow step 6).",
        schema: schema_decide,
        read_only: false,
    },
    ToolDef {
        name: "set_recovery_objective",
        title: "Set recovery objective",
        description: "RTO/RPO of a component (microservice), as default or for one scenario (workflow step 7).",
        schema: schema_objective,
        read_only: false,
    },
    ToolDef {
        name: "add_mitigation",
        title: "Add mitigation",
        description: "Add and select a recovery strategy for a component (microservice) and scenario and create its runbook with ordered steps (workflow steps 8–9). Roles can be given by name.",
        schema: schema_mitigation,
        read_only: false,
    },
    ToolDef {
        name: "complete_workflow_step",
        title: "Complete workflow step",
        description: "Complete a workflow step; fails with the blocking issues if its gate is not met.",
        schema: schema_complete_step,
        read_only: false,
    },
    ToolDef {
        name: "submit_plan",
        title: "Submit plan",
        description: "Snapshot the service and submit the DR plan for review (workflow step 12).",
        schema: schema_submit,
        read_only: false,
    },
    ToolDef {
        name: "approve_plan",
        title: "Approve plan",
        description: "Approve a plan version in review.",
        schema: schema_approve,
        read_only: false,
    },
    ToolDef {
        name: "export_plan",
        title: "Export plan",
        description: "Emergency handbook of a plan version as Markdown.",
        schema: schema_export,
        read_only: true,
    },
    ToolDef {
        name: "declare_recovery",
        title: "Declare recovery",
        description: "Declare DR for a scenario and start a recovery run against the approved plan.",
        schema: schema_declare,
        read_only: false,
    },
    ToolDef {
        name: "get_recovery_status",
        title: "Recovery status",
        description: "RTO clock, progress, and the steps that can be worked on now.",
        schema: schema_run_id,
        read_only: true,
    },
    ToolDef {
        name: "update_recovery_step",
        title: "Update recovery step",
        description: "Start, complete (verification required), skip or fail a runbook step during a run.",
        schema: schema_update_step,
        read_only: false,
    },
    ToolDef {
        name: "set_recovery_phase",
        title: "Set recovery phase",
        description: "Mark a run recovered, reconstituting, or aborted.",
        schema: schema_phase,
        read_only: false,
    },
    ToolDef {
        name: "close_recovery",
        title: "Close recovery",
        description: "Close a run with outcome and lessons learned (become action items).",
        schema: schema_close,
        read_only: false,
    },
];

pub fn exists(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

pub fn definitions() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": (t.schema)(),
                "annotations": { "readOnlyHint": t.read_only, "destructiveHint": false, "openWorldHint": false },
            })
        })
        .collect()
}

pub async fn call(
    app: &AppState,
    ctx: &TenantContext,
    name: &str,
    args: Value,
) -> AppResult<ToolOutput> {
    match name {
        "get_catalog" => get_catalog(args),
        "list_services" => list_services(app, ctx, args).await,
        "get_service_overview" => service_overview(app, ctx, args).await,
        "get_readiness" => readiness(app, ctx, args).await,
        "suggest_scenarios" => suggest_scenarios(app, ctx, args).await,
        "create_service" => create_service(app, ctx, args).await,
        "add_dependencies" => add_dependencies(app, ctx, args).await,
        "record_business_impact" => record_bia(app, ctx, args).await,
        "add_scenarios" => add_scenarios(app, ctx, args).await,
        "decide_scenario" => decide_scenario(app, ctx, args).await,
        "set_recovery_objective" => set_objective(app, ctx, args).await,
        "add_mitigation" => add_mitigation(app, ctx, args).await,
        "complete_workflow_step" => complete_step(app, ctx, args).await,
        "submit_plan" => submit_plan(app, ctx, args).await,
        "approve_plan" => approve_plan(app, ctx, args).await,
        "export_plan" => export_plan(app, ctx, args).await,
        "declare_recovery" => declare_recovery(app, ctx, args).await,
        "get_recovery_status" => recovery_status(app, ctx, args).await,
        "update_recovery_step" => update_step(app, ctx, args).await,
        "set_recovery_phase" => set_phase(app, ctx, args).await,
        "close_recovery" => close_recovery(app, ctx, args).await,
        other => Err(AppError::invalid(
            "UNKNOWN_TOOL",
            None,
            format!("unknown tool `{other}`"),
        )),
    }
}

// ───────────────────────────── helpers ─────────────────────────────

fn args<T: DeserializeOwned>(value: Value) -> AppResult<T> {
    serde_json::from_value(value)
        .map_err(|e| AppError::invalid("INVALID_ARGUMENTS", None, e.to_string()))
}

fn minutes(v: u32) -> AppResult<Minutes> {
    Minutes::new(v)
}

fn opt_minutes(v: Option<u32>) -> AppResult<Option<Minutes>> {
    v.map(Minutes::new).transpose()
}

fn rating(v: Option<i64>) -> AppResult<Option<Rating>> {
    v.map(Rating::new).transpose()
}

fn enum_opt<T: std::str::FromStr<Err = crate::shared::kernel::ParseEnumError>>(
    v: Option<&str>,
    field: &str,
) -> AppResult<Option<T>> {
    v.map(|s| parse_enum(s, field)).transpose()
}

/// Resolves a role given by id or (case-insensitive) name.
fn resolve_role(roles: &[Role], value: Option<&str>) -> AppResult<Option<Uuid>> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    roles
        .iter()
        .find(|r| r.meta.id.to_string() == v || r.name.eq_ignore_ascii_case(v))
        .map(|r| Some(r.meta.id))
        .ok_or_else(|| {
            let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
            AppError::invalid(
                "UNKNOWN_ROLE",
                Some("ownerRole"),
                format!("unknown role `{v}`; known roles: {}", names.join(", ")),
            )
        })
}

fn json_of<T: serde::Serialize>(value: T) -> AppResult<ToolOutput> {
    serde_json::to_value(value)
        .map(ToolOutput::Json)
        .map_err(AppError::internal)
}

// ───────────────────────────── tools ─────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LanguageArgs {
    language: Option<String>,
}

fn get_catalog(value: Value) -> AppResult<ToolOutput> {
    let a: LanguageArgs = args(value)?;
    let lang = enum_opt::<Language>(a.language.as_deref(), "language")?.unwrap_or(Language::En);
    json_of(json!({
        "scenarioTemplates": SCENARIO_TEMPLATES.iter().map(|t| json!({ "id": t.id, "category": t.category.to_string(), "title": t.title(lang) })).collect::<Vec<_>>(),
        "strategyTypes": STRATEGY_TYPES.iter().map(|s| json!({ "key": s.key.to_string(), "label": s.label.get(lang), "typicalRto": s.typical_rto.get(lang), "typicalRpo": s.typical_rpo.get(lang) })).collect::<Vec<_>>(),
        "workflowSteps": STEP_DEFINITIONS.iter().map(|d| json!({ "key": d.key.to_string(), "number": d.number, "title": d.title.get(lang) })).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListServicesArgs {
    query: Option<String>,
}

async fn list_services(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: ListServicesArgs = args(value)?;
    let filter = ServiceFilter {
        query: a.query,
        ..Default::default()
    };
    let page = app
        .services
        .list(
            ctx,
            &filter,
            PageRequest::parse(None, Some(PageRequest::MAX_LIMIT))?,
        )
        .await?;
    json_of(
        page.items
            .into_iter()
            .map(|(s, sum)| ItServiceDto::new(s, Some(sum)))
            .collect::<Vec<_>>(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceIdArgs {
    service_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadinessArgs {
    service_id: Option<Uuid>,
}

async fn readiness(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: ReadinessArgs = args(value)?;
    match a.service_id {
        Some(id) => json_of(ServiceReadinessDto::from(
            app.readiness.service(ctx, id).await?,
        )),
        None => json_of(TenantReadinessDto::from(app.readiness.tenant(ctx).await?)),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SuggestArgs {
    service_id: Uuid,
    language: Option<String>,
}

async fn suggest_scenarios(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: SuggestArgs = args(value)?;
    let lang = enum_opt::<Language>(a.language.as_deref(), "language")?.unwrap_or(Language::En);
    let templates = app.scenarios.suggestions(ctx, a.service_id).await?;
    json_of(
        templates
            .into_iter()
            .map(|t| json!({
                "catalogTemplateId": t.id, "category": t.category.to_string(), "title": t.title(lang),
                "description": t.description(lang), "relevance": t.relevance.as_str(),
            }))
            .collect::<Vec<_>>(),
    )
}

async fn service_overview(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: ServiceIdArgs = args(value)?;
    let (service, summary) = app.services.get_with_summary(ctx, a.service_id).await?;
    let workflow = app.workflow.state(ctx, a.service_id).await?;
    let aggregate = app.plans.loader().load(ctx, a.service_id).await?;
    json_of(json!({
        "service": ItServiceDto::new(service, Some(summary)),
        "microservices": aggregate.microservices.iter().map(|b| json!({
            "id": b.microservice.meta.id, "name": b.microservice.name, "isDefault": b.microservice.is_default, "restoreOrder": b.microservice.restore_order,
            "objectives": b.objectives.len(), "strategies": b.strategies.len(), "runbooks": b.runbooks.len(),
        })).collect::<Vec<_>>(),
        "scenarios": aggregate.scenarios.iter().map(|s| json!({
            "id": s.meta.id, "title": s.title, "status": s.status.to_string(),
            "category": s.category.clone(), "drRequired": s.dr_required.map(|d| d.to_string()),
        })).collect::<Vec<_>>(),
        "workflow": {
            "completionPercent": workflow.completion_percent,
            "currentStep": workflow.current_step.map(|k| k.to_string()),
            "steps": workflow.steps.iter().map(|s| json!({
                "key": s.definition.key.to_string(), "number": s.definition.number, "status": s.status.to_string(),
                "gatePassed": s.gate_passed,
                "issues": s.issues.iter().map(|i| format!("[{}] {} {}", i.severity, i.rule_id, i.message)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        },
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MicroserviceArgs {
    name: String,
    description: Option<String>,
    platform: Option<String>,
    hosting_location: Option<String>,
    data_stores: Option<Vec<String>>,
    restore_order: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateServiceArgs {
    name: String,
    description: Option<String>,
    business_owner_id: Option<Uuid>,
    technical_owner_id: Option<Uuid>,
    protection_requirement_availability: Option<String>,
    impact_level: Option<String>,
    #[serde(default)]
    microservices: Vec<MicroserviceArgs>,
}

async fn create_service(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: CreateServiceArgs = args(value)?;
    let service = app
        .services
        .create(
            ctx,
            ItServiceInput {
                name: Some(a.name),
                description: a.description,
                business_owner_id: Some(a.business_owner_id),
                technical_owner_id: Some(a.technical_owner_id),
                protection_requirement_availability: enum_opt(
                    a.protection_requirement_availability.as_deref(),
                    "protectionRequirementAvailability",
                )?,
                impact_level: enum_opt(a.impact_level.as_deref(), "impactLevel")?,
                ..Default::default()
            },
        )
        .await?;
    let mut created = Vec::new();
    for m in a.microservices {
        let ms = app
            .microservices
            .create(
                ctx,
                service.meta.id,
                MicroserviceInput {
                    name: Some(m.name),
                    description: m.description,
                    platform: m.platform,
                    hosting_location: m.hosting_location,
                    data_stores: m.data_stores,
                    restore_order: m.restore_order,
                    ..Default::default()
                },
            )
            .await?;
        created.push(json!({ "id": ms.meta.id, "name": ms.name }));
    }
    json_of(json!({ "serviceId": service.meta.id, "name": service.name, "microservices": created }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DependencyArgs {
    kind: String,
    target_microservice_id: Option<Uuid>,
    target_name: Option<String>,
    #[serde(default = "upstream")]
    direction: String,
    criticality: String,
    dependency_rto_minutes: Option<u32>,
}

fn upstream() -> String {
    "upstream".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AddDependenciesArgs {
    microservice_id: Uuid,
    dependencies: Vec<DependencyArgs>,
}

async fn add_dependencies(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: AddDependenciesArgs = args(value)?;
    let mut created = Vec::new();
    for d in a.dependencies {
        let view = app
            .dependencies
            .create(
                ctx,
                a.microservice_id,
                parse_enum(&d.kind, "kind")?,
                parse_enum(&d.direction, "direction")?,
                parse_enum(&d.criticality, "criticality")?,
                DependencyInput {
                    target_microservice_id: Some(d.target_microservice_id),
                    target_name: d.target_name,
                    dependency_rto: Some(opt_minutes(d.dependency_rto_minutes)?),
                    ..Default::default()
                },
            )
            .await?;
        created.push(json!({ "id": view.dependency.meta.id, "rtoConflict": view.rto_conflict }));
    }
    json_of(json!({ "created": created }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RatingArgs {
    impact_category: String,
    time_window_minutes: u32,
    level: i64,
    rationale: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BiaArgs {
    service_id: Uuid,
    mtpd_minutes: u32,
    service_rto_minutes: u32,
    service_rpo_minutes: u32,
    minimum_operating_level: Option<String>,
    regulatory_requirements: Option<String>,
    #[serde(default)]
    impact_ratings: Vec<RatingArgs>,
}

async fn record_bia(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: BiaArgs = args(value)?;
    let ratings = a
        .impact_ratings
        .into_iter()
        .map(|r| {
            Ok(ImpactRating {
                category: parse_enum(&r.impact_category, "impactCategory")?,
                time_window: minutes(r.time_window_minutes)?,
                level: Rating::new(r.level)?,
                rationale: r.rationale,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let input = BiaInput {
        mtpd: minutes(a.mtpd_minutes)?,
        service_rto: minutes(a.service_rto_minutes)?,
        service_rpo: minutes(a.service_rpo_minutes)?,
        minimum_operating_level: a.minimum_operating_level,
        regulatory_requirements: a.regulatory_requirements,
        impact_ratings: ratings,
    };
    let (bia, derived) = app.bia.put(ctx, a.service_id, None, input).await?;
    json_of(json!({
        "serviceId": bia.service_id, "mtpdMinutes": bia.mtpd.get(), "serviceRtoMinutes": bia.service_rto.get(),
        "serviceRpoMinutes": bia.service_rpo.get(), "derivedMtpdMinutes": derived.map(Minutes::get),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScenarioArgs {
    title: Option<String>,
    parent_scenario_id: Option<Uuid>,
    description: Option<String>,
    category: Option<String>,
    catalog_template_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AddScenariosArgs {
    service_id: Uuid,
    scenarios: Vec<ScenarioArgs>,
}

async fn add_scenarios(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: AddScenariosArgs = args(value)?;
    let mut created = Vec::new();
    for s in a.scenarios {
        let input = ScenarioInput {
            title: s.title,
            description: s.description,
            category: s.category,
            parent_scenario_id: Some(s.parent_scenario_id),
            ..Default::default()
        };
        let scenario = app
            .scenarios
            .create(ctx, a.service_id, input, s.catalog_template_id.as_deref())
            .await?;
        created.push(json!({ "id": scenario.meta.id, "title": scenario.title, "category": scenario.category }));
    }
    json_of(json!({ "created": created }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DecideArgs {
    scenario_id: Uuid,
    decision: String,
    rationale: String,
    dr_required: Option<String>,
    likelihood: Option<i64>,
    impact: Option<i64>,
    priority: Option<String>,
    affected_microservice_ids: Option<Vec<Uuid>>,
}

async fn decide_scenario(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: DecideArgs = args(value)?;
    let decision = ScenarioDecision {
        decision: parse_enum(&a.decision, "decision")?,
        likelihood: rating(a.likelihood)?,
        impact: rating(a.impact)?,
        priority: enum_opt(a.priority.as_deref(), "priority")?,
        dr_required: enum_opt(a.dr_required.as_deref(), "drRequired")?,
        rationale: a.rationale,
        affected_microservice_ids: a.affected_microservice_ids,
    };
    let s = app.scenarios.decide(ctx, a.scenario_id, decision).await?;
    json_of(
        json!({ "id": s.meta.id, "status": s.status.to_string(), "riskScore": s.risk_score(), "drRequired": s.dr_required.map(|d| d.to_string()) }),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectiveArgs {
    microservice_id: Uuid,
    scenario_id: Option<Uuid>,
    rto_minutes: u32,
    rpo_minutes: u32,
    mttr_target_minutes: Option<u32>,
    #[serde(default)]
    first_functions: Vec<String>,
}

async fn set_objective(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: ObjectiveArgs = args(value)?;
    let input = ObjectiveInput {
        scenario_id: Some(a.scenario_id),
        mttr_target: Some(opt_minutes(a.mttr_target_minutes)?),
        first_functions: Some(a.first_functions),
        ..Default::default()
    };
    let o = app
        .objectives
        .create(
            ctx,
            a.microservice_id,
            minutes(a.rto_minutes)?,
            minutes(a.rpo_minutes)?,
            input,
        )
        .await?;
    json_of(
        json!({ "id": o.meta.id, "rtoMinutes": o.rto.get(), "rpoMinutes": o.rpo.get(), "scenarioId": o.scenario_id }),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrategyArgs {
    #[serde(rename = "type")]
    strategy_type: String,
    title: Option<String>,
    description: Option<String>,
    estimated_rto_minutes: u32,
    estimated_rpo_minutes: u32,
    #[serde(default)]
    prerequisites: Vec<String>,
    implementation_status: Option<String>,
    last_tested_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StepArgs {
    phase: String,
    title: String,
    instructions: Option<String>,
    owner_role: Option<String>,
    expected_duration_minutes: Option<u32>,
    verification: Option<String>,
    #[serde(default)]
    is_decision_point: bool,
    authorization_role: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunbookArgs {
    title: String,
    description: Option<String>,
    steps: Vec<StepArgs>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MitigationArgs {
    microservice_id: Uuid,
    scenario_id: Uuid,
    /// Further scenarios the same measure covers (the runbook belongs to `scenario_id`).
    #[serde(default)]
    also_covers_scenario_ids: Vec<Uuid>,
    strategy: StrategyArgs,
    #[serde(default)]
    accept_gap: bool,
    gap_justification: Option<String>,
    runbook: RunbookArgs,
}

/// Strategy + selection + runbook with steps in one use case ("mitigation as steps").
async fn add_mitigation(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: MitigationArgs = args(value)?;
    let roles = app.directory.list_roles(ctx).await?;
    let steps = a
        .runbook
        .steps
        .into_iter()
        .map(|s| {
            Ok(NewStep {
                phase: parse_enum(&s.phase, "phase")?,
                position: None,
                input: StepInput {
                    title: Some(s.title),
                    instructions: s.instructions,
                    owner_role_id: Some(resolve_role(&roles, s.owner_role.as_deref())?),
                    expected_duration: Some(opt_minutes(s.expected_duration_minutes)?),
                    verification: s.verification,
                    is_decision_point: Some(s.is_decision_point),
                    requires_authorization_role_id: Some(resolve_role(
                        &roles,
                        s.authorization_role.as_deref(),
                    )?),
                    ..Default::default()
                },
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let (strategy, _) = app
        .strategies
        .create(
            ctx,
            a.microservice_id,
            std::iter::once(a.scenario_id)
                .chain(a.also_covers_scenario_ids.iter().copied())
                .collect(),
            parse_enum(&a.strategy.strategy_type, "strategy.type")?,
            minutes(a.strategy.estimated_rto_minutes)?,
            minutes(a.strategy.estimated_rpo_minutes)?,
            StrategyInput {
                title: a.strategy.title,
                description: a.strategy.description,
                prerequisites: Some(a.strategy.prerequisites),
                implementation_status: enum_opt(
                    a.strategy.implementation_status.as_deref(),
                    "strategy.implementationStatus",
                )?,
                last_tested_at: a.strategy.last_tested_at.map(Some),
                ..Default::default()
            },
        )
        .await?;
    let (strategy, gap) = app
        .strategies
        .select(ctx, strategy.meta.id, a.accept_gap, a.gap_justification)
        .await?;
    let view = app
        .runbooks
        .create(
            ctx,
            a.microservice_id,
            a.scenario_id,
            RunbookInput {
                strategy_id: Some(Some(strategy.meta.id)),
                title: Some(a.runbook.title),
                description: a.runbook.description,
                ..Default::default()
            },
            steps,
        )
        .await?;
    json_of(json!({
        "strategyId": strategy.meta.id,
        "gapCheck": { "status": gap.status.to_string(), "rtoGapMinutes": gap.rto_gap_minutes, "rpoGapMinutes": gap.rpo_gap_minutes },
        "runbookId": view.runbook.meta.id,
        "steps": view.steps.iter().map(|s| json!({ "id": s.meta.id, "seq": s.seq, "title": s.title })).collect::<Vec<_>>(),
        "criticalPathMinutes": view.summary.critical_path.get(),
        "exceedsRto": view.summary.exceeds_rto,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompleteStepArgs {
    service_id: Uuid,
    step: String,
    #[serde(default)]
    acknowledge_warnings: bool,
    comment: Option<String>,
}

async fn complete_step(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: CompleteStepArgs = args(value)?;
    let step = app
        .workflow
        .complete(
            ctx,
            a.service_id,
            parse_enum(&a.step, "step")?,
            a.comment,
            a.acknowledge_warnings,
        )
        .await?;
    json_of(json!({ "step": step.definition.key.to_string(), "status": step.status.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitArgs {
    service_id: Uuid,
    comment: Option<String>,
}

async fn submit_plan(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: SubmitArgs = args(value)?;
    let v = app.plans.submit(ctx, a.service_id, a.comment).await?;
    json_of(
        json!({ "planVersionId": v.id, "version": v.plan_number, "status": v.status.to_string() }),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApproveArgs {
    plan_version_id: Uuid,
    comment: Option<String>,
}

async fn approve_plan(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: ApproveArgs = args(value)?;
    let v = app
        .plans
        .approve(ctx, a.plan_version_id, a.comment, None)
        .await?;
    json_of(
        json!({ "planVersionId": v.id, "status": v.status.to_string(), "nextReviewDue": v.next_review_due }),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportArgs {
    plan_version_id: Uuid,
    language: Option<String>,
}

async fn export_plan(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: ExportArgs = args(value)?;
    let lang = enum_opt::<Language>(a.language.as_deref(), "language")?.unwrap_or(Language::En);
    match app.plans.export(ctx, a.plan_version_id, true, lang).await? {
        Export::Markdown(text) => Ok(ToolOutput::Text(text)),
        Export::Json(_) => Err(AppError::internal("unexpected export format")),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclareArgs {
    service_id: Uuid,
    scenario_id: Uuid,
    mode: String,
    note: Option<String>,
}

async fn declare_recovery(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: DeclareArgs = args(value)?;
    let view = app
        .recovery_runs
        .declare(
            ctx,
            a.service_id,
            Declaration {
                scenario_id: a.scenario_id,
                mode: parse_enum(&a.mode, "mode")?,
                plan_version_id: None,
                dr_test_id: None,
                microservice_ids: None,
                note: a.note,
            },
        )
        .await?;
    let run_id = view.run.id;
    status_of(app, ctx, run_id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunIdArgs {
    run_id: Uuid,
}

async fn recovery_status(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: RunIdArgs = args(value)?;
    status_of(app, ctx, a.run_id).await
}

/// Run state plus the steps that are actionable right now.
async fn status_of(app: &AppState, ctx: &TenantContext, run_id: Uuid) -> AppResult<ToolOutput> {
    let view = app.recovery_runs.get(ctx, run_id).await?;
    let steps = app.recovery_runs.steps(ctx, run_id, None, None).await?;
    let actionable: Vec<RunStepStateDto> = steps
        .into_iter()
        .filter(|s| {
            matches!(
                s.display_status,
                RunStepStatus::Pending | RunStepStatus::InProgress | RunStepStatus::Failed
            )
        })
        .map(RunStepStateDto::from)
        .collect();
    json_of(json!({ "run": RecoveryRunDto::from(view), "actionableSteps": actionable }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateStepArgs {
    run_id: Uuid,
    step_id: Uuid,
    to: String,
    note: Option<String>,
    verification_passed: Option<bool>,
}

async fn update_step(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: UpdateStepArgs = args(value)?;
    let t = Transition {
        to: parse_enum(&a.to, "to")?,
        note: a.note,
        verification_passed: a.verification_passed,
        assignee_person_id: None,
    };
    app.recovery_runs
        .transition(ctx, a.run_id, a.step_id, t)
        .await?;
    status_of(app, ctx, a.run_id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PhaseArgs {
    run_id: Uuid,
    to: String,
    note: Option<String>,
}

async fn set_phase(app: &AppState, ctx: &TenantContext, value: Value) -> AppResult<ToolOutput> {
    let a: PhaseArgs = args(value)?;
    let to: RunStatus = parse_enum(&a.to, "to")?;
    app.recovery_runs
        .change_status(ctx, a.run_id, to, a.note)
        .await?;
    status_of(app, ctx, a.run_id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CloseArgs {
    run_id: Uuid,
    outcome: String,
    summary: Option<String>,
    #[serde(default)]
    lessons_learned: Vec<String>,
}

async fn close_recovery(
    app: &AppState,
    ctx: &TenantContext,
    value: Value,
) -> AppResult<ToolOutput> {
    let a: CloseArgs = args(value)?;
    let closing = Closing {
        outcome: parse_enum(&a.outcome, "outcome")?,
        summary: a.summary,
        achieved: Vec::new(),
        lessons_learned: a.lessons_learned,
    };
    let view = app.recovery_runs.close(ctx, a.run_id, closing).await?;
    json_of(RecoveryRunDto::from(view))
}

// ───────────────────────────── input schemas ─────────────────────────────

fn string_enum(values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values })
}

fn uuid() -> Value {
    json!({ "type": "string", "format": "uuid" })
}

fn minutes_schema(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn schema_catalog() -> Value {
    object(json!({ "language": string_enum(&["en", "de"]) }), &[])
}

fn schema_list_services() -> Value {
    object(
        json!({ "query": { "type": "string", "description": "Search in name and description" } }),
        &[],
    )
}

fn schema_service_id() -> Value {
    object(json!({ "serviceId": uuid() }), &["serviceId"])
}

fn schema_create_service() -> Value {
    object(
        json!({
            "name": { "type": "string" },
            "description": { "type": "string" },
            "businessOwnerId": uuid(),
            "technicalOwnerId": uuid(),
            "protectionRequirementAvailability": string_enum(&["normal", "high", "very_high"]),
            "impactLevel": string_enum(&["low", "moderate", "high"]),
            "microservices": { "type": "array", "items": object(json!({
                "name": { "type": "string" }, "description": { "type": "string" }, "platform": { "type": "string" },
                "hostingLocation": { "type": "string" }, "dataStores": { "type": "array", "items": { "type": "string" } },
                "restoreOrder": { "type": "integer", "minimum": 1 }
            }), &["name"]) }
        }),
        &["name"],
    )
}

fn schema_add_dependencies() -> Value {
    object(
        json!({
            "microserviceId": uuid(),
            "dependencies": { "type": "array", "items": object(json!({
                "kind": string_enum(&["microservice", "infrastructure", "platform", "external_service", "supplier", "personnel"]),
                "targetMicroserviceId": uuid(),
                "targetName": { "type": "string", "description": "Required unless kind is microservice" },
                "direction": string_enum(&["upstream", "downstream"]),
                "criticality": string_enum(&["critical", "degradable", "optional"]),
                "dependencyRtoMinutes": minutes_schema("Known recovery time of the dependency")
            }), &["kind", "criticality"]) }
        }),
        &["microserviceId", "dependencies"],
    )
}

fn schema_bia() -> Value {
    object(
        json!({
            "serviceId": uuid(),
            "mtpdMinutes": minutes_schema("Maximum tolerable period of disruption (BSI MTA)"),
            "serviceRtoMinutes": minutes_schema("Service RTO (BSI WAZ), at most the MTPD"),
            "serviceRpoMinutes": minutes_schema("Service RPO (BSI MTDV)"),
            "minimumOperatingLevel": { "type": "string", "description": "Notbetriebsniveau" },
            "regulatoryRequirements": { "type": "string" },
            "impactRatings": { "type": "array", "items": object(json!({
                "impactCategory": string_enum(&["financial", "operational", "reputational", "legal_regulatory", "people_safety"]),
                "timeWindowMinutes": minutes_schema("Time since outage"),
                "level": { "type": "integer", "minimum": 1, "maximum": 4 },
                "rationale": { "type": "string" }
            }), &["impactCategory", "timeWindowMinutes", "level"]) }
        }),
        &[
            "serviceId",
            "mtpdMinutes",
            "serviceRtoMinutes",
            "serviceRpoMinutes",
        ],
    )
}

fn schema_add_scenarios() -> Value {
    object(
        json!({
            "serviceId": uuid(),
            "scenarios": { "type": "array", "items": object(json!({
                "title": { "type": "string" }, "description": { "type": "string" },
                "category": { "type": "string", "description": "A key from get_catalog's scenarioCategories (built-in or tenant-defined custom category)" },
                "parentScenarioId": { "type": "string", "format": "uuid", "description": "Create as sub-scenario" },
                "catalogTemplateId": { "type": "string", "description": "See get_catalog" }
            }), &[]) }
        }),
        &["serviceId", "scenarios"],
    )
}

fn schema_decide() -> Value {
    object(
        json!({
            "scenarioId": uuid(),
            "decision": string_enum(&["selected", "rejected"]),
            "rationale": { "type": "string" },
            "drRequired": string_enum(&["yes", "no_handled_by_ha", "no_accepted_risk", "degraded_mode"]),
            "likelihood": { "type": "integer", "minimum": 1, "maximum": 4 },
            "impact": { "type": "integer", "minimum": 1, "maximum": 4 },
            "priority": string_enum(&["low", "medium", "high", "critical"]),
            "affectedMicroserviceIds": { "type": "array", "items": uuid() }
        }),
        &["scenarioId", "decision", "rationale"],
    )
}

fn schema_objective() -> Value {
    object(
        json!({
            "microserviceId": uuid(),
            "scenarioId": uuid(),
            "rtoMinutes": minutes_schema("Must not exceed the service RTO"),
            "rpoMinutes": minutes_schema("Must not exceed the service RPO"),
            "mttrTargetMinutes": minutes_schema("Typical recovery time target"),
            "firstFunctions": { "type": "array", "items": { "type": "string" } }
        }),
        &["microserviceId", "rtoMinutes", "rpoMinutes"],
    )
}

fn schema_mitigation() -> Value {
    let types: Vec<String> = STRATEGY_TYPES.iter().map(|s| s.key.to_string()).collect();
    let types: Vec<&str> = types.iter().map(String::as_str).collect();
    object(
        json!({
            "microserviceId": uuid(),
            "scenarioId": uuid(),
            "alsoCoversScenarioIds": { "type": "array", "items": uuid(), "description": "Further scenarios the same measure covers" },
            "strategy": object(json!({
                "type": string_enum(&types),
                "title": { "type": "string" }, "description": { "type": "string" },
                "estimatedRtoMinutes": minutes_schema("Estimated recovery time of this strategy"),
                "estimatedRpoMinutes": minutes_schema("Estimated data loss of this strategy"),
                "prerequisites": { "type": "array", "items": { "type": "string" } },
                "implementationStatus": string_enum(&["not_implemented", "in_progress", "implemented"]),
                "lastTestedAt": { "type": "string", "format": "date-time", "description": "When the recovery capability was last tested" }
            }), &["type", "estimatedRtoMinutes", "estimatedRpoMinutes"]),
            "acceptGap": { "type": "boolean", "description": "Required if the strategy misses the objective" },
            "gapJustification": { "type": "string" },
            "runbook": object(json!({
                "title": { "type": "string" }, "description": { "type": "string" },
                "steps": { "type": "array", "items": object(json!({
                    "phase": string_enum(&["activation", "recovery", "reconstitution"]),
                    "title": { "type": "string" }, "instructions": { "type": "string" },
                    "ownerRole": { "type": "string", "description": "Role name (e.g. DBA) or id" },
                    "expectedDurationMinutes": minutes_schema("Expected duration"),
                    "verification": { "type": "string", "description": "How to confirm success" },
                    "isDecisionPoint": { "type": "boolean" },
                    "authorizationRole": { "type": "string", "description": "Role that must authorize (decision points)" }
                }), &["phase", "title"]) }
            }), &["title", "steps"])
        }),
        &["microserviceId", "scenarioId", "strategy", "runbook"],
    )
}

fn schema_readiness() -> Value {
    object(
        json!({ "serviceId": { "type": "string", "format": "uuid", "description": "Omit for the whole tenant" } }),
        &[],
    )
}

fn schema_suggest() -> Value {
    object(
        json!({ "serviceId": uuid(), "language": string_enum(&["en", "de"]) }),
        &["serviceId"],
    )
}

fn schema_complete_step() -> Value {
    let keys: Vec<String> = STEP_DEFINITIONS.iter().map(|d| d.key.to_string()).collect();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    object(
        json!({ "serviceId": uuid(), "step": string_enum(&keys), "acknowledgeWarnings": { "type": "boolean" }, "comment": { "type": "string" } }),
        &["serviceId", "step"],
    )
}

fn schema_submit() -> Value {
    object(
        json!({ "serviceId": uuid(), "comment": { "type": "string" } }),
        &["serviceId"],
    )
}

fn schema_approve() -> Value {
    object(
        json!({ "planVersionId": uuid(), "comment": { "type": "string" } }),
        &["planVersionId"],
    )
}

fn schema_export() -> Value {
    object(
        json!({ "planVersionId": uuid(), "language": string_enum(&["en", "de"]) }),
        &["planVersionId"],
    )
}

fn schema_declare() -> Value {
    object(
        json!({ "serviceId": uuid(), "scenarioId": uuid(), "mode": string_enum(&["real", "test"]), "note": { "type": "string" } }),
        &["serviceId", "scenarioId", "mode"],
    )
}

fn schema_run_id() -> Value {
    object(json!({ "runId": uuid() }), &["runId"])
}

fn schema_update_step() -> Value {
    object(
        json!({
            "runId": uuid(), "stepId": uuid(),
            "to": string_enum(&["in_progress", "done", "skipped", "failed"]),
            "note": { "type": "string", "description": "Required for skipped and failed" },
            "verificationPassed": { "type": "boolean", "description": "Required (true) for done" }
        }),
        &["runId", "stepId", "to"],
    )
}

fn schema_phase() -> Value {
    object(
        json!({ "runId": uuid(), "to": string_enum(&["recovered", "reconstituting", "aborted"]), "note": { "type": "string" } }),
        &["runId", "to"],
    )
}

fn schema_close() -> Value {
    object(
        json!({
            "runId": uuid(), "outcome": string_enum(&["success", "partial", "failed"]), "summary": { "type": "string" },
            "lessonsLearned": { "type": "array", "items": { "type": "string" } }
        }),
        &["runId", "outcome"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<_> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TOOLS.len());
    }
}
