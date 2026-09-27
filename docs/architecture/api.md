---
title: Backend API
type: architecture
status: draft
tags: [api, openapi, backend]
created: 2026-09-27
updated: 2026-09-27
related: [0006-rest-openapi-contract-first, dr-plan-model, plan-authoring, recovery-execution]
---

# Backend API

The contract is **[`api/openapi.yaml`](../../api/openapi.yaml)** (OpenAPI 3.1, base path `/api/v1`). It is contract-first: the spec is the source of truth for the Rust backend and for the generated Angular client ([[0006-rest-openapi-contract-first]]).

Lint it with `npx @redocly/cli lint` (the configuration is in `redocly.yaml`). AI agents can use the same use cases through the MCP endpoint `POST /mcp` ([[mcp]]).

## Resource layout

The **tenant is not part of the path**. It comes from the `tenant_id` claim of the JWT access token (see [Tenancy](#tenancy)). Collections are nested under their parent. Single items are addressed flat by id.

```
/tenants                                        list my tenants / create a tenant (tenant-independent)
/tenant                                         current tenant from the token: get / update / delete
/members | /persons | /roles | /catalog         directory & seed data
/services                                       IT services                 (step 1)
  /{s}/bia                                      business impact analysis   (step 3)
  /{s}/scenarios                                brainstorm                  (step 4)
  /{s}/scenario-suggestions                     catalog scenarios relevant for the service
  /{s}/recovery-strategies                      all measures of the service (each covers scenarioIds[])
  /{s}/readiness  ·  /compliance                readiness figures, next actions; NIST/BSI requirement status
  /{s}/handbook                                 live draft of the emergency handbook (Markdown)
  /{s}/microservices                            microservices               (step 1)
  /{s}/runbooks?scenarioId=                     all mitigations for a scenario
  /{s}/role-assignments | communication-rules   (steps 10–11)
  /{s}/workflow  ·  /workflow/steps/{key}/complete|reopen   guided workflow + gates
  /{s}/validation  ·  /dependency-graph         integrity rules, restore order
  /{s}/plan-versions                            submit for review           (step 12)
  /{s}/dr-tests  ·  /action-items               (steps 13–15)
  /{s}/recovery-runs                            declare DR
/readiness                                      tenant-wide readiness dashboard
/scenarios/{id}  ·  /merge  ·  /decision        consolidate / select (steps 5–6); PATCH also sets
                                                parentScenarioId and the pre-decision risk rating
/microservices/{m}
  /dependencies                                 (step 2)
  /recovery-objectives                          (step 7)
  /recovery-strategies  →  /recovery-strategies/{id}/select   mitigations + gap check (step 8)
  /data-protection                              backups (BSI CON.3)
  /runbooks  →  /runbooks/{id}/steps  ·  /steps/order         mitigation steps (step 9)
/plan-versions/{id}/approve|reject|export
/ai/suggestions  →  /{id}/decision              AI proposals → accept/edit/reject
/recovery-runs/{r}
  /steps  ·  /steps/{id}/transition             execute runbook steps
  /status  ·  /close  ·  /events  ·  /stream (SSE)   phases, timeline, live updates
/audit-log
```

## Tenancy

- The backend extracts `tenant_id` from the validated JWT and scopes **every** query to it. Storage also enforces this, for example with PostgreSQL row-level security fed from the request context ([[0004-tenant-service-microservice-hierarchy]]).
- A missing or invalid claim, or a tenant the user is not a member of, returns `403`. An id from another tenant returns `404`, so the API never confirms that it exists.
- To switch tenants, the client gets a new token for the other tenant from the identity provider. `GET /tenants` lists the caller's memberships to support this.
- `POST /tenants` creates a tenant and makes the caller its `admin`. The client then needs a token for the new tenant.

## "Mitigations"

A mitigation for a scenario is two things:
1. A **recovery strategy** for each affected microservice (*how* to recover, with estimated RTO and RPO). Selecting one runs the gap check against the objective (BSI *Soll-Ist-Vergleich*).
2. A **runbook** with ordered **steps** in the phases `activation` → `recovery` → `reconstitution` (NIST SP 800-34). Each step has an owner role, a duration, a verification and dependencies.

`POST /microservices/{m}/runbooks` accepts inline `steps`, so a complete mitigation can be created in one call.

## Cross-cutting conventions

| Concern | Convention |
|---------|-----------|
| Tenancy | Taken from the JWT `tenant_id` claim, never from the path (see above) |
| Updates | `PATCH` with `application/merge-patch+json` |
| Concurrency | `version` / `ETag` + `If-Match`. A stale version returns `412`. |
| Pagination | `cursor` + `limit`, returning `{ items, nextCursor }` (for large collections) |
| Errors | RFC 9457 `application/problem+json`. `422` carries `issues[]` with `ruleId`, `severity` and `standardRef`. |
| Workflow gates | `POST …/workflow/steps/{key}/complete` evaluates the gate and returns `422` with the blocking issues |
| AI | Async `202` plus polling. Proposals are only applied through `/decision`, with `origin: ai_accepted`. `503` means AI is down and everything else keeps working. |
| Immutability | Approved `plan-versions` are read-only. Recovery runs pin a version. Run events and the audit log are append-only. |
| Live updates | `GET /recovery-runs/{r}/stream` (Server-Sent Events, resumable with `Last-Event-ID`) |
| Units | Durations are integer minutes. Timestamps are RFC 3339 UTC. |

## Authorization (tenant roles)

| Role | Can |
|------|-----|
| `admin` | Everything, including tenant settings and members |
| `author` | Create and edit services, microservices and DR items. Request AI suggestions. Submit plan versions. |
| `reviewer` | Everything an author can do, plus approve and reject plan versions, and confirm the scenario selection gate |
| `responder` | Read plans. Declare and execute recovery runs. |
| `auditor` | Read-only, including the audit log and run timelines |

DR roles (Incident Commander and others) additionally control runbook decision points (`requiresAuthorizationRoleId`) and failover authorization.
