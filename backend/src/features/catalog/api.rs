use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use super::domain::{
    SCENARIO_TEMPLATES, STRATEGY_TYPES, category_label, default_roles, test_type_label,
    workflow_steps,
};
use crate::app::AppState;
use crate::features::dr_tests::domain::DrTestType;
use crate::features::scenarios::domain::ScenarioCategory;
use crate::shared::kernel::{Language, TenantContext};

pub fn routes() -> Router<AppState> {
    Router::new().route("/catalog", get(get_catalog))
}

/// `Accept-Language: de…` selects German, everything else English.
pub fn language_from(headers: &HeaderMap) -> Language {
    let accept = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if accept.trim_start().to_ascii_lowercase().starts_with("de") {
        Language::De
    } else {
        Language::En
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyLabel {
    key: String,
    label: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TemplateDto {
    id: &'static str,
    category: String,
    title: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StrategyTypeDto {
    key: String,
    label: &'static str,
    typical_rto: &'static str,
    typical_rpo: &'static str,
    notes: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TestTypeDto {
    key: String,
    label: &'static str,
    depth: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StepDto {
    key: String,
    number: u8,
    phase: String,
    title: &'static str,
    nist_ref: &'static str,
    bsi_ref: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDto {
    scenario_categories: Vec<KeyLabel>,
    scenario_templates: Vec<TemplateDto>,
    strategy_types: Vec<StrategyTypeDto>,
    test_types: Vec<TestTypeDto>,
    default_roles: Vec<&'static str>,
    workflow_steps: Vec<StepDto>,
}

async fn get_catalog(_ctx: TenantContext, headers: HeaderMap) -> Json<CatalogDto> {
    let lang = language_from(&headers);
    Json(CatalogDto {
        scenario_categories: ScenarioCategory::ALL
            .iter()
            .map(|c| KeyLabel {
                key: c.to_string(),
                label: category_label(*c).get(lang),
            })
            .collect(),
        scenario_templates: SCENARIO_TEMPLATES
            .iter()
            .map(|t| TemplateDto {
                id: t.id,
                category: t.category.to_string(),
                title: t.title(lang),
                description: t.description(lang),
            })
            .collect(),
        strategy_types: STRATEGY_TYPES
            .iter()
            .map(|s| StrategyTypeDto {
                key: s.key.to_string(),
                label: s.label.get(lang),
                typical_rto: s.typical_rto.get(lang),
                typical_rpo: s.typical_rpo.get(lang),
                notes: s.notes.get(lang),
            })
            .collect(),
        test_types: DrTestType::ALL
            .iter()
            .map(|t| {
                let (label, depth) = test_type_label(*t);
                TestTypeDto {
                    key: t.to_string(),
                    label: label.get(lang),
                    depth,
                }
            })
            .collect(),
        default_roles: default_roles().collect(),
        workflow_steps: workflow_steps()
            .iter()
            .map(|s| StepDto {
                key: s.key.to_string(),
                number: s.number,
                phase: s.phase.to_string(),
                title: s.title.get(lang),
                nist_ref: s.nist_ref,
                bsi_ref: s.bsi_ref,
            })
            .collect(),
    })
}
