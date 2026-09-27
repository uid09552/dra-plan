---
title: "ADR-0007: Hexagonal, feature-sliced Rust backend (tokio, axum, tower, clap)"
type: adr
status: accepted
tags: [adr, backend, rust, architecture]
created: 2026-09-27
updated: 2026-09-27
related: [backend, 0001-rust-backend, 0006-rest-openapi-contract-first]
---

# ADR-0007: Hexagonal, feature-sliced Rust backend (tokio, axum, tower, clap)

## Context
The backend holds safety-relevant domain logic (integrity rules, workflow gates, recovery state) that must be testable without infrastructure. Several technical choices are still open (the database and the AI provider) and must stay swappable. The tool must be easy to operate in degraded conditions.

## Decision
- **Hexagonal architecture** (ports and adapters): `domain` (pure, with port traits) ← `application` (use cases) ← `api` (axum and DTOs), with `infra` adapters implementing the ports, wired in a single composition root.
- **Layered by feature:** `features/<feature>/{domain,application,infra,api}`, one feature per OpenAPI tag.
- **Frameworks:** tokio, axum, tower / tower-http, clap, dotenvy (the maintained fork of dotenv), tracing.
- **CLI-first configuration:** one binary with subcommands (`serve`, `migrate`, `seed-catalog`, `export`, `healthcheck`). All settings are clap arguments with a `DRA_*` environment fallback. No config files.
- **DTOs at the edge**, two-level validation (DTO syntax, then domain semantics), and RFC 9457 errors.
- **OWASP Top 10** controls as a baseline (see the backend architecture doc).

## Consequences
- Domain rules and gates are unit-testable without a database or HTTP. Use cases are tested against in-memory fakes.
- The database engine and AI provider can be decided or changed later by writing new adapters.
- Feature slices keep changes local but cost some boilerplate (DTO ↔ domain mapping, port traits).
- A CLI-first binary works well with containers and 12-factor deployment, and the `export` subcommand provides an offline emergency path.
