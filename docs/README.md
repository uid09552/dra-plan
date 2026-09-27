---
title: Documentation Index
type: index
status: active
tags: [index]
created: 2026-09-27
updated: 2026-09-27
---

# dra-workflow Knowledge Base

This folder is an open, tool-agnostic knowledge base: plain Markdown files with YAML frontmatter, linked with `[[wiki-links]]`. You can read it on GitHub/GitLab, open it as an Obsidian or Foam vault, or publish it with MkDocs. No proprietary format is required.

## Map

| Area | Purpose |
|------|---------|
| [overview/](overview/) | Vision, scope, open questions |
| [architecture/](architecture/) | System structure and components |
| [adr/](adr/) | Architecture Decision Records |
| [domain/](domain/) | DR concepts, glossary, DR plan data model |
| [workflows/](workflows/) | Plan-authoring and recovery-execution workflows |
| [requirements/](requirements/) | Product feature requirements and their implementation status |
| [templates/](templates/) | Templates for new docs |

### Key documents
- [[vision]]: what the tool is and why it exists
- [[open-questions]]: undecided topics
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

### Structure

```
docs/
├── README.md                 this index
├── overview/                 vision, open questions
├── architecture/             system design
├── adr/                      NNNN-*.md decision records
├── domain/                   data model, glossary, standards mapping, catalogs
├── workflows/                plan authoring, recovery execution
└── templates/                adr.md, doc.md
```

Diagrams use Mermaid code blocks, which render on GitHub/GitLab, Obsidian and MkDocs (with a plugin).

## Conventions

### Frontmatter (required)

```yaml
---
title: Human readable title
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
- Link between docs with `[[slug]]`. Slugs must be unique across `docs/`.
- ADRs are named `NNNN-short-title.md` (for example `0001-rust-backend.md`) and are never renumbered.
- An accepted ADR is not edited. Supersede it with a new ADR and set `status: superseded`.

### Writing
- One topic per file, and short files are better than long ones.
- Write **Open question:** for anything undecided, and add it to [[open-questions]].
