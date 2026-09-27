---
title: "ADR-0006: REST with a contract-first OpenAPI 3.1 spec"
type: adr
status: accepted
tags: [adr, api, openapi]
created: 2026-09-27
updated: 2026-09-27
related: [api, 0001-rust-backend, 0002-angular-material-frontend]
---

# ADR-0006: REST with a contract-first OpenAPI 3.1 spec

## Context
A Rust backend and an Angular frontend need a shared, typed contract. The domain is resource-oriented (tenants, services, microservices, DR items) with a few explicit state transitions (workflow gates, plan approval, recovery step transitions).

## Options considered
- **REST + OpenAPI:** broad tooling, client generation for Angular, easy to audit and cache.
- **GraphQL:** flexible reads, but it adds complexity for the command-style transitions and for authorization per field.

## Decision
Use REST, described by **`api/openapi.yaml`** (OpenAPI 3.1), maintained **contract-first**:
- State transitions are explicit action sub-resources (`/approve`, `/decision`, `/transition`, `/complete`), not generic field updates.
- Errors use RFC 9457 problem details. Updates use JSON merge patch. Concurrency uses optimistic `ETag`/`If-Match`.
- Live recovery updates use Server-Sent Events.
- The tenant is taken from the JWT `tenant_id` claim, not from the URL. Paths stay short, and a client can't address another tenant by changing the path.

## Consequences
- The Angular client and models are generated from the spec (the generator is still to be chosen).
- The backend must stay conformant to the spec. Add a contract test in CI once the backend exists.
- Changes to the spec are reviewed like code. Breaking changes need a new major path version.
