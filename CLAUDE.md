# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project

**dra-workflow** is a workflow-driven, AI-assisted tool for **IT service Disaster Recovery (DR)**. It covers two phases:

1. **Plan authoring:** a guided workflow walks the user through creating a Disaster Recovery Plan (DRP) for an IT service: scope, dependencies, RTO/RPO, recovery steps, contacts and tests. AI assists by suggesting content, spotting gaps and checking consistency.
2. **Recovery execution:** during an actual disaster, the tool guides responders through the recovery runbook step by step, tracks progress and decisions, and gives AI support for troubleshooting.

Background and design live in [docs/](docs/README.md). Read the relevant docs before changing behavior.

## Tech stack

| Layer    | Technology                                          |
|----------|-----------------------------------------------------|
| Backend  | Rust (stable, Cargo workspace)                      |
| Frontend | Angular (standalone components, signals) + Angular Material (Material 3) |
| Docs     | Markdown + YAML frontmatter knowledge base in `docs/` |

Still open, so ask before assuming (see [docs/overview/open-questions.md](docs/overview/open-questions.md)): web framework, database, AI provider/integration, authentication, deployment target.

## Repository layout (planned)

```
backend/     Rust Cargo workspace (API server, domain, workflow engine, AI integration)
frontend/    Angular application (Angular Material UI)
docs/        Knowledge base: overview, architecture, ADRs, domain, workflows
```

Update this section once the scaffolding exists.

## Commands

Not scaffolded yet. Once it is, record the real commands here, for example:

- Backend: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`
- Frontend: `npm ci`, `ng serve`, `ng test`, `ng lint`, `ng build`

## Conventions

### General
- The tool must be usable **during a disaster**. Prefer robustness, offline tolerance and clear state over cleverness. A DR plan must stay readable and exportable even when the AI service or the tool itself is degraded.
- AI output is **advisory**. Never auto-apply AI suggestions to a plan or run a recovery action without explicit user confirmation, and keep track of what came from AI and what the user wrote.
- Treat DR plans as sensitive: they contain infrastructure details, contacts and possibly credentials references. Don't log plan contents, and never store secrets in plans (store references only).

### Rust backend
- Keep domain logic (DR plan model, workflow state machine) free of web and DB framework types.
- Use `thiserror` for library errors and `anyhow` only at binary edges. No `unwrap()`/`expect()` outside tests and startup.
- `cargo fmt` and `cargo clippy -D warnings` must pass.

### Angular frontend
- Standalone components, signals, and the new control flow (`@if`, `@for`).
- Use Angular Material components and theming. Don't add a second UI library.
- Accessibility (WCAG 2.1 AA) matters because the UI is used under stress.

### Documentation
- Every doc in `docs/` follows the frontmatter conventions in [docs/README.md](docs/README.md).
- Record significant decisions as ADRs in `docs/adr/` using [docs/templates/adr.md](docs/templates/adr.md).
- Update docs in the same change as the behavior they describe.
