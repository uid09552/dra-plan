# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project

**dra-workflow** is a workflow-driven, AI-assisted tool for **IT service Disaster Recovery (DR)**. It covers two phases:

1. **Plan authoring:** a guided workflow walks the user through creating a Disaster Recovery Plan (DRP) for an IT service: scope, dependencies, RTO/RPO, recovery steps, contacts and tests. AI assists by suggesting content, spotting gaps and checking consistency.
2. **Recovery execution:** during an actual disaster, the tool guides responders through the recovery runbook step by step, tracks progress and decisions, and gives AI support for troubleshooting.

Methodology: **NIST SP 800-34 Rev. 1** and **BSI-Standard 200-4** (with IT-Grundschutz DER.4 and CON.3). See [docs/domain/standards-mapping.md](docs/domain/standards-mapping.md).

### Core domain (read before touching the model or the workflow)
- **Hierarchy:** Tenant → IT service → component → DR items. "Component" is the UI/docs term; the API and database call it `microservice`. Components are optional: every service has a default component (`is_default`) standing for the whole service ([ADR-0011](docs/adr/0011-optional-components.md)). The BIA, scenarios, roles, communication, tests and plan versions attach to the IT service. Dependencies, objectives, strategies, backups and runbooks attach to the component. See [docs/domain/dr-plan-model.md](docs/domain/dr-plan-model.md).
- **Authoring workflow:** 15 steps in 5 phases (Understand → Assess → Design → Document → Validate & Maintain), each step with a gate. Scenario selection must be agreed *before* recovery design. See [docs/workflows/plan-authoring.md](docs/workflows/plan-authoring.md).
- **Recovery workflow:** Activation → Recovery → Reconstitution. It runs against an immutable, approved plan snapshot. See [docs/workflows/recovery-execution.md](docs/workflows/recovery-execution.md).
- **Integrity rules** such as microservice RTO ≤ service RTO ≤ MTPD live in the Rust domain layer.
- **Multi-tenant:** every table carries `tenant_id`.

Background and design live in [docs/](docs/README.md). Read the relevant docs before changing behavior.
Product requirements: [docs/requirements/README.md](docs/requirements/README.md); what is built: [docs/requirements/feature-status.md](docs/requirements/feature-status.md) (keep it current).

## Tech stack

| Layer    | Technology                                          |
|----------|-----------------------------------------------------|
| Backend  | Rust (stable, single crate `dra-server`): tokio, axum 0.8, tower / tower-http, sqlx 0.8, clap, dotenvy, validator, tracing |
| Database | PostgreSQL 17 (docker compose, port 5434); forced row-level security, composite tenant foreign keys, audit trigger |
| AI access | REST API + MCP endpoint `POST /mcp` (use-case based tools) |
| Identity / gateway | Keycloak 26.2 (organizations = tenants, `tenant` claim) + APISIX 3.16 (token validation); docker network `drp`, settings in `.env` |
| Frontend | Angular 21 (standalone, signals, zoneless) + Angular Material 3; Bootstrap grid/utilities for layout only |
| Docs     | Markdown + YAML frontmatter knowledge base in `docs/` |

Still open, so ask before assuming (see [docs/overview/open-questions.md](docs/overview/open-questions.md)): AI provider, backend JWT validation (Keycloak is set up, but the backend still uses the `--dev-mode` mock), deployment target, Angular client generator.

## Repository layout

```
api/openapi.yaml     REST contract (source of truth)
backend/             Rust crate dra-server: src/{shared,features/<feature>,mcp}, migrations/, tests/, Dockerfile
frontend/            Angular app: src/app/{core,layout,features}; Dockerfile + nginx/ (run stage)
deploy/              postgres init (app role not superuser → RLS), keycloak realm import, apisix config
docker-compose.yml   PostgreSQL + Keycloak + APISIX on network `drp`; secrets from `.env` (template `.env.example`, never commit `.env`)
docs/                Knowledge base: overview, architecture, ADRs, domain, workflows, requirements
todo.txt             User's task list; remove items once they are done and verified (open items only)
LICENSE, NOTICE       Apache License 2.0; README.md (project overview), CONTRIBUTING.md (contributor guide)
```

## Commands

Everything goes through the root `Makefile` (`make help`):

