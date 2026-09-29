---
title: Open Questions
type: overview
status: active
tags: [decisions, backlog]
created: 2026-09-27
updated: 2026-09-29
related: [architecture-overview]
---

# Open Questions

Decisions that are not made yet. When one is resolved, record an ADR in `docs/adr/` and remove it here.

| # | Question | Options / notes |
|---|----------|-----------------|
| 3 | AI provider and integration | Hosted LLM API (for example Claude), self-hosted model. The `AiProvider` port exists; applying accepted proposals is not implemented yet. |
| 4 | Multi-organization tenant selection | Backend JWT validation maps Keycloak's `tenant` claim and `dra-viewer`/`dra-admin` realm roles when dev mode is disabled. Users with several organization memberships still need an explicit tenant-selection flow. |
| 5 | Deployment target | Container/Kubernetes, single binary, on-prem. **The tool must stay available when the protected services are down.** |
| 6 | Offline / degraded mode | Static export, local replica, PWA offline cache |
| 8 | Angular client generator | openapi-generator (typescript-angular), ng-openapi-gen, orval. The UI uses a small hand-written typed client for now. |
| 9 | Minimum-operating-level modeling | Free text for now. Should it become structured (list of functions that must work first)? |
| 10 | Tenant-wide shared platforms | Should shared infrastructure (database cluster, IdP) be its own IT service with a DR plan, or only a dependency? |
| 11 | Impact-rating scale | Tenant-configurable categories and time windows. Which defaults? |
| 12 | Business processes | The requirements put business processes above IT services (BIA, dependency mapping). A separate entity or an attribute of the IT service? |
| 13 | Document formats | PDF/Word/Excel generation for plans and reports: server-side (e.g. Typst, LibreOffice) or client-side? Markdown only today. |
| 14 | Localized validation messages | Gate and validation issue messages come from the backend in English; localize there (Accept-Language) or map `ruleId` in the UI? |

## Resolved
- Standards basis: NIST SP 800-34 + BSI 200-4, see [[0005-standards-basis]]
- Multi-tenancy and hierarchy: see [[0004-tenant-service-microservice-hierarchy]]
- Backend framework and architecture: axum/tokio/tower, hexagonal and sliced by feature, CLI-first, see [[0007-hexagonal-feature-sliced-backend]]
- Plan versioning: immutable snapshots, in review → approved / back to draft, previous approval retired
- Identity provider and gateway: Keycloak organizations as tenants, APISIX, see [[0010-keycloak-organizations-apisix]]
- Persistence: PostgreSQL with forced row-level security and composite tenant foreign keys
- AI agent access: use-case based MCP endpoint, see [[0008-use-case-based-mcp]]
- UI layout, i18n and theming: see [[0009-ui-layout-i18n-theming]]
- API style: REST + OpenAPI 3.1, contract-first, see [[0006-rest-openapi-contract-first]] and [[api]]
