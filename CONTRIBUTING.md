# Contributing to dra-workflow

Thank you for helping improve dra-workflow. This guide explains how to set up the project, what we
expect from a change and how to get it merged.

## Ways to contribute

- **Report a bug**: open an issue with steps to reproduce, the expected and the actual behavior, and
  versions (browser, OS, commit).
- **Suggest a feature**: open an issue describing the problem first. Check
  [docs/requirements/feature-status.md](docs/requirements/feature-status.md) for what is planned.
- **Improve docs**: fixes to `docs/`, this file or the README are always welcome.
- **Send code**: for anything larger than a small fix, please open an issue first so we can agree on
  the approach before you invest time.

Be respectful and constructive in issues, reviews and discussions.

## Development setup

Prerequisites: Docker with Compose, `make`, Rust (stable, ≥ 1.85), Node.js 24.

```bash
cp .env.example .env     # replace every "change-me"; never commit .env
make db-up               # PostgreSQL on 127.0.0.1:5434
make run                 # backend in dev mode (mocked auth, tenant "demo") on :8090
make ui-install && make ui   # Angular dev server on :4200
```

`make help` lists all targets. `make app-up` starts everything in containers instead.

## Making a change

1. Fork the repository and create a branch from `main` (for example `fix/scenario-merge`).
2. Keep the change focused; one topic per pull request.
3. Update tests and docs in the same change as the behavior.
4. Run `make check` and make sure it passes.
5. Open a pull request that explains *what* changed and *why*, and links the issue.

### Checks that must pass

| Area | Command |
|------|---------|
| Backend format and lints | `make lint` (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`) |
| Backend tests (needs PostgreSQL) | `make test` |
| Frontend formatting, tests, build | `make ui-lint`, `make ui-test`, `make ui-build` |
| OpenAPI | `npx @redocly/cli lint api/openapi.yaml` (part of `make check`) |

### Commit messages

Write short, imperative subject lines ("Add risk matrix drag and drop"), with a body that explains
the reason when it is not obvious.

## Conventions

The full conventions live in [CLAUDE.md](CLAUDE.md) and in [docs/](docs/README.md). The most important ones:

**Domain**
- Follow the model in [docs/domain/dr-plan-model.md](docs/domain/dr-plan-model.md): tenant → IT service →
  component → DR items. Integrity rules and workflow gates live in the Rust domain layer.
- AI output is advisory: never apply it to a plan without explicit user confirmation.
- DR plans are sensitive: never log plan contents, tokens or personal data; never store secrets in plans.

**API**
- Contract first: change [`api/openapi.yaml`](api/openapi.yaml) first, then the implementation. Keep it lint-clean.
- State transitions are action endpoints (`/decision`, `/approve`, `/complete`), not generic PATCHes.

**Backend (Rust)**
- Hexagonal architecture sliced by feature: `src/features/<feature>/{domain,application,infra,api}`.
  The domain depends on nothing else; only `bootstrap.rs` knows concrete adapters.
- DTOs in the API layer (camelCase, `deny_unknown_fields`), validation in the DTO and in the domain.
- Every repository call takes a `TenantContext`; queries are parameterized; no `unwrap()` outside tests.
- Schema changes are new files in `backend/migrations/` (never edit an applied migration).
- Tests: pure unit tests for the domain, integration tests against PostgreSQL in `backend/tests/`.

**Frontend (Angular)**
- Standalone components, signals, `inject()`, the new control flow; Angular Material components,
  Bootstrap only for grid and utilities.
- Every visible text goes through the `t` pipe, with keys in both `core/i18n/en.ts` and `de.ts`.
- Colors from Material tokens (`var(--mat-sys-…)`) so dark mode works; no CDN dependencies.
- Editing follows the existing pattern: right-click menu (`ContextMenu`) plus a visible button for
  keyboard and touch users, `EditDialog` / `ConfirmDialog`, `injectMutation()`.
- Accessibility (WCAG 2.1 AA) matters: the UI is used under stress.

**Documentation**
- Docs in `docs/` use the frontmatter described in [docs/README.md](docs/README.md).
- Record significant decisions as ADRs in `docs/adr/` using [docs/templates/adr.md](docs/templates/adr.md).
- Keep [docs/requirements/feature-status.md](docs/requirements/feature-status.md) current.

## Reporting security issues

Please do **not** open a public issue for a security vulnerability. Report it privately through the
repository host's private vulnerability reporting (for example GitHub "Report a vulnerability" under
*Security*) and include steps to reproduce. We will acknowledge the report and coordinate a fix and
disclosure with you.

## License

dra-workflow is licensed under the [Apache License, Version 2.0](LICENSE). By submitting a
contribution you agree that it is licensed under the same license (see section 5 of the license), and
you confirm that you have the right to submit it. Do not include code you cannot license this way.
