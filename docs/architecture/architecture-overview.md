---
title: Architecture Overview
type: architecture
status: draft
tags: [architecture]
created: 2026-09-27
updated: 2026-09-27
related: [0001-rust-backend, 0002-angular-material-frontend, open-questions]
---

# Architecture Overview

> Draft. Components are logical. Frameworks and storage are still open ([[open-questions]]).

```
┌─────────────────────────────┐
│  Angular + Material UI      │  Plan wizard · Plan editor · Recovery console
└──────────────┬──────────────┘
               │ HTTP/JSON (+ push for live recovery updates)
┌──────────────▼──────────────┐
│  Rust backend               │
│  ├─ api          HTTP layer, auth                     │
│  ├─ domain       DR plan model, validation rules      │
│  ├─ workflow     authoring + recovery state machines  │
│  ├─ ai           provider abstraction, prompts        │
│  ├─ export       Markdown/PDF export                  │
│  └─ storage      persistence adapters                 │
└──────────────┬──────────────┘
       ┌───────┴────────┐
       ▼                ▼
   Database        AI provider (external/self-hosted)
```

## Components

- **domain:** Pure Rust types for [[dr-plan-model]] and validation (for example "RTO of a service must be ≥ RTO of its dependencies"). No I/O.
- **workflow:** Explicit state machines for [[plan-authoring]] and [[recovery-execution]], with persisted, resumable state.
- **ai:** A trait-based provider interface so the backend can switch between hosted and self-hosted models. Every AI output is labeled as a suggestion and needs user acceptance.
- **export:** Produces standalone plan documents for offline and crisis use.
- **api:** Thin adapter layer that maps HTTP to domain and workflow calls.

## Cross-cutting concerns
- **Availability:** The DR tool must not share a failure domain with the services it protects.
- **Security:** Plans are sensitive. They need encryption at rest, role-based access and audit logs.
- **Auditability:** Recovery runs produce an immutable timeline of steps, decisions and actors.
