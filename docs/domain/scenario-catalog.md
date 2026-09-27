---
title: Scenario and Strategy Catalog
type: domain
status: draft
tags: [domain, scenarios, strategies, seed-data]
created: 2026-09-27
updated: 2026-09-27
related: [plan-authoring, dr-plan-model]
---

# Scenario and Strategy Catalog

Default seed data for workflow steps 4–8 ([[plan-authoring]]). Each tenant can extend it.

## Scenario categories (`scenario.category`)

The 15 categories of the [[README|feature requirements]] (migration `0002` mapped the former
`data` → `database`, `dependency` → `supplier`, `security` → `cybersecurity`, `people_facility` → `people`):

| Category | Example scenarios |
|----------|-------------------|
| `infrastructure` | Complete data center or region outage |
| `hardware` | Server or host failure |
| `network` | Network outage, DNS failure |
| `cloud` | Cloud provider or region outage |
| `application` | Bad deployment, expired certificate |
| `database` | Database failure, data corruption |
| `storage` | Storage system failure |
| `backup` | Backup unusable when needed |
| `cybersecurity` | Ransomware, DDoS, compromised credentials |
| `people` | Loss of key personnel or knowledge |
| `supplier` | External API, payment or SaaS provider outage, identity provider down |
| `facility` | Building unavailable |
| `power` | Power or cooling outage |
| `environmental` | Flood, fire, storm |
| `operational` | Operator error, accidental deletion |

Scenarios form a tree: `parentScenarioId` makes a scenario a sub-scenario (mind map). Cycles and
merged parents are rejected; deleting a parent keeps its children as top-level scenarios.

## Scenario suggestions

`GET /services/{id}/scenario-suggestions` (MCP `suggest_scenarios`) proposes catalog templates that are
relevant for the service and not yet captured (matched by title). Rules (`catalog::domain::ServiceProfile`):

| Relevance | Derived from |
|-----------|--------------|
| `always` | every service (site outage, server failure, network outage, ransomware, key personnel, …) |
| `data_stores` | a component lists data stores |
| `cloud` | platform or hosting mentions AWS, Azure, GCP, cloud, Kubernetes, OpenShift |
| `external_dependency` | a dependency of kind `external_service` or `supplier` |
| `identity_provider` | a dependency name mentions IdP, Keycloak, Entra, Active Directory, LDAP, SSO |
| `high_protection` | protection requirement high/very high or impact level high |

LLM-based suggestions will add to this once an AI provider is chosen ([[open-questions]]).

## Recovery strategy types (`recovery_strategy.type`)

| Type | Typical RTO | Typical RPO | Notes |
|------|-------------|-------------|-------|
| `backup_restore` | Hours to days | Backup interval | Cheapest option. Needs tested restores (BSI CON.3). |
| `replication` | Minutes to hours | Seconds to minutes | Also replicates corruption, so combine it with backups. |
| `active_passive` | Minutes to hours | Seconds to minutes | Warm or cold standby |
| `active_active` | Near zero | Near zero | Highest cost and complexity |
| `cross_region_failover` | Minutes to hours | Seconds to minutes | Requires DNS and traffic switching |
| `iac_rebuild` | Hours | Depends on the data | Rebuild from infrastructure-as-code. Also the preferred option after a security compromise. |
| `degraded_mode` | Immediate | n/a | Runs at the minimum operating level (*Notbetriebsniveau*) |
| `manual_workaround` | Varies | n/a | A business process fallback |

## DR test types (`dr_test.type`)

In increasing depth: `plan_review` → `tabletop` (walk through the scenario verbally) → `simulation` (exercise the procedures without failing the service) → `technical_recovery` (actually restore or fail over a component) → `full_dr` (recover the whole service in the DR environment).
