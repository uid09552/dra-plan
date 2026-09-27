---
title: Frontend Architecture
type: architecture
status: active
tags: [architecture, frontend, angular, ui]
created: 2026-09-27
updated: 2026-09-27
related: [0002-angular-material-frontend, 0009-ui-layout-i18n-theming, api]
---

# Frontend Architecture

Angular 21 (standalone components, signals, zoneless change detection) with Angular Material 3 in
`frontend/`. Decisions: [[0002-angular-material-frontend]], [[0009-ui-layout-i18n-theming]].

## Structure

```
frontend/src/app/
├── app.config.ts / app.routes.ts   # providers, lazy routes under the Shell
├── core/
│   ├── api/          # typed REST client (Api) and models
│   ├── http/         # interceptors (Accept-Language, problem+json → snackbar), injectMutation()
│   ├── markdown/     # safe Markdown block parser for the handbook preview (no innerHTML)
│   ├── ui/           # ContextMenu (right-click), EditDialog / ConfirmDialog
│   ├── i18n/         # I18n service, `t` pipe, en.ts / de.ts dictionaries
│   ├── theme/        # Theme service (system / light / dark)
│   └── prefs/        # guarded localStorage access
├── layout/shell/     # cockpit: toolbar + left navigation + account menu
└── features/         # dashboard, services (list, detail, create dialog), recovery,
                      # catalog (knowledge base), settings, profile
```

## Cockpit

- **Top bar:** navigation toggle, product name, language menu, theme menu, and the account menu on the right
  (profile, settings, sign out).
- **Left navigation:** Cockpit (dashboard), IT services, Recovery, Knowledge base, Settings. At or above
  960 px it is docked and can be collapsed; below that it overlays the content and closes after navigating.
- **Responsive layout:** Bootstrap **grid and utility classes only** (`bootstrap-grid.css`,
  `bootstrap-utilities.css`); all components are Angular Material. Tables scroll horizontally on small
  screens.

## Pages

| Route | Content |
|-------|---------|
| `/dashboard` | DR readiness ("How ready are we?"): score, critical gaps, attention, completed steps, RTO compliance, plans requiring review, untested scenarios, last exercise, next actions across services, services by readiness, active recoveries |
| `/services` | Searchable list with workflow progress, plan status, next review; create dialog |
| `/services/:id` | Tabs: **Guide**, **Scenarios** (mind map / risk matrix, measures per scenario), **Measures** (recovery strategies, each covering one or more scenarios), **Dependencies** (map), **Compliance**, **Handbook** (live draft), **Plan versions** (submit, approve, export) |
| `/recovery` | Active recovery runs with RTO clock and progress |
| `/catalog` | Workflow steps with NIST / BSI references, strategy types, scenario templates |
| `/settings` | Language, theme, tenant settings |
| `/profile` | Current (dev) user and tenant memberships |

## Service page

The **Guide** is the single place to enter and edit plan data. There is no separate workflow tab: each
guide stage shows the workflow steps (gates) it covers, with their issues and complete/reopen buttons.

| Stage | Workflow steps | Content |
|-------|----------------|---------|
| 1 Service & components | 1 | Name, description, business/technical owner (from contacts, "new contact" inline), protection requirement, impact level; optional components (add/edit/delete, the default component stands for the whole service) |
| 2 Business impact (BIA) | 3 | MTPD/RTO/RPO, minimum operating level, impact per category and time window |
| 3 Dependencies & RTO/RPO | 2, 7 | Dependencies per component; default objectives per component |
| 4 Scenarios | 4–6 | Counts, suggestions, link to the mind map |
| 5 Mitigations | 8–9 | Same `MeasuresView` as the Measures tab: uncovered scenario × component pairs, measures with scenarios, implementation status, last test, selection (gap acceptance) |
| 6 Plan & documentation | 10–12 | Roles with deputies, communication rules (incl. who authorizes failover), handbook preview, compliance, submit |
| 7 Test & improve | 13–15 | Untested scenarios, last exercise, RTO compliance |

Each stage has guiding questions, required/optional hints on fields and a next-step hint (first
blocking issue, else "complete the steps", else the next stage). A readiness score is shown on top.

**Editing conventions** (all lists and maps):

- **Right-click** (or the context-menu key, or the ⋮ button in tables for keyboard/touch users) opens a
  menu with *edit*, *add* and *delete*. Edits open `EditDialog`; deletes ask in `ConfirmDialog`.
- **Components** (API: microservices) are managed with `ComponentEditor` (dialogs for add/edit/delete and
  add dependency) from the guide's component table and from the dependency map (right-click a component
  node, or the empty area to add one). The default component cannot be deleted.
- **Scenarios** can name affected components in their edit dialog; empty means the whole service.
- **Mind map:** drag a scenario onto another scenario to make it a sub-scenario, onto a category to move
  it there, onto the service node to detach it. Pointer events, so it works with touch. `Delete` key on a
  focused node deletes it (with confirmation).
- **Risk matrix:** drag a scenario chip into a cell to set likelihood × impact (`PATCH /scenarios/{id}`);
  unrated scenarios wait in a tray below the matrix.
- Validation issues (422) are shown inline above the form; other errors in a snackbar.

## Cross-cutting

- **Language:** English/German at runtime. The default comes from the browser language (`de-*` → German),
  a manual choice is remembered. The `Accept-Language` header is sent to the backend, which localizes
  catalog labels and the handbook export.
- **Dark mode:** follows the OS (`color-scheme: light dark` with the Material 3 theme); the user can pin
  light or dark.
- **Offline-friendly assets:** fonts (Roboto) and icons (Material Symbols) are bundled, with no CDN
  dependency, because the UI must keep working during an incident.
- **Login:** behind the gateway, APISIX runs the OIDC flow and keeps the session; the UI holds no tokens.
  A `401` from the API (or a failing lazy chunk) reloads the page once, which leads to the Keycloak login
  and back; a repeated attempt within 30 s is suppressed. *Sign out* links to `/logout`
  ([[identity-gateway]]).
- **Dev setup:** `make run` (backend) and `make ui` (Angular dev server with a proxy for `/api` and `/mcp`).
