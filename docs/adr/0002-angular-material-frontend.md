---
title: "ADR-0002: Angular with Angular Material for the UI"
type: adr
status: accepted
tags: [adr, frontend, angular]
created: 2026-09-27
updated: 2026-09-27
related: [architecture-overview]
---

# ADR-0002: Angular with Angular Material for the UI

## Context
The UI is dominated by forms and workflows (multi-step wizards, editors, checklists), and it has to be accessible and consistent.

## Decision
Use Angular (standalone components, signals) with Angular Material (Material 3 theming) as the only component library.

## Consequences
- Material Stepper, Forms and CDK cover the wizard and runbook UX well.
- A consistent, accessible design system with little custom styling.
- The team follows Angular's opinionated structure and tooling (Angular CLI).
