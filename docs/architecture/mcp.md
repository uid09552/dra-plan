---
title: MCP Interface
type: architecture
status: active
tags: [architecture, mcp, ai, api]
created: 2026-09-27
updated: 2026-09-27
related: [0008-use-case-based-mcp, api, backend, plan-authoring, recovery-execution]
---

# MCP Interface

AI agents (Claude and other MCP clients) use the backend through the **Model Context Protocol** endpoint
**`POST /mcp`**, next to the REST API ([[api]]). The decision is recorded in [[0008-use-case-based-mcp]].

## Transport

- MCP **Streamable HTTP**, stateless: every `POST /mcp` carries one JSON-RPC 2.0 message and gets a JSON
  response. There are no server-initiated streams, so `GET /mcp` returns `405`. Batches are rejected.
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`. Notifications get `202 Accepted`.
- Protocol versions `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05` are accepted (echoed back).
- **Same authentication and tenant scoping** as the REST API (currently the dev-mode mock).
- **`Origin` check** (DNS-rebinding protection required by the MCP spec): requests with an `Origin`
  header must match the CORS allowlist (`--cors-origins`); otherwise `403`.
- Tool errors (validation, gates, conflicts) are returned as tool results with `isError: true` and the
  rule ids, so the model can correct itself. Internal errors are not detailed.

## Tools (use-case based)

Tools follow the workflow ([[plan-authoring]], [[recovery-execution]]), not the REST resources. Several
tools combine multiple REST operations in one call.

| Tool | Workflow step | What it does |
|------|---------------|--------------|
| `get_catalog` | – | Scenario templates, strategy types, the 15 workflow steps |
| `list_services` | – | Services with readiness summary |
| `get_service_overview` | all | Service, microservices, scenarios and every step's gate issues → "what next" |
| `get_readiness` | all | Readiness score, gaps and next actions for one service or the whole tenant |
| `suggest_scenarios` | 4 | Catalog scenarios relevant for the service (rule-based), to add with `add_scenarios` |
| `create_service` | 1 | IT service **with its microservices** |
| `add_dependencies` | 2 | Several dependencies of a microservice (reports RTO conflicts) |
| `record_business_impact` | 3 | BIA: MTPD, RTO, RPO, minimum operating level, ratings |
| `add_scenarios` | 4 | Several scenarios, free text or catalog templates |
| `decide_scenario` | 6 | Select/reject with risk rating and rationale |
| `set_recovery_objective` | 7 | RTO/RPO per microservice (default or per scenario) |
| `add_mitigation` | 8–9 | **Strategy + selection (gap check) + runbook with ordered steps**; roles by name; implementation status and last test date; `alsoCoversScenarioIds` for measures covering several scenarios |
| `complete_workflow_step` | 1–15 | Completes a step or returns its blocking issues |
| `submit_plan`, `approve_plan` | 12 | Plan version lifecycle |
| `export_plan` | 12 | Emergency handbook as Markdown |
| `declare_recovery` | run | Start a recovery run against the approved plan |
| `get_recovery_status` | run | RTO clock, progress, **actionable steps** |
| `update_recovery_step` | run | Start / done (verification required) / skip / fail a step |
| `set_recovery_phase` | run | recovered → reconstituting, or abort |
| `close_recovery` | run | Outcome and lessons learned (→ action items) |

Every tool calls the same application use cases as the REST handlers, so validation, workflow gates,
tenant isolation and the audit trail are identical.

## Connecting a client

```bash
make run            # backend in dev mode on 127.0.0.1:8090
claude mcp add --transport http dra-workflow http://127.0.0.1:8090/mcp
```
