---
title: "ADR-0004: Tenant → IT service → microservice → DR items hierarchy"
type: adr
status: accepted
tags: [adr, data-model, multi-tenancy]
created: 2026-09-27
updated: 2026-09-27
related: [dr-plan-model]
---

# ADR-0004: Tenant → IT service → microservice → DR items hierarchy

## Context
Organizations run many IT services, and each service is made of several technical components (microservices). DR requirements are set from a business view (the service), but recovery is carried out per component.

## Decision
- The tool is **multi-tenant**. `tenant_id` is on every table so isolation can be enforced (for example with row-level security).
- The hierarchy is **Tenant → IT Service → Microservice → DR items**.
- The BIA, scenarios, roles, communication, tests and plan versions attach to the **IT service**.
- Dependencies, recovery objectives, strategies, data protection and runbooks attach to the **microservice**. They are scoped by scenario and bounded by the service-level objectives.
- The **DR plan version** is an immutable snapshot of the whole IT service subtree.

## Consequences
- Service-level targets act as upper bounds, which gives simple, automatable consistency checks.
- Recovery runs pin a snapshot, so later edits never change a recovery that is in progress.
- A service with a single component still has to have one microservice record. The UI can hide this.
- Shared platform components (for example a central database cluster) are modeled as dependencies of kind `infrastructure`/`platform`, or as a microservice of another IT service.
