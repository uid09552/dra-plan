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
| [templates/](templates/) | Templates for new docs |

### Key documents
- [[vision]]: what the tool is and why it exists
- [[open-questions]]: undecided topics
- [[architecture-overview]]: high-level system design
- [[glossary]]: DR terminology (RTO, RPO, BIA, and others)
- [[dr-plan-model]]: structure of a DR plan
- [[plan-authoring]]: guided creation workflow
- [[recovery-execution]]: in-disaster recovery workflow

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
