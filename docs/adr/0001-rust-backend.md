---
title: "ADR-0001: Rust for the backend"
type: adr
status: accepted
tags: [adr, backend, rust]
created: 2026-09-27
updated: 2026-09-27
related: [architecture-overview]
---

# ADR-0001: Rust for the backend

## Context
The backend hosts the DR plan domain model, the workflow engine and the AI integration. It has to be reliable and easy to deploy in a failure domain separate from the protected services.

## Decision
Implement the backend in Rust as a Cargo workspace.

## Consequences
- Strong typing for the domain model and the workflow states. Compile-time guarantees reduce runtime failures.
- A single static binary makes independent or standby deployment easy.
- The web framework and database crates are still open ([[open-questions]]).
