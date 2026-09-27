---
title: "ADR-0010: Keycloak organizations as tenants, APISIX as API gateway"
type: adr
status: accepted
tags: [adr, security, keycloak, apisix, multi-tenancy]
created: 2026-09-27
updated: 2026-09-27
related: [identity-gateway, 0004-tenant-service-microservice-hierarchy, 0006-rest-openapi-contract-first]
---

# ADR-0010: Keycloak organizations as tenants, APISIX as API gateway

## Context
The API expects the tenant from the access token ([[0006-rest-openapi-contract-first]]). We need an
identity provider that can model tenants, and a gateway in front of the REST API and `/mcp`.

## Decision
- **Keycloak** (26.x) is the identity provider. Each tenant is a **Keycloak organization**
  (alias = tenant slug, attribute `tenant_id`).
- An **Organization Membership mapper** on the realm clients writes the claim **`tenant`** (organization
  alias → `{ id, tenant_id }`). The `organization` scope is a default client scope.
- **APISIX** (standalone, declarative YAML) validates bearer tokens with the `openid-connect` plugin
  (JWKS) for `/api/*` and `/mcp`, and forwards them to the backend.
- The client ids and secrets of the realm client and the APISIX client come from `.env`.
- All containers run on the internal Docker network `drp`.

## Consequences
- Tenant membership is managed in Keycloak (inviting users to organizations), not in the app.
- The claim format differs from the originally planned flat `tenant_id` claim. The backend's JWT
  adapter maps the `tenant` object to the internal tenant, and has to handle users in several organizations.
- Defense in depth: the gateway rejects invalid tokens, and the backend must still validate them itself,
  because it can be reached directly as well.
