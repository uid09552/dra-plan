---
title: Identity and API Gateway (Keycloak, APISIX)
type: architecture
status: active
tags: [architecture, security, keycloak, apisix, multi-tenancy]
created: 2026-09-27
updated: 2026-09-27
related: [0010-keycloak-organizations-apisix, backend, api, 0004-tenant-service-microservice-hierarchy]
---

# Identity and API Gateway

Local stack in `docker-compose.yml` (all containers on the internal network **`drp`**), configured from
`.env` (template: `.env.example`). Decision: [[0010-keycloak-organizations-apisix]].

```
browser / MCP client ──► APISIX :9080 ──(valid token only)──► dra-server (host :8090)
        │                   │ JWKS / discovery (http://keycloak:8080, network drp)
        └── login ───────► Keycloak :8180 (realm `dra`, organizations = tenants)
                            │
                        PostgreSQL :5434 (databases `dra`, `keycloak`)
```

| Service | Container | Host port | Notes |
|---------|-----------|-----------|-------|
| PostgreSQL 17 | `dra_postgres` | 127.0.0.1:5434 | DBs `dra` (app role `dra`, not superuser → RLS applies) and `keycloak` |
| Keycloak 26.2 | `dra_keycloak` | 127.0.0.1:8180 | Realm imported from `deploy/keycloak/realm-dra.json` on start |
| APISIX 3.16 | `dra_apisix` | 127.0.0.1:9080 | Standalone mode, routes in `deploy/apisix/apisix.yaml` |

## Keycloak realm

- **Organizations = tenants.** The organization alias equals the tenant slug, and the organization attribute
  `tenant_id` holds the tenant key. The seed data is organization `demo` with member `demo-user`.
- **Clients** (ids and secrets from `.env`):
  - `DRA_CLIENT_ID` (default `dra-workflow`): the application client. Authorization code + PKCE for the UI;
    direct access grants are enabled for development and tests (`make token`). An audience mapper adds
    itself to `aud`.
  - `APISIX_CLIENT_ID` (default `apisix`): the gateway client (service account only).
- **Tenant mapper:** both clients carry an *Organization Membership* mapper with claim name **`tenant`**,
  organization id and attributes enabled. The `organization` client scope is a default scope, so the
  claim is always issued:

  ```json
  "tenant": { "demo": { "id": "e865fcfa-…", "tenant_id": ["demo"] } },
  "organization": ["demo"]
  ```
- Tokens use the public issuer `KEYCLOAK_PUBLIC_URL` (`http://localhost:8180/realms/dra`). Containers use
  the backchannel `http://keycloak:8080` (`KC_HOSTNAME_BACKCHANNEL_DYNAMIC`).
- Secrets in the realm file are `${ENV}` placeholders, resolved by Keycloak at import. The import runs only
  if the realm does not exist yet (`IGNORE_EXISTING`). After changing `.env`, update the clients in the
  admin console, or drop the realm and restart Keycloak.

## APISIX routes

| Route | Paths | Auth |
|-------|-------|------|
| `dra-health` | `GET /api/v1/health` | public |
| `dra-api` | `/api/*`, `/mcp` | `openid-connect` in `bearer_only` mode: JWT signature via JWKS, issuer, expiry. Invalid or missing token → 401. The token is forwarded to the backend. |

The upstream is the backend on the host (`DRA_BACKEND_HOST:DRA_BACKEND_PORT`, default
`host.docker.internal:8090`). Start it with `make run-gateway`, which listens on `0.0.0.0:8090` so the
container can reach it. `make run` stays on `127.0.0.1` for UI-only work.

## Status and next steps

- The backend still uses the **dev-mode mock** for authentication. It does not read the token yet. Next step:
  a JWT `Authenticator` adapter that validates the token (JWKS, `iss`, `aud` = `DRA_CLIENT_ID`) and maps the
  `tenant` claim (organization alias or `tenant_id` attribute → tenant slug) to the internal tenant id.
  Users in several organizations will need an explicit tenant selection.
- The UI does not log in yet (OIDC code flow + PKCE against the `DRA_CLIENT_ID` client).
- Production: TLS everywhere, disable direct access grants, and run Keycloak with `start` (not `start-dev`).

## Commands

```bash
make up          # PostgreSQL + Keycloak + APISIX
make run-gateway # backend reachable by APISIX
make token       # access token of demo-user
curl -H "Authorization: Bearer $(make -s token)" http://localhost:9080/api/v1/services
```
