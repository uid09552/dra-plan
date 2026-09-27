---
title: Container Images and Local Deployment
type: architecture
status: active
tags: [deployment, docker, nginx]
created: 2026-09-27
updated: 2026-09-27
related: [architecture-overview, backend, frontend, identity-gateway, open-questions]
---

# Container Images and Local Deployment

Two images, each built in a **build stage** and shipped in a small **run stage**. The deployment target
itself is still open ([[open-questions]] #5).

| Image | Dockerfile | Build stage | Run stage |
|-------|-----------|-------------|-----------|
| `dra-server` | `backend/Dockerfile` | `rust:1.94-bookworm`, `cargo build --release --locked` (BuildKit caches for registry and `target/`) | `gcr.io/distroless/cc-debian12:nonroot`: only the binary, runs as non-root, ~60 MB |
| `dra-ui` | `frontend/Dockerfile` | `node:24-alpine`, `npm ci` + `ng build` (production) | `nginxinc/nginx-unprivileged:1.29-alpine`: port 8080, non-root, ~90 MB |

## Backend image

- Entry point `dra-server`, default command `serve`. Settings come only from `DRA_*` environment
  variables or arguments ([[backend]]); the image sets `DRA_LISTEN_ADDR=0.0.0.0:8090` and JSON logs.
- Migrations are embedded in the binary; set `DRA_MIGRATE_ON_START=true` or run `dra-server migrate`.
- `HEALTHCHECK` runs `dra-server healthcheck` (no shell or curl in distroless).
- Backend JWT validation is not implemented yet: without `DRA_DEV_MODE=true` every API call returns 401
  (fail closed). Dev mode mocks authentication — never expose it on a shared network.

## UI image

nginx serves the Angular build and proxies `/api/` and `/mcp` to `BACKEND_URL` (default
`http://backend:8090`), so browser and API share one origin (no CORS). The config is a template
(`frontend/nginx/default.conf.template`) rendered by the image's envsubst entrypoint at start-up.

- SPA fallback to `index.html` (deep links work); `index.html` is `no-cache`, hashed bundles are cached for a year.
- SSE of recovery runs (`/api/v1/recovery-runs/{id}/stream`) is proxied without buffering.
- Security headers: strict CSP (`script-src 'self'`, styles `'self' 'unsafe-inline'` for Angular Material),
  `nosniff`, `no-referrer`, `frame-ancestors 'none'`. Critical-CSS inlining is disabled in the production
  build because it needs an inline event handler.
- `GET /healthz` for container health checks.

## docker compose

The profile `app` adds both containers to the stack on network `drp`:

```bash
make app-up        # = docker compose --profile app up -d --build
                   # → http://localhost:9080 (gateway, Keycloak login) or :8080 (UI directly, no login)
make docker-build  # build dra-server:local and dra-ui:local only
make down          # stop everything (incl. app containers), keep data
```

The backend container connects as role `dra` (not a superuser, so row-level security applies). APISIX
still routes to the backend on the host; set `DRA_BACKEND_HOST=backend` in `.env` to route it to the
container ([[identity-gateway]]).

## Troubleshooting

- **Keycloak exits with `password authentication failed for user "keycloak"`** (APISIX then reports
  the Keycloak container as unhealthy): PostgreSQL runs its init scripts only when the volume is created,
  so a later change of `KEYCLOAK_DB_PASSWORD` in `.env` never reached the database. `make up` / `make app-up`
  run `make db-sync` first, which applies the current value (idempotent `deploy/postgres/10-keycloak.sh`).
  With plain `docker compose up`, run `make db-sync` once before.
- Use Docker Compose v2 (`docker compose`, as in the Makefile). The legacy `docker-compose` 1.x is end of life.
