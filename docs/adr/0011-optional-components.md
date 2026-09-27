---
title: "ADR-0011: Components are optional (default component per service)"
type: adr
status: accepted
tags: [adr, domain, data-model]
created: 2026-09-27
updated: 2026-09-27
related: [0004-tenant-service-microservice-hierarchy, dr-plan-model, plan-authoring]
---

# ADR-0011: Components are optional (default component per service)

## Context
[[0004-tenant-service-microservice-hierarchy]] attaches objectives, strategies, backups and runbooks to
microservices. The workflow therefore required at least one microservice (`NO_MICROSERVICES`) and every
selected scenario had to name affected microservices (`NO_AFFECTED_MICROSERVICES`). Many IT services are
recovered as a whole (a SaaS subscription, a single VM, an appliance), and "microservice" suggests an
architecture style the service may not have. Users should only describe parts when they recover separately.

## Options considered
- **Service-level DR items** (nullable `microservice_id` everywhere): honest model, but touches every DR
  table, repository, gate, snapshot and the handbook; two code paths for every rule.
- **Default component per service**: one component with `is_default = true` stands for the whole service.
  All existing rules keep working; scenarios without affected components apply to it.
- **Create a component in the UI when needed**: hidden coupling; API and MCP clients would still hit the gate.

## Decision
Default component per service:
- Migration `0003` adds `microservice.is_default` (at most one per service) and a trigger that creates it
  in the same transaction as the service (name = service name); existing services are backfilled.
- The default component cannot be deleted (409). It can be renamed and edited.
- `PlanAggregate::affected_components`: explicitly affected components, otherwise the default one. Used by
  the gates (objectives, strategies, runbooks) and by recovery runs.
- Gates `NO_MICROSERVICES` and `NO_AFFECTED_MICROSERVICES` are removed.
- The dependency graph hides the default component when explicit components exist and no dependency
  involves it.
- Terminology: the UI and docs say **component**; the API and database keep `microservice` (no breaking change).

## Consequences
- A service can be planned end to end without describing any components.
- Services with explicit components still have the default component; it carries objectives and
  strategies for scenarios that affect the whole service.
- API clients see one component more than they created (`isDefault: true`).
