---
title: Open Questions
type: overview
status: active
tags: [decisions, backlog]
created: 2026-09-27
updated: 2026-09-27
related: [architecture-overview]
---

# Open Questions

Decisions that are not made yet. When one is resolved, record an ADR in `docs/adr/` and remove it here.

| # | Question | Options / notes |
|---|----------|-----------------|
| 1 | Rust web framework | axum, actix-web |
| 2 | Persistence | PostgreSQL, SQLite (simple, portable, good for offline copies) |
| 3 | AI provider and integration | Hosted LLM API (for example Claude), self-hosted model, pluggable provider trait. Consider data-sensitivity rules. |
| 4 | Authentication / authorization | OIDC/SSO, local accounts. Roles: author, reviewer, responder, admin. |
| 5 | Deployment target | Container/Kubernetes, single binary, on-prem. **The tool must stay available when the protected services are down.** |
| 6 | Offline / degraded mode | Static export, local replica, PWA offline cache |
| 7 | Plan versioning and approval | Draft → review → approved, with version history |
| 8 | Standards alignment | ISO 22301, ISO/IEC 27031, NIST SP 800-34 |
| 9 | API style | REST + OpenAPI, GraphQL. Consider generating Angular client types from the API spec. |
| 10 | Multi-tenancy | Single organization or multi-tenant |
