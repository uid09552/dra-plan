---
title: "ADR-0003: Open Markdown knowledge base for documentation"
type: adr
status: accepted
tags: [adr, docs]
created: 2026-09-27
updated: 2026-09-27
related: [README]
---

# ADR-0003: Open Markdown knowledge base for documentation

## Context
Documentation should be versioned with the code, readable without special tools, and usable by both humans and AI agents.

## Decision
Keep all project docs in `docs/` as Markdown with YAML frontmatter and `[[wiki-links]]`, following the conventions in the docs index (`docs/README.md`).

## Consequences
- Works in Git hosting, Obsidian/Foam, MkDocs and plain editors.
- Frontmatter lets scripts and AI agents index docs by type, status and tags.
- Authors have to keep frontmatter and links up to date.