- `make up` / `make down`: whole stack (Postgres :5434, Keycloak :8180, APISIX :9080); `make token`: access token of `demo-user`
- `make app-up`: whole stack plus containerized backend and UI (nginx) on `http://localhost:8080`; `make docker-build` builds the images (see [docs/architecture/deployment.md](docs/architecture/deployment.md))
- `make run-gateway`: backend on `0.0.0.0:8090` so APISIX can reach it (see [docs/architecture/identity-gateway.md](docs/architecture/identity-gateway.md))
- `make run`: Postgres + backend in dev mode (mocked auth, tenant `demo`, auto-migrate) on `127.0.0.1:8090`
- `make ui`: Angular dev server on `http://localhost:4200` (proxies `/api` and `/mcp` to the backend)
- `make test`: backend unit + integration tests (needs Postgres; `DRA_TEST_DATABASE_URL`)
- `make lint`: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`
- `make ui-test`, `make ui-lint`, `make ui-build`: frontend tests (Vitest), Prettier, production build
- `make check`: all of the above plus `npx @redocly/cli lint` (OpenAPI)
- Backend CLI: `dra-server {serve|migrate|seed-catalog|export|healthcheck} --help`

## Conventions

### General
- The tool must be usable **during a disaster**. Prefer robustness, offline tolerance and clear state over cleverness. A DR plan must stay readable and exportable even when the AI service or the tool itself is degraded.
- AI output is **advisory**. Never auto-apply AI suggestions to a plan or run a recovery action without explicit user confirmation, and keep track of what came from AI and what the user wrote.
- Treat DR plans as sensitive: they contain infrastructure details, contacts and possibly credentials references. Don't log plan contents, and never store secrets in plans (store references only).

### API
- Contract-first: change `api/openapi.yaml` first, then the implementation. Keep it lint-clean. See [docs/architecture/api.md](docs/architecture/api.md).
- State transitions are action endpoints (`/approve`, `/decision`, `/transition`, `/complete`), not generic PATCHes.

### Rust backend
Full reference: [docs/architecture/backend.md](docs/architecture/backend.md) ([ADR-0007](docs/adr/0007-hexagonal-feature-sliced-backend.md)).

**Architecture: hexagonal (ports and adapters), sliced by feature.**
- Code is organized **by feature first, then by layer**: `src/features/<feature>/{domain,application,infra,api}`. Features follow the OpenAPI tags (for example `services`, `microservices`, `scenarios`, `runbooks`, `recovery_runs`, `ai`). Cross-cutting code goes in `src/shared/`.
- **Dependency rule:** `api → application → domain ← infra`. The domain depends on nothing else. Only `bootstrap.rs` (the composition root) knows the concrete adapters.
  - `domain/`: entities, value objects (newtypes such as `Minutes`, `Rating`, `TenantId`), integrity rules, and the **port traits** (repositories, AI client, clock, notifier). This layer has no tokio, axum, serde, sqlx or other framework types, and it is pure and unit-testable.
  - `application/`: use cases (commands and queries) that orchestrate ports, enforce authorization and workflow gates, and own transactions. It depends on port traits only.
  - `infra/`: **adapters** that implement the ports, such as database repositories (engine still open), the AI provider client and export renderers. This is the only layer that talks to the database or the network.
  - `api/`: axum handlers, routes, **DTOs** and mapping. This is the only layer that knows HTTP, and it stays thin (it parses, calls a use case and maps the result).
- A feature never reaches into another feature's `infra/` or `api/`. It uses the other feature's `application` interface or a port.

**Frameworks:** `tokio` (runtime), `axum` (HTTP), `tower` / `tower-http` (middleware: tracing, request-id, timeout, body limit, CORS, security headers, compression), `clap` (CLI and settings), `dotenvy` (the maintained fork of `dotenv`, which loads `.env` in development), and `tracing` (structured logs). Do not add another web framework or runtime.

**Configuration: CLI-first, no config files.**
- The backend is **one binary with clap subcommands**, for example `serve`, `migrate`, `seed-catalog`, `export` (write a plan version to a file for offline use) and `healthcheck`.
- **Every** setting is a clap argument with an `env = "DRA_…"` fallback. The precedence is CLI argument > environment variable > `.env` (loaded by dotenvy at startup, for development only) > default.
- Validate the whole configuration at startup and fail fast with a clear message. Secrets (database URL, AI key, JWKS/OIDC settings) are only passed as arguments or environment variables. Wrap them in `secrecy::SecretString`, and never log or `Debug`-print them.

**DTOs and validation.**
- Never serialize domain types or database rows directly. `api/dto.rs` holds request and response DTOs (serde `camelCase`, matching `api/openapi.yaml`) and converts them with `TryFrom`/`From` into and out of domain types.
- Request DTOs use `#[serde(deny_unknown_fields)]`.
- Validation happens at two levels:
  1. **Syntactic, in the DTO** (lengths, ranges, formats, enums), with a validation crate. It returns `422` problem+json with `issues[]`.
  2. **Semantic, in the domain.** Constructors and value objects return `Result` (parse, don't validate), and integrity rules and workflow gates produce `Issue`s with a `ruleId`.
- Errors: domain errors (`thiserror`) → application errors → one `IntoResponse` mapping to RFC 9457 problem+json in `shared/`. Never leak internals or stack traces.

**Security: OWASP Top 10 is a baseline, not optional.**
- **Access control:** the tenant comes only from the validated JWT `tenant_id` claim, carried in a `TenantContext` extractor. Every repository method takes a `TenantContext`, so there is no unscoped query. An id belonging to another tenant returns `404`. Role and DR-role checks live in the application layer.
- **Authentication:** verify the JWT signature using JWKS, with an algorithm allowlist, and check `iss`, `aud`, `exp` and `nbf`. Reject tokens without a tenant claim.
- **Injection:** use parameterized queries only and never build SQL from strings. Treat **LLM output as untrusted input**: AI proposals go through the same DTO and domain validation before anything is applied.
- **Misconfiguration:** secure defaults, a strict CORS allowlist, security headers, request-size limits, timeouts, maximum page size, and rate limits (AI endpoints in particular).
- **SSRF:** outbound URLs (AI provider, identity provider) come from configuration only, never from user input.
- **Logging:** use `tracing` with a request id, and log security events (authentication or authorization failures, approvals, DR declarations). Never log plan contents, tokens or personal data.
- **Supply chain:** commit `Cargo.lock`, and run `cargo audit` and `cargo deny` in CI.

**Code rules.**
- Use `thiserror` in library code and `anyhow` only in `main.rs` and `bootstrap.rs`. No `unwrap()`/`expect()` outside tests and startup.
- Handlers are `async` and must never block. Put blocking work in `spawn_blocking`.
- Tests:
  - `domain` code gets pure unit tests.
  - `application` code is tested against in-memory fake ports.
  - `api` code is tested with `tower::ServiceExt::oneshot`.
  - `infra` code gets integration tests against a real database.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must pass.

### MCP
- `/mcp` exposes **use-case tools**, not one tool per endpoint. New tools call application use cases (never
  repositories) and live in `backend/src/mcp/tools.rs`. See [docs/architecture/mcp.md](docs/architecture/mcp.md).

### Angular frontend
- Standalone components, signals (`rxResource`, `input()`), `inject()`, and the control flow (`@if`, `@for`). File names without `.component` (`shell.ts`, class `Shell`).
- Angular Material for all components. Bootstrap is used **only** for grid and utility classes (no Bootstrap components); don't use Bootstrap class names such as `.container` for your own layout. See [ADR-0009](docs/adr/0009-ui-layout-i18n-theming.md).
- Every user-visible text goes through the `t` pipe with keys in both `core/i18n/en.ts` and `de.ts`.
- Use Material system tokens (`var(--mat-sys-…)`) for colors so dark mode works; no hard-coded colors outside `light-dark()`.
- Fonts and icons are bundled (offline). Don't add CDN dependencies.
- Accessibility (WCAG 2.1 AA) matters because the UI is used under stress.
- **Editing UX:** the service **Guide** is the one place to enter and edit plan data; don't add parallel
  read-only tabs for the same data. Lists, tables and maps support **right-click** (`ContextMenu` in
  `core/ui`) with edit/add/delete, plus a ⋮ button or the context-menu key for keyboard/touch users. Use
  `EditDialog`/`ConfirmDialog` for edits and deletes, and drag & drop where it maps naturally (mind map,
  risk matrix). Mutations go through `injectMutation()` (busy state, inline 422 issues).

### Documentation
- Every doc in `docs/` follows the frontmatter conventions in [docs/README.md](docs/README.md).
- Record significant decisions as ADRs in `docs/adr/` using [docs/templates/adr.md](docs/templates/adr.md).
- Update docs in the same change as the behavior they describe.
