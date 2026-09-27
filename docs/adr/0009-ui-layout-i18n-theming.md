---
title: "ADR-0009: UI layout, runtime i18n and theming"
type: adr
status: accepted
tags: [adr, frontend, ui, i18n]
created: 2026-09-27
updated: 2026-09-27
related: [frontend, 0002-angular-material-frontend]
---

# ADR-0009: UI layout, runtime i18n and theming

## Context
The UI needs a responsive cockpit, English/German switching at runtime (defaulting to the browser
language) and a dark mode. It must stay usable during a disaster, possibly without internet access.
ADR-0002 allows Angular Material as the only component library; Bootstrap was requested for responsive layout.

## Decision
- **Bootstrap only for layout:** include `bootstrap-grid.css` and `bootstrap-utilities.css` (grid, spacing,
  flex, display). No Bootstrap components, JavaScript or reboot, so Material stays the single component
  library and styles don't conflict. Avoid Bootstrap class names that collide with layout classes of our
  own (for example `.container`).
- **Runtime i18n** with a small signal-based service and typed dictionaries (`en.ts`, `de.ts`; the German file
  must contain every English key). Angular's built-in i18n was not chosen because it is compile-time only.
- **Theming** with Material 3 `mat.theme` and CSS `color-scheme: light dark`; a class on `<html>` pins
  light or dark.
- **Self-hosted fonts and icons** (`@fontsource/roboto`, `material-symbols`).

## Consequences
- The UI switches language and theme instantly, without reloading the page.
- Backend-generated texts (validation messages) are still English; catalog labels and exports are
  localized through `Accept-Language`.
- The initial bundle is around 800 kB raw (about 175 kB compressed). The production budget warning is set to 1 MB.
