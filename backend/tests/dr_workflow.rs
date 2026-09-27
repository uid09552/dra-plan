#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end: the complete guided DR workflow for one IT service, from service definition to a
//! closed test-mode recovery run, through the real HTTP router and PostgreSQL.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{TestApp, app};

async fn role_id(app: &TestApp, name: &str) -> String {
    let roles = app.get("/roles").await.expect(StatusCode::OK);
    roles
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn complete(app: &TestApp, service: &str, step: &str) -> common::Reply {
    app.post(
        &format!("/services/{service}/workflow/steps/{step}/complete"),
        json!({ "acknowledgeWarnings": true }),
    )
    .await
}

#[tokio::test]
async fn full_dr_lifecycle() {
    let app = app().await;

    // ── Step 1: define the service ───────────────────────────────
    let alice = app
        .post(
            "/persons",
            json!({ "name": "Alice", "phone": "+49 1", "alternateContact": "signal" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let bob = app
        .post(
            "/persons",
            json!({ "name": "Bob", "email": "bob@example.com" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let service = app
        .post("/services", json!({ "name": "Payments", "protectionRequirementAvailability": "very_high", "impactLevel": "high" }))
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(service.headers["etag"], "\"1\"");
    let svc = service.id();

    // Gate blocks: no owners. Components are optional: every service has a default component.
    let blocked = complete(&app, &svc, "define_service")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        blocked
            .rule_ids()
            .contains(&"MISSING_BUSINESS_OWNER".to_owned())
    );
    assert!(!blocked.rule_ids().contains(&"NO_MICROSERVICES".to_owned()));
    let defaults = app
        .get(&format!("/services/{svc}/microservices"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(defaults.body.as_array().unwrap().len(), 1);
    assert_eq!(defaults.body[0]["isDefault"], json!(true));
    assert_eq!(defaults.body[0]["name"], json!("Payments"));
    app.delete(&format!(
        "/microservices/{}",
        defaults.body[0]["id"].as_str().unwrap()
    ))
    .await
    .expect(StatusCode::CONFLICT);

    app.patch(
        &format!("/services/{svc}"),
        json!({ "businessOwnerId": alice, "technicalOwnerId": bob }),
    )
    .await
    .expect(StatusCode::OK);
    let api = app
        .post(
            &format!("/services/{svc}/microservices"),
            json!({ "name": "payment-api", "restoreOrder": 2 }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let db = app
        .post(
            &format!("/services/{svc}/microservices"),
            json!({ "name": "payment-db", "restoreOrder": 1, "dataStores": ["postgres"] }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    complete(&app, &svc, "define_service")
        .await
        .expect(StatusCode::OK);

    // ── Step 2: dependencies ─────────────────────────────────────
    app.post(
        &format!("/microservices/{api}/dependencies"),
        json!({ "kind": "microservice", "targetMicroserviceId": db, "direction": "upstream", "criticality": "critical" }),
    )
    .await
    .expect(StatusCode::CREATED);
    app.post(
        &format!("/microservices/{db}/dependencies"),
        json!({ "kind": "infrastructure", "targetName": "SAN", "direction": "upstream", "criticality": "critical", "dependencyRtoMinutes": 30 }),
    )
    .await
    .expect(StatusCode::CREATED);
    let graph = app
        .get(&format!("/services/{svc}/dependency-graph"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        graph.body["suggestedRestoreOrder"],
        json!([db, api]),
        "dependencies first"
    );
    complete(&app, &svc, "map_dependencies")
        .await
        .expect(StatusCode::OK);

    // Order is enforced: step 4 cannot be completed before step 3.
    let order = complete(&app, &svc, "brainstorm_scenarios")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(order.rule_ids().contains(&"WORKFLOW_ORDER".to_owned()));

    // ── Step 3: BIA ──────────────────────────────────────────────
    let bad_bia = app
        .put(
            &format!("/services/{svc}/bia"),
            json!({ "mtpdMinutes": 60, "serviceRtoMinutes": 120, "serviceRpoMinutes": 15 }),
        )
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_bia.rule_ids(), vec!["SERVICE_RTO_EXCEEDS_MTPD"]);
    let bia = app
        .put(
            &format!("/services/{svc}/bia"),
            json!({
                "mtpdMinutes": 480, "serviceRtoMinutes": 240, "serviceRpoMinutes": 15,
                "minimumOperatingLevel": "card payments only",
                "impactRatings": [
                    { "impactCategory": "financial", "timeWindowMinutes": 60, "level": 2 },
                    { "impactCategory": "financial", "timeWindowMinutes": 240, "level": 3 }
                ]
            }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(bia.body["derivedMtpdMinutes"], 240);
    complete(&app, &svc, "business_impact")
        .await
        .expect(StatusCode::OK);

    // ── Steps 4–6: scenarios ─────────────────────────────────────
    let region = app
        .post(
            &format!("/services/{svc}/scenarios"),
            json!({ "catalogTemplateId": "site-outage" }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(region.body["category"], "infrastructure");
    let region = region.id();
    let corruption = app
        .post(
            &format!("/services/{svc}/scenarios"),
            json!({ "title": "DB corruption", "category": "database" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let deletion = app
        .post(
            &format!("/services/{svc}/scenarios"),
            json!({ "title": "Data deleted" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let bad_deploy = app
        .post(
            &format!("/services/{svc}/scenarios"),
            json!({ "catalogTemplateId": "bad-deployment" }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    complete(&app, &svc, "brainstorm_scenarios")
        .await
        .expect(StatusCode::OK);

    let uncategorized = complete(&app, &svc, "consolidate_scenarios")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        uncategorized
            .rule_ids()
            .contains(&"SCENARIO_UNCATEGORIZED".to_owned())
    );
    let merged = app
        .post(
            &format!("/scenarios/{corruption}/merge"),
            json!({ "sourceScenarioIds": [deletion] }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(merged.body["status"], "brainstormed");
    assert_eq!(
        app.get(&format!("/scenarios/{deletion}")).await.body["status"],
        "merged"
    );
    complete(&app, &svc, "consolidate_scenarios")
        .await
        .expect(StatusCode::OK);

    app.post(
        &format!("/scenarios/{bad_deploy}/decision"),
        json!({ "decision": "rejected", "decisionRationale": "handled by blue/green" }),
    )
    .await
    .expect(StatusCode::OK);
    app.post(
        &format!("/scenarios/{corruption}/decision"),
        json!({ "decision": "rejected", "decisionRationale": "covered by PITR runbook later", "dr_required": "yes" }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY); // unknown field (snake_case)
    app.post(
        &format!("/scenarios/{corruption}/decision"),
        json!({ "decision": "rejected", "decisionRationale": "covered by backups" }),
    )
    .await
    .expect(StatusCode::OK);
    let selected = app
        .post(
            &format!("/scenarios/{region}/decision"),
            json!({ "decision": "selected", "likelihood": 2, "impact": 4, "priority": "high", "drRequired": "yes",
                    "decisionRationale": "critical", "affectedMicroserviceIds": [api, db] }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(selected.body["riskScore"], 8);
    complete(&app, &svc, "select_scenarios")
        .await
        .expect(StatusCode::OK);

    // ── Step 7: objectives ───────────────────────────────────────
    app.post(
        &format!("/microservices/{api}/recovery-objectives"),
        json!({ "rtoMinutes": 300, "rpoMinutes": 15 }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY); // exceeds service RTO 240
    app.post(
        &format!("/microservices/{api}/recovery-objectives"),
        json!({ "rtoMinutes": 120, "rpoMinutes": 15 }),
    )
    .await
    .expect(StatusCode::CREATED);
    let db_objective = app
        .post(
            &format!("/microservices/{db}/recovery-objectives"),
            json!({ "rtoMinutes": 180, "rpoMinutes": 5 }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    let conflict = complete(&app, &svc, "recovery_objectives")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        conflict
            .rule_ids()
            .contains(&"DEPENDENCY_RTO_CONFLICT".to_owned()),
        "db (180) slower than api (120)"
    );
    app.patch(
        &format!("/recovery-objectives/{db_objective}"),
        json!({ "rtoMinutes": 60 }),
    )
    .await
    .expect(StatusCode::OK);
    complete(&app, &svc, "recovery_objectives")
        .await
        .expect(StatusCode::OK);

    // ── Step 8: strategies (mitigations) ─────────────────────────
    let slow = app
        .post(
            &format!("/microservices/{db}/recovery-strategies"),
            json!({ "scenarioId": region, "type": "backup_restore", "estimatedRtoMinutes": 240, "estimatedRpoMinutes": 60 }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(slow.body["gapCheck"]["status"], "gap");
    let gap = app
        .post(
            &format!("/recovery-strategies/{}/select", slow.id()),
            json!({}),
        )
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(gap.rule_ids(), vec!["STRATEGY_GAP"]);
    let replica = app
        .post(
            &format!("/microservices/{db}/recovery-strategies"),
            json!({ "scenarioId": region, "type": "cross_region_failover", "title": "Promote replica", "estimatedRtoMinutes": 30, "estimatedRpoMinutes": 1 }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    app.post(&format!("/recovery-strategies/{replica}/select"), json!({}))
        .await
        .expect(StatusCode::OK);
    let api_strategy = app
        .post(
            &format!("/microservices/{api}/recovery-strategies"),
            json!({ "scenarioId": region, "type": "iac_rebuild", "estimatedRtoMinutes": 150, "estimatedRpoMinutes": 0 }),
        )
        .await
        .expect(StatusCode::CREATED)
        .id();
    // Accepting a gap requires a justification and creates an action item.
    app.post(
        &format!("/recovery-strategies/{api_strategy}/select"),
        json!({ "acceptGap": true }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY);
    let accepted = app
        .post(
            &format!("/recovery-strategies/{api_strategy}/select"),
            json!({ "acceptGap": true, "gapJustification": "IaC pipeline speed-up planned" }),
        )
        .await
        .expect(StatusCode::OK);
    assert!(accepted.body["acceptedGapActionItemId"].is_string());
    app.post(&format!("/microservices/{db}/data-protection"), json!({ "dataStore": "postgres", "method": "pitr", "frequencyMinutes": 5, "immutable": true }))
        .await
        .expect(StatusCode::CREATED);
    complete(&app, &svc, "recovery_strategies")
        .await
        .expect(StatusCode::OK);

    // ── Step 9: runbooks with mitigation steps ───────────────────
    let ic = role_id(&app, "Incident Commander").await;
    let dba = role_id(&app, "DBA").await;
    let platform = role_id(&app, "Platform").await;
    let comms = role_id(&app, "Communications").await;
    let db_runbook = app
        .post(
            &format!("/microservices/{db}/runbooks"),
            json!({
                "scenarioId": region, "strategyId": replica, "title": "Fail over database",
                "steps": [
                    { "phase": "activation", "title": "Declare DR", "ownerRoleId": ic, "expectedDurationMinutes": 10,
                      "verification": "incident opened", "isDecisionPoint": true, "requiresAuthorizationRoleId": ic },
                    { "phase": "recovery", "title": "Promote replica", "ownerRoleId": dba, "expectedDurationMinutes": 15,
                      "verification": "replica writable" }
                ]
            }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(db_runbook.body["criticalPathMinutes"], 25);
    assert_eq!(db_runbook.body["exceedsRto"], false);
    let db_runbook_id = db_runbook.id();
    let steps = db_runbook.body["steps"].as_array().unwrap().clone();
    // Insert a step in the middle; later steps are renumbered.
    let dns = app
        .post(
            &format!("/runbooks/{db_runbook_id}/steps"),
            json!({ "phase": "recovery", "title": "Update DNS", "ownerRoleId": platform, "expectedDurationMinutes": 5, "verification": "dig ok", "position": 2 }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(dns.body["seq"], 2);
    let listed = app
        .get(&format!("/runbooks/{db_runbook_id}/steps"))
        .await
        .expect(StatusCode::OK);
    let titles: Vec<&str> = listed
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Declare DR", "Update DNS", "Promote replica"]);
    // Reorder back; a step depending on a later step is rejected.
    let order = json!({ "stepIds": [steps[0]["id"], steps[1]["id"], dns.id()] });
    app.put(&format!("/runbooks/{db_runbook_id}/steps/order"), order)
        .await
        .expect(StatusCode::OK);
    app.patch(
        &format!("/runbook-steps/{}", steps[0]["id"].as_str().unwrap()),
        json!({ "dependsOnStepIds": [dns.id()] }),
    )
    .await
    .expect(StatusCode::UNPROCESSABLE_ENTITY);

    let missing = complete(&app, &svc, "runbooks")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        missing.rule_ids().contains(&"MISSING_RUNBOOK".to_owned()),
        "api has no runbook yet"
    );
    app.post(
        &format!("/microservices/{api}/runbooks"),
        json!({ "scenarioId": region, "title": "Rebuild API", "steps": [
            { "phase": "recovery", "title": "Run IaC pipeline", "ownerRoleId": platform, "expectedDurationMinutes": 60, "verification": "pods ready" },
            { "phase": "reconstitution", "title": "Validate payments", "ownerRoleId": comms, "expectedDurationMinutes": 15, "verification": "test payment ok" }
        ]}),
    )
    .await
    .expect(StatusCode::CREATED);
    let service_runbooks = app
        .get(&format!("/services/{svc}/runbooks?scenarioId={region}"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(
        service_runbooks.body.as_array().unwrap()[0]["title"],
        "Fail over database",
        "restore order"
    );
    complete(&app, &svc, "runbooks")
        .await
        .expect(StatusCode::OK);

    // ── Steps 10–11: roles and communication ─────────────────────
    let roles_gate = complete(&app, &svc, "roles")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        roles_gate
            .rule_ids()
            .contains(&"ROLE_WITHOUT_ASSIGNEE".to_owned())
    );
    for role in [&ic, &dba, &platform, &comms] {
        app.post(
            &format!("/services/{svc}/role-assignments"),
            json!({ "roleId": role, "personId": alice }),
        )
        .await
        .expect(StatusCode::CREATED);
        app.post(
            &format!("/services/{svc}/role-assignments"),
            json!({ "roleId": role, "personId": bob, "isDeputy": true }),
        )
        .await
        .expect(StatusCode::CREATED);
    }
    complete(&app, &svc, "roles").await.expect(StatusCode::OK);
    for trigger in ["dr_declared", "recovered"] {
        app.post(
            &format!("/services/{svc}/communication-rules"),
            json!({ "trigger": trigger, "audience": "internal", "channel": "phone bridge", "responsibleRoleId": comms, "authorizerRoleId": ic }),
        )
        .await
        .expect(StatusCode::CREATED);
    }
    app.post(
        &format!("/services/{svc}/communication-rules"),
        json!({ "trigger": "status_update", "audience": "management", "channel": "email", "responsibleRoleId": comms, "frequencyMinutes": 30 }),
    )
    .await
    .expect(StatusCode::CREATED);
    complete(&app, &svc, "communication")
        .await
        .expect(StatusCode::OK);

    let validation = app
        .get(&format!("/services/{svc}/validation"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(validation.body["blockingCount"], 0, "{}", validation.text);

    // ── Step 12: submit, approve, export ─────────────────────────
    let version = app
        .post(
            &format!("/services/{svc}/plan-versions"),
            json!({ "comment": "first plan" }),
        )
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(version.body["status"], "in_review");
    assert_eq!(version.body["snapshot"]["formatVersion"], 1);
    let version_id = version.id();
    app.post(&format!("/services/{svc}/plan-versions"), json!({}))
        .await
        .expect(StatusCode::CONFLICT);
    let approved = app
        .post(&format!("/plan-versions/{version_id}/approve"), json!({}))
        .await
        .expect(StatusCode::OK);
    assert_eq!(approved.body["status"], "approved");
    assert!(approved.body["nextReviewDue"].is_string());
    let md = app
        .send(
            axum::http::Method::GET,
            &format!("/plan-versions/{version_id}/export?format=markdown"),
            None,
            &[("accept-language", "de")],
        )
        .await;
    assert_eq!(md.status, StatusCode::OK);
    assert!(
        md.text.contains("# Notfallhandbuch: Payments"),
        "{}",
        md.text
    );
    assert!(md.text.contains("Promote replica"));
    assert!(
        md.text.contains("signal"),
        "alternate contact for offline use"
    );
    app.get(&format!("/plan-versions/{version_id}/export?format=pdf"))
        .await
        .expect(StatusCode::NOT_IMPLEMENTED);

    let workflow = app
        .get(&format!("/services/{svc}/workflow"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(workflow.body["currentStepKey"], "test");
    assert_eq!(
        workflow.body["steps"][11]["status"], "complete",
        "approval completes step 12"
    );

    // Edits after approval do not change the approved snapshot.
    app.patch(
        &format!("/services/{svc}"),
        json!({ "name": "Payments v2" }),
    )
    .await
    .expect(StatusCode::OK);
    let pinned = app
        .get(&format!("/plan-versions/{version_id}"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(pinned.body["snapshot"]["service"]["name"], "Payments");

    // ── Steps 13–14: test via a recovery run ─────────────────────
    let test = app
        .post(&format!("/services/{svc}/dr-tests"), json!({ "type": "technical_recovery", "scenarioId": region, "plannedAt": "2026-10-01T08:00:00Z" }))
        .await
        .expect(StatusCode::CREATED);
    assert_eq!(
        test.body["planVersionId"],
        version_id.as_str(),
        "defaults to the approved version"
    );
    let test_id = test.id();
    let run = app
        .post(
            &format!("/services/{svc}/recovery-runs"),
            json!({ "scenarioId": region, "mode": "test", "drTestId": test_id }),
        )
        .await
        .expect(StatusCode::CREATED);
    let run_id = run.id();
    assert_eq!(run.body["progress"]["total"], 5);
    assert_eq!(run.body["clock"]["serviceRtoMinutes"], 240);
    app.post(
        &format!("/services/{svc}/recovery-runs"),
        json!({ "scenarioId": region, "mode": "real" }),
    )
    .await
    .expect(StatusCode::CONFLICT);
    app.delete(&format!("/services/{svc}"))
        .await
        .expect(StatusCode::CONFLICT);

    let run_steps = app
        .get(&format!("/recovery-runs/{run_id}/steps"))
        .await
        .expect(StatusCode::OK);
    let run_steps: Vec<Value> = run_steps.body.as_array().unwrap().clone();
    assert_eq!(
        run_steps[0]["title"], "Declare DR",
        "db first (restore order)"
    );
    assert_eq!(run_steps[1]["status"], "blocked");
    let step = |i: usize| run_steps[i]["runbookStepId"].as_str().unwrap().to_owned();
    let transition = |i: usize, body: Value| {
        let path = format!("/recovery-runs/{run_id}/steps/{}/transition", step(i));
        let app = &app;
        async move { app.post(&path, body).await }
    };
    transition(1, json!({ "to": "in_progress" }))
        .await
        .expect(StatusCode::CONFLICT);
    transition(0, json!({ "to": "done" }))
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    transition(0, json!({ "to": "done", "verificationPassed": true }))
        .await
        .expect(StatusCode::OK);
    for i in 1..5 {
        transition(i, json!({ "to": "in_progress" }))
            .await
            .expect(StatusCode::OK);
        transition(i, json!({ "to": "done", "verificationPassed": true }))
            .await
            .expect(StatusCode::OK);
    }
    app.post(
        &format!("/recovery-runs/{run_id}/events"),
        json!({ "type": "communication_sent", "message": "status mail sent" }),
    )
    .await
    .expect(StatusCode::CREATED);
    app.post(
        &format!("/recovery-runs/{run_id}/close"),
        json!({ "outcome": "success" }),
    )
    .await
    .expect(StatusCode::CONFLICT);
    app.post(
        &format!("/recovery-runs/{run_id}/status"),
        json!({ "to": "recovered" }),
    )
    .await
    .expect(StatusCode::OK);
    let closed = app
        .post(
            &format!("/recovery-runs/{run_id}/close"),
            json!({ "outcome": "partial", "summary": "ok",
                    "achieved": [{ "microserviceId": db, "achievedRtoMinutes": 40, "achievedRpoMinutes": 2 },
                                 { "microserviceId": api, "achievedRtoMinutes": 150 }],
                    "lessonsLearned": ["DNS TTL too high"] }),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(closed.body["status"], "closed");
    let events = app
        .get(&format!("/recovery-runs/{run_id}/events"))
        .await
        .expect(StatusCode::OK);
    let types: Vec<&str> = events
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert_eq!(types.first(), Some(&"declared"));
    assert_eq!(types.last(), Some(&"closed"));

    let results = app
        .get(&format!("/dr-tests/{test_id}/results"))
        .await
        .expect(StatusCode::OK);
    let results = results.body.as_array().unwrap();
    let db_result = results
        .iter()
        .find(|r| r["microserviceId"] == db.as_str())
        .unwrap();
    assert_eq!(db_result["targetRtoMinutes"], 60);
    assert_eq!(db_result["passed"], true);
    let api_result = results
        .iter()
        .find(|r| r["microserviceId"] == api.as_str())
        .unwrap();
    assert_eq!(api_result["rtoMet"], false, "150 > 120");
    let test = app
        .get(&format!("/dr-tests/{test_id}"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(test.body["outcome"], "partially_passed");
    app.delete(&format!("/dr-tests/{test_id}"))
        .await
        .expect(StatusCode::CONFLICT);
    complete(&app, &svc, "test").await.expect(StatusCode::OK);
    complete(&app, &svc, "measure").await.expect(StatusCode::OK);

    // ── Step 15: improve ─────────────────────────────────────────
    let items = app
        .get(&format!("/services/{svc}/action-items?status=open"))
        .await
        .expect(StatusCode::OK);
    let items = items.body.as_array().unwrap().clone();
    assert_eq!(items.len(), 2, "accepted gap + lesson learned");
    let improve = complete(&app, &svc, "improve")
        .await
        .expect(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        improve
            .rule_ids()
            .contains(&"ACTION_WITHOUT_OWNER".to_owned())
    );
    for item in &items {
        app.patch(
            &format!("/action-items/{}", item["id"].as_str().unwrap()),
            json!({ "ownerPersonId": alice, "dueDate": "2026-12-31" }),
        )
        .await
        .expect(StatusCode::OK);
    }
    complete(&app, &svc, "improve").await.expect(StatusCode::OK);

    let summary = app
        .get(&format!("/services/{svc}"))
        .await
        .expect(StatusCode::OK);
    assert_eq!(summary.body["summary"]["workflowCompletionPercent"], 100);
    assert_eq!(summary.body["summary"]["currentPlanStatus"], "approved");

    // Reopening an early step flags later steps for review.
    let reopened = app
        .post(
            &format!("/services/{svc}/workflow/steps/select_scenarios/reopen"),
            json!({}),
        )
        .await
        .expect(StatusCode::OK);
    assert_eq!(reopened.body["steps"][5]["status"], "in_progress");
    assert_eq!(reopened.body["steps"][6]["status"], "needs_review");

    // Audit trail recorded by the database trigger.
    let audit = app
        .get(&format!(
            "/audit-log?entityType=recovery_objective&entityId={db_objective}"
        ))
        .await
        .expect(StatusCode::OK);
    let actions: Vec<&str> = audit.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, vec!["update", "create"]);
}
