---
okf_version: "0.2"
title: Documentation Index
description: Knowledge base of dra-workflow, a guided tool for IT service disaster recovery planning and recovery execution (NIST SP 800-34, BSI-Standard 200-4) - vision, architecture, ADRs, domain model, workflows, requirements and a UI tour.
type: index
status: active
tags: [index, okf, disaster-recovery]
resource: https://github.com/uid09552/dra-plan
created: 2026-09-27
updated: 2026-09-27
---

# dra-workflow Knowledge Base

This folder is an open, tool-agnostic knowledge base: plain Markdown files with YAML frontmatter, linked with `[[wiki-links]]`. You can read it on GitHub/GitLab, open it as an Obsidian or Foam vault, or build the MkDocs site (`make docs-serve`, see [Publishing](#publishing)). No proprietary format is required.

The folder is also an **OKF (Open Knowledge Format) v0.2 bundle**: this file is the bundle root (`okf_version` in the frontmatter), every doc carries a `title`, `type`, `status` and `tags`, and [[log]] records notable changes. Agents can load it as files without the site.

## Map

| Area | Purpose |
|------|---------|
| `overview/` | Vision, scope, open questions |
| `guide/` | User-facing guides, such as the UI tour with screenshots |
| `architecture/` | System structure and components |
| `adr/` | Architecture Decision Records |
| `domain/` | DR concepts, glossary, DR plan data model |
| `workflows/` | Plan-authoring and recovery-execution workflows |
| `requirements/` | Product feature requirements and their implementation status |
| `templates/` | Templates for new docs (not published on the site) |
| `assets/` | Images, such as `assets/screenshots/` |

### Key documents
- [[vision]]: what the tool is and why it exists
- [[open-questions]]: undecided topics
- [[ui-tour]]: the main screens, with screenshots
- [[requirements/README|Feature requirements]] and [[feature-status]]: what the product must do, and what is built
- [[architecture-overview]]: high-level system design
- [[api]]: backend REST API (contract: `api/openapi.yaml`)
- [[deployment]]: container images (build/run stages), nginx, compose profile `app`
- [[backend]]: backend architecture (hexagonal, sliced by feature, CLI configuration, OWASP controls)
- [[mcp]]: MCP interface for AI agents (use-case tools at `/mcp`)
- [[frontend]]: Angular cockpit (layout, i18n, dark mode)
- [[identity-gateway]]: Keycloak (organizations = tenants) and APISIX gateway
- [[glossary]]: DR terminology (RTO, RPO, BIA, and others), including BSI terms
- [[dr-plan-model]]: data model (tenant → IT service → microservice → DR items)
- [[standards-mapping]]: how NIST SP 800-34 and BSI 200-4 map onto the tool
- [[scenario-catalog]]: default scenarios, recovery strategies and test types
- [[plan-authoring]]: the 15-step guided plan creation workflow
- [[recovery-execution]]: in-disaster recovery workflow

### Decisions (ADRs)
- [[0001-rust-backend]] · [[0002-angular-material-frontend]] · [[0003-markdown-knowledge-base]]
- [[0004-tenant-service-microservice-hierarchy]] · [[0005-standards-basis]] · [[0006-rest-openapi-contract-first]] · [[0007-hexagonal-feature-sliced-backend]]
- [[0008-use-case-based-mcp]] · [[0009-ui-layout-i18n-theming]] · [[0010-keycloak-organizations-apisix]]
- [[0011-optional-components]]

### Structure

```
docs/
├── README.md                 this index (OKF bundle root)
├── log.md                    update log
├── overview/                 vision, open questions
├── guide/                    UI tour
├── architecture/             system design
├── adr/                      NNNN-*.md decision records
├── domain/                   data model, glossary, standards mapping, catalogs
├── workflows/                plan authoring, recovery execution
├── requirements/             feature requirements and status
├── assets/screenshots/       UI screenshots (PNG, 1440 × 900)
└── templates/                adr.md, doc.md
```

Diagrams use Mermaid code blocks, which render on GitHub/GitLab, Obsidian and the MkDocs site.

## Conventions

### Frontmatter (required)

```yaml
---
title: Human readable title
description: One sentence on what the doc covers   # optional, recommended (OKF)
type: overview | architecture | adr | domain | workflow | guide | index | template
status: draft | active | deprecated | superseded
tags: [lowercase, keywords]
created: YYYY-MM-DD
updated: YYYY-MM-DD
related: [other-doc-slug]   # optional
---
```

### Naming and linking
- File names are lowercase `kebab-case.md`. The file name (without `.md`) is the doc's slug.
- Link between docs with `[[slug]]`, `[[slug|text]]` or `[[slug#anchor]]`. Slugs must be unique across `docs/`;
  the two `README.md` files are linked by path (`[[README]]`, `[[requirements/README|…]]`).
- Images live in `assets/` and are linked with relative Markdown links (`![alt](../assets/screenshots/x.png)`).
- ADRs are named `NNNN-short-title.md` (for example `0001-rust-backend.md`) and are never renumbered.
- An accepted ADR is not edited. Supersede it with a new ADR and set `status: superseded`.

### Writing
- One topic per file, and short files are better than long ones.
- Write **Open question:** for anything undecided, and add it to [[open-questions]].
- Add an entry to [[log]] when you create, restructure or significantly revise a doc.

## Publishing

`mkdocs.yml` at the repository root builds this folder into a static site with Material for MkDocs.
The hook `tools/mkdocs/wikilinks.py` turns `[[wiki-links]]` into page links and fails the strict build
on unknown or ambiguous slugs.

```bash
make docs-serve   # live preview on http://127.0.0.1:8000
make docs         # strict build into site/
```

Both targets install the pinned toolchain from `requirements-docs.txt` into `.venv-docs/`. New docs
must be added to `nav` in `mkdocs.yml`.
