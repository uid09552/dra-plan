---
title: Identity and API Gateway (Keycloak, APISIX)
type: architecture
status: active
tags: [architecture, security, keycloak, apisix, multi-tenancy]
created: 2026-09-27
updated: 2026-09-29
related: [0010-keycloak-organizations-apisix, backend, api, 0004-tenant-service-microservice-hierarchy]
---

# Identity and API Gateway

Local stack in `docker-compose.yml` (all containers on the internal network **`drp`**), configured from
`.env` (template: `.env.example`). Decision: [[0010-keycloak-organizations-apisix]].

```
browser ──► APISIX :9080 ──(session cookie → bearer token)──► dra-server (`backend` :8090)
   │            │  └── pages, static assets ───────────────► UI nginx (`ui` :8080)
   │            │ OIDC client, JWKS, discovery (http://keycloak:8080, network drp)
  └── login ─► Keycloak via `/auth` (realm `dra`, organizations = tenants)
MCP client ─► APISIX :9080 /mcp (bearer token) ─────────────► dra-server
                            │
                        PostgreSQL :5434 (databases `dra`, `keycloak`)
```

| Service | Container | Host port | Notes |
|---------|-----------|-----------|-------|
| PostgreSQL 17 | `dra_postgres` | 127.0.0.1:5434 | DBs `dra` (app role `dra`, not superuser → RLS applies) and `keycloak` |
| Keycloak 26.2 | `dra_keycloak` | internal only | Browser endpoints proxied by APISIX under `/auth`; realm imported from `deploy/keycloak/realm-dra.json` on start |
| APISIX 3.16 | `dra_apisix` | 127.0.0.1:9080 | Standalone mode, routes in `deploy/apisix/apisix.yaml` |

## Keycloak realm

- **Organizations = tenants.** The organization alias equals the tenant slug, and the organization attribute
  `tenant_id` holds the tenant key. The seed data is organization `demo` with member `demo-user`.
- **Demo login:** use username `demo-user` (or `demo-user@demo.local`) and the current `DEMO_USER_PASSWORD`
  from `.env`. `demo` is the organization/tenant name, not a login username. The realm import uses
  `IGNORE_EXISTING`, so changing `DEMO_USER_PASSWORD` after the realm has been created does not reset the
  stored password; update the user credential in the Keycloak admin console if needed.
- **Demo roles:** `demo-user` has `dra-viewer`; `demo-admin` has `dra-admin`. Both also receive the
  Keycloak `account` client roles needed for the Account Console. Set `DEMO_ADMIN_PASSWORD` in `.env` to
  provision an admin password; otherwise set it through the admin console. The application profile link
  opens `http://localhost:9080/auth/realms/dra/account`.
- **API access:** APISIX asks Keycloak to authorize each HTTP method. `dra-viewer` may use `GET` and
  `HEAD`; `OPTIONS` preflight is handled without authentication. `dra-admin` may also use `POST`, `PUT`,
  `PATCH`, and `DELETE`. Backend access is not published as a host port; requests must pass through APISIX.
- **Login entry points:** start the application login at `http://localhost:9080` after `make app-up`; APISIX
  redirects to `/auth/realms/dra/`. The administrator console is `http://localhost:9080/auth/admin` and
  uses the `master` realm and `KEYCLOAK_ADMIN` credentials, not the demo user.
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
- Tokens use the public issuer `KEYCLOAK_PUBLIC_URL` (`http://localhost:9080/auth/realms/dra`). APISIX
  fetches discovery over the internal Docker network; the discovered OIDC endpoints use the public
  `/auth` route, which APISIX proxies back to Keycloak.
- Secrets in the realm file are `${ENV}` placeholders, resolved by Keycloak at import. The import runs only
  if the realm does not exist yet (`IGNORE_EXISTING`). The redirect/logout URLs of `DRA_CLIENT_ID`
  (`UI_URL` and `DRA_PUBLIC_URL`) are re-applied to an existing realm by `make keycloak-sync` (part of
  `make up` / `make app-up`, script `deploy/keycloak/sync-client.sh`). Other changes: admin console, or
  drop the realm and restart Keycloak.

## APISIX routes

APISIX is the **OIDC client for the browser** (backend-for-frontend): it runs the authorization code flow
with PKCE against `DRA_CLIENT_ID` (confidential), keeps the tokens in an encrypted session cookie
(`APISIX_SESSION_SECRET`) and forwards the access token to the backend. The UI therefore holds no tokens.
With backend dev mode disabled, the backend independently validates each JWT signature, issuer, audience,
expiry, tenant and API role; it does not trust role headers from the gateway.

| Route | Paths | Auth |
|-------|-------|------|
| `keycloak` | `/auth/*` | Proxies to Keycloak; APISIX strips the `/auth` prefix. |
| `dra-health` | `GET /api/v1/health` | public |
| `dra-mcp` | `/mcp` | bearer token only (`APISIX_CLIENT_ID`, JWKS): AI/MCP clients. Missing or invalid → 401. |
| `dra-api-options` | `/api/*` `OPTIONS` | CORS preflight, no user token required. |
| `dra-api-read` | `/api/*` `GET`, `HEAD` | OIDC token/session plus Keycloak role/method authorization (`dra-viewer` or `dra-admin`). |
| `dra-api-write` | `/api/*` `POST`, `PUT`, `PATCH`, `DELETE` | OIDC token/session plus `dra-admin` authorization. |
| `dra-ui-assets` | `*.js`, `*.css`, fonts, icons | public: no data, identical for all users (lazy chunks must load after a session expired) |
| `dra-ui` | everything else | session required, otherwise **302 to the Keycloak login** and back to the requested URL. Handles `/oidc/callback` and `/logout` (ends the Keycloak session, then returns to `DRA_PUBLIC_URL`). |

Upstreams: `DRA_BACKEND_HOST:DRA_BACKEND_PORT` (default `backend:8090`) and `DRA_UI_HOST:DRA_UI_PORT`
(default `ui:8080`), i.e. the containers of `make app-up`. For a backend on the host use
`DRA_BACKEND_HOST=host.docker.internal` and `make run-gateway` (listens on `0.0.0.0:8090`).

**UI behaviour:** a `401` from the API (session expired) makes the UI reload the page; behind the gateway
this starts the login and returns to the same page. A lazy page chunk that fails to load triggers the same
reload. A second attempt within 30 seconds is not repeated (no redirect loops, e.g. without a gateway).
The *Sign out* menu item links to `/logout`.

## Status and next steps

- The backend validates Keycloak JWT signature (JWKS), `iss`, `aud`, `exp`, the `tenant` claim and
  `realm_access.roles` when `DRA_DEV_MODE=false`. `dra-viewer` is read-only (`GET`, `HEAD`, `OPTIONS`);
  `dra-admin` can use all methods. Compose defaults to JWT validation; `make run` explicitly enables the
  local dev mock. Users in multiple organizations still need an explicit tenant-selection flow.
- Production: TLS everywhere, disable direct access grants, and run Keycloak with `start` (not `start-dev`).

## Commands

```bash
make app-up      # PostgreSQL + Keycloak + APISIX + backend and UI containers
                 # → http://localhost:9080 (login as demo-user, password DEMO_USER_PASSWORD from .env)
make up          # PostgreSQL + Keycloak + APISIX only (with DRA_BACKEND_HOST=host.docker.internal:
make run-gateway #   backend on the host, reachable by APISIX)
make keycloak-sync # re-apply client redirect/logout URLs from .env
make token       # access token of demo-user
curl -H "Authorization: Bearer $(make -s token)" http://localhost:9080/api/v1/services
```
