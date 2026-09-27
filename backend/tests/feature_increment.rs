#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Readiness dashboard, compliance mapping, scenario suggestions and the scenario mind map
//! (docs/requirements/README.md).

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{TestApp, app};

async fn service_with_profile(app: &TestApp) -> (String, String) {
    let service = app
        .post(
            "/services",
            json!({ "name": "Webshop", "protectionRequirementAvailability": "high" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let microservice = app
        .post(
            &format!("/services/{service}/microservices"),
            json!({ "name": "orders", "platform": "Kubernetes (AKS)", "dataStores": ["orders-db"] }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    app.post(
        &format!("/microservices/{microservice}/dependencies"),
        json!({ "kind": "external_service", "targetName": "Keycloak SSO", "direction": "upstream", "criticality": "critical" }),
    )
    .await
    .expect(StatusCode::CREATED);
    (service, microservice)
}

fn template_ids(body: &Value) -> Vec<String> {
    body.as_array()
        .unwrap()
        .iter()
        .map(|t| t["templateId"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn suggestions_follow_the_service_profile() {
    let app = app().await;
    let (service, _) = service_with_profile(&app).await;

    let suggestions = app
        .get(&format!("/services/{service}/scenario-suggestions"))
        .await
        .expect(StatusCode::OK);
    let ids = template_ids(&suggestions.body);
    for expected in [
        "site-outage",
        "database-failure",
        "cloud-provider-outage",
        "identity-provider-down",
        "external-api-outage",
        "ransomware",
    ] {
        assert!(
            ids.contains(&expected.to_owned()),
            "missing {expected} in {ids:?}"
        );
    }

    // Accepting a suggestion removes it from the list.
    app.post(
        &format!("/services/{service}/scenarios"),
        json!({ "catalogTemplateId": "database-failure" }),
    )
    .await
    .expect(StatusCode::CREATED);
    let after = app
        .get(&format!("/services/{service}/scenario-suggestions"))
        .await
        .expect(StatusCode::OK);
    assert!(!template_ids(&after.body).contains(&"database-failure".to_owned()));

    // A bare service only gets the generic scenarios.
    let bare = app
        .post("/services", json!({ "name": "Intranet" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let bare_ids = template_ids(
        &app.get(&format!("/services/{bare}/scenario-suggestions"))
            .await
            .expect(StatusCode::OK)
            .body,
    );
    assert!(!bare_ids.contains(&"cloud-provider-outage".to_owned()));
    assert!(!bare_ids.contains(&"database-failure".to_owned()));
}

#[tokio::test]
async fn scenarios_form_a_tree_without_cycles() {
    let app = app().await;
    let (service, _) = service_with_profile(&app).await;
    let parent = app
        .post(
            &format!("/services/{service}/scenarios"),
            json!({ "title": "Data center outage", "category": "facility" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let child = app
        .post(
            &format!("/services/{service}/scenarios"),
            json!({ "title": "Cooling failure", "category": "facility", "parentScenarioId": parent }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(child.body["parentScenarioId"], json!(parent));
    let child = child.id();

    app.patch(
        &format!("/scenarios/{parent}"),
        json!({ "parentScenarioId": child }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY);

    // Old coarse categories are gone.
    app.post(
        &format!("/services/{service}/scenarios"),
        json!({ "title": "x", "category": "data" }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY);

    // Detaching works with null.
    let detached = app
        .patch(
            &format!("/scenarios/{child}"),
            json!({ "parentScenarioId": null }),
        )
        .await
        .expect(StatusCode::OK);
    assert!(detached.body.get("parentScenarioId").is_none());

    // Risk matrix: rating before the decision; out-of-range values are rejected.
    let rated = app
        .patch(
            &format!("/scenarios/{child}"),
            json!({ "likelihood": 3, "impact": 4 }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(rated.body["riskScore"], json!(12));
    assert_eq!(rated.body["status"], json!("brainstormed"));
    app.patch(&format!("/scenarios/{child}"), json!({ "impact": 5 }))
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);

    // Deleting a parent keeps its children as top-level scenarios.
    let grandchild = app
        .post(
            &format!("/services/{service}/scenarios"),
            json!({ "title": "Chiller leak", "parentScenarioId": parent }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    app.delete(&format!("/scenarios/{parent}"))
        .await
        .expect(StatusCode::NO_CONTENT);
    let orphan = app
        .get(&format!("/scenarios/{grandchild}"))
        .await
        .expect(StatusCode::OK);
    assert!(orphan.body.get("parentScenarioId").is_none());
}

#[tokio::test]
async fn readiness_and_compliance_report_gaps() {
    let app = app().await;
    let (service, _) = service_with_profile(&app).await;

    let r = app
        .get(&format!("/services/{service}/readiness"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(r.body["total"], json!(15));
    assert!(r.body["score"].as_u64().unwrap() < 50);
    assert!(r.body["critical"].as_u64().unwrap() > 0);
    let actions = r.body["nextActions"].as_array().unwrap();
    assert!(!actions.is_empty() && actions.len() <= 5);
    assert_eq!(actions[0]["severity"], json!("blocking"));

    let tenant = app.get("/readiness").await.expect(StatusCode::OK);
    assert_eq!(tenant.body["services"].as_array().unwrap().len(), 1);
    assert_eq!(tenant.body["nextActions"][0]["serviceId"], json!(service));
    // No approved plan yet counts as "requires review".
    assert_eq!(tenant.body["plansRequiringReview"], json!(1));

    let compliance = app
        .get(&format!("/services/{service}/compliance"))
        .await
        .expect(StatusCode::OK);
    let items = compliance.body.as_array().unwrap();
    assert!(items.len() >= 10);
    assert!(items.iter().all(|i| i["status"] != json!("fulfilled")));
    assert!(items.iter().any(|i| i["framework"] == json!("BSI 200-4")));

    app.get(&format!("/services/{}/readiness", uuid::Uuid::new_v4()))
        .await
        .expect(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn draft_handbook_shows_mitigation_status() {
    let app = app().await;
    let (service, microservice) = service_with_profile(&app).await;
    let scenario = app
        .post(
            &format!("/services/{service}/scenarios"),
            json!({ "title": "Database failure", "category": "database" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let strategy = app
        .post(
            &format!("/microservices/{microservice}/recovery-strategies"),
            json!({
                "scenarioId": scenario, "type": "backup_restore", "title": "Restore from backup",
                "estimatedRtoMinutes": 240, "estimatedRpoMinutes": 60,
                "implementationStatus": "implemented", "lastTestedAt": "2026-01-15T10:00:00Z"
            }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(strategy.body["implementationStatus"], json!("implemented"));
    app.post(
        &format!("/recovery-strategies/{}/select", strategy.id()),
        json!({}),
    )
    .await
    .expect(StatusCode::OK);

    let handbook = app
        .get(&format!("/services/{service}/handbook"))
        .await
        .expect(StatusCode::OK);
    assert!(
        handbook.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/markdown")
    );
    assert!(handbook.text.contains("DRAFT"));
    assert!(handbook.text.contains("Webshop"));
    assert!(handbook.text.contains(
        "| orders | Database failure | Restore from backup | 4 h / 1 h | implemented | 2026-01-15 |"
    ));
}

#[tokio::test]
async fn components_are_optional() {
    let app = app().await;
    let service = app
        .post("/services", json!({ "name": "Intranet" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let scenario = app
        .post(
            &format!("/services/{service}/scenarios"),
            json!({ "title": "Site outage", "category": "facility" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    // Selected without affected components → applies to the whole service (default component).
    app.post(
        &format!("/scenarios/{scenario}/decision"),
        json!({ "decision": "selected", "decisionRationale": "single site", "drRequired": "yes" }),
    )
    .await
    .expect(StatusCode::OK);

    let workflow = app
        .get(&format!("/services/{service}/workflow"))
        .await
        .expect(StatusCode::OK);
    let rules = |key: &str| -> Vec<String> {
        workflow.body["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == key)
            .unwrap()["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["ruleId"].as_str().unwrap().to_owned())
            .collect()
    };
    assert!(!rules("define_service").contains(&"NO_MICROSERVICES".to_owned()));
    assert!(!rules("select_scenarios").contains(&"NO_AFFECTED_MICROSERVICES".to_owned()));
    assert!(rules("recovery_objectives").contains(&"MISSING_OBJECTIVE".to_owned()));

    // The objective goes to the default component.
    let default = app
        .get(&format!("/services/{service}/microservices"))
        .await
        .expect(StatusCode::OK)
        .body[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.post(
        &format!("/microservices/{default}/recovery-objectives"),
        json!({ "rtoMinutes": 240, "rpoMinutes": 60 }),
    )
    .await
    .expect(StatusCode::CREATED);
    let workflow = app
        .get(&format!("/services/{service}/workflow"))
        .await
        .expect(StatusCode::OK);
    let objectives = workflow.body["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "recovery_objectives")
        .unwrap()["issues"]
        .clone();
    assert!(
        !objectives.to_string().contains("MISSING_OBJECTIVE"),
        "{objectives}"
    );
}

#[tokio::test]
async fn one_measure_covers_several_scenarios() {
    let app = app().await;
    let service = app
        .post("/services", json!({ "name": "CRM" }))
        .await
        .expect(StatusCode::CREATED)
        .id();
    let mut scenarios = Vec::new();
    for title in ["Database failure", "Ransomware"] {
        let id = app
            .post(
                &format!("/services/{service}/scenarios"),
                json!({ "title": title }),
            )
            .await
            .expect(StatusCode::CREATED)
            .id();
        app.post(
            &format!("/scenarios/{id}/decision"),
            json!({ "decision": "selected", "decisionRationale": "r", "drRequired": "yes" }),
        )
        .await
        .expect(StatusCode::OK);
        scenarios.push(id);
    }
    let component = app
        .get(&format!("/services/{service}/microservices"))
        .await
        .expect(StatusCode::OK)
        .body[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Legacy single-scenario form still works …
    let legacy = app
        .post(
            &format!("/microservices/{component}/recovery-strategies"),
            json!({ "scenarioId": scenarios[0], "type": "manual_workaround", "estimatedRtoMinutes": 60, "estimatedRpoMinutes": 0 }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(legacy.body["scenarioIds"], json!([scenarios[0]]));

    // … and one measure can cover both scenarios.
    let measure = app
        .post(
            &format!("/microservices/{component}/recovery-strategies"),
            json!({ "scenarioIds": scenarios, "type": "backup_restore", "title": "Restore backup",
                    "estimatedRtoMinutes": 120, "estimatedRpoMinutes": 60 }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(measure.body["scenarioIds"].as_array().unwrap().len(), 2);
    let measure = measure.id();
    app.post(&format!("/recovery-strategies/{measure}/select"), json!({}))
        .await
        .expect(StatusCode::OK);

    // Service-wide list for the measures tab.
    let all = app
        .get(&format!("/services/{service}/recovery-strategies"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(all.body.as_array().unwrap().len(), 2);

    // Selected for both scenarios → no MISSING_STRATEGY.
    let workflow = app
        .get(&format!("/services/{service}/workflow"))
        .await
        .expect(StatusCode::OK);
    let strategies = workflow.body["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "recovery_strategies")
        .unwrap()["issues"]
        .to_string();
    assert!(!strategies.contains("MISSING_STRATEGY"), "{strategies}");

    // A covered scenario cannot be deleted; unlinking works (at least one scenario stays).
    app.delete(&format!("/scenarios/{}", scenarios[1]))
        .await
        .expect(StatusCode::CONFLICT);
    let patched = app
        .patch(
            &format!("/recovery-strategies/{measure}"),
            json!({ "scenarioIds": [scenarios[0]] }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(patched.body["scenarioIds"], json!([scenarios[0]]));
    app.patch(
        &format!("/recovery-strategies/{measure}"),
        json!({ "scenarioIds": [] }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY);
}
