---
title: Knowledge Base Update Log
type: index
status: active
tags: [index, log, okf]
created: 2026-09-27
updated: 2026-10-11
related: [README]
---

# Knowledge Base Update Log

Notable changes to this knowledge base, newest first. Add an entry when a doc is created, restructured
or significantly revised.

## 2026-10-11

* **Revision**: [[dr-plan-model]]: runbook steps gain `prerequisites` and `evidence_required`
  (migration `0006`), copied into run step states.
* **Revision**: [[plan-authoring]] step 9 and [[ui-tour]]: the runbook editor in the Guide's
  Mitigations stage, with a screenshot.
* **Revision**: [[feature-status]] §8: runbook editor done; evidence, comments and approval during
  runs still open.

## 2026-09-27

* **Creation**: The knowledge base is published as a MkDocs site (`mkdocs.yml`, `make docs`,
  `make docs-serve`). `[[wiki-links]]` are resolved by the hook `tools/mkdocs/wikilinks.py`; `templates/` is not
  published.
* **Creation**: [[ui-tour]]: a screen-by-screen tour of the cockpit with screenshots in
  `assets/screenshots/`.
* **Revision**: [[README]]: OKF (Open Knowledge Format) v0.2 root frontmatter (`okf_version`,
  `description`, `resource`), links to the UI tour and this log, ADR-0011 added to the decisions list.
* **Revision**: [[feature-status]] and [[scenario-catalog]]: `[[README|…]]` links now name
  `requirements/README`, because two docs share the slug `README`.
