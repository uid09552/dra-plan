---
title: Feature Status
type: requirements
status: active
tags: [requirements, status, mvp]
created: 2026-09-27
updated: 2026-09-27
related: [README, plan-authoring, frontend, backend, api, mcp]
---

# Feature Status

Implementation status of the [[requirements/README|feature requirements]] (sections 1–19). Legend:
**done** = usable end to end (API, UI, MCP where it makes sense); **partial** = parts exist (often
API only, or a simplified version); **open** = not started.

Open and partial items are also the backlog in `todo.txt`.

## Overview

| # | Requirement | Status | What exists | Gaps |
|---|-------------|--------|-------------|------|
| 1 | Guided DR process | done | Guide tab: 7 stages as breadcrumb (Service & components → BIA → Dependencies & RTO/RPO → Scenarios → Mitigations → Plan & documentation → Test & improve), guiding questions, required/optional markers, next-step hint, all 15 workflow gates with complete/reopen; readiness score per service; data is saved per change | Templates per organization/service type; role-based task lists |
| 2 | Scenario brainstorming & mind map | done | Mind map (service → category → scenario → sub-scenario), 15 categories, rule-based suggestions (`GET /services/{id}/scenario-suggestions`, MCP `suggest_scenarios`), accept/reject/edit/merge/add/sub-scenarios/delete via right-click menu, drag & drop to re-parent or re-categorize | LLM-based suggestions (AI provider still open); a single "DR object" view per scenario (data is linked, but shown in several places) |
| 3 | Business impact analysis | partial | MTPD/RTO/RPO, minimum operating level, regulatory requirements, impact rating per category and time window (tenant settings) | Business processes, data criticality, customer impact as separate category |
| 4 | Service & dependency mapping | partial | Optional components (default component = whole service), add/edit/delete in the guide and by right-click in the map; dependencies per component (component, infrastructure, platform, external service, supplier, personnel), visual map in restore order, RTO conflicts, single points of failure, restore order suggestion | Business-process layer; impact propagation |
| 5 | Risk & scenario assessment | done | Likelihood × impact (1–4), 4×4 risk matrix with drag & drop rating, DR decision, recovery capability (implementation status, last tested), gap check, gate warnings | Scenario side-by-side comparison |
| 6 | Recovery strategy | done | Measures tab (and guide stage 5): one measure covers one or more scenarios; strategy types from the catalog, guided questions, estimated RTO/RPO vs. objective (gap check, accepted gaps), implementation status, last test date | – |
| 7 | DR plan builder | done | Handbook generated from structured data (live draft preview + immutable approved versions, Markdown export), mitigation status section | Recovery locations and failback as explicit sections |
| 8 | Recovery procedures / runbooks | partial | Runbooks with steps (owner, duration, verification, decision points) in API and MCP (`add_mitigation`), shown in the handbook | Runbook editor in the UI; step evidence/comments/approval |
| 9 | DR activation & incident guidance | partial | Recovery runs (declare, step transitions, phases, close) in API/MCP with SSE; read-only recovery overview in the UI | Guided execution UI, escalation, decision log view |
| 10 | Task & responsibility management | partial | DR roles, role assignments with deputies and escalation level (guide stage 6: add/edit/delete), action items with owners and due dates (API) | RACI matrix; recovery teams; action items UI |
| 11 | Communication management | partial | Communication rules (trigger, audience, channel, responsible and authorizing role, interval, message template) editable in guide stage 6; shown in the handbook | Stakeholder groups; communication history during a recovery |
| 12 | DR testing & exercises | partial | DR tests with type, scenario, results, measured RTO/RPO (API); readiness uses them (last exercise, RTO compliance, untested scenarios) | UI to plan, run and report exercises |
| 13 | Recovery validation | partial | Step verification in recovery runs; phase `recovered` → `reconstituting` | Guided validation chain (technical → data → business) and acceptance |
| 14 | Evidence & documentation | partial | Audit log of all changes (who/what/when), decisions and approvals stored with author | File/screenshot evidence; evidence package |
| 15 | Lessons learned | partial | Lessons learned on closing a run, action items (API) | After-action review UI; retest loop |
| 16 | DR readiness dashboard | done | Cockpit: readiness %, critical gaps, attention, completed steps, next actions, last exercise, RTO compliance, plans requiring review, untested scenarios; per service (`GET /readiness`, MCP `get_readiness`) | – |
| 17 | Compliance & framework mapping | done | Requirement → what to do → evidence → status for NIST SP 800-34, BSI 200-4, IT-Grundschutz (Compliance tab, `GET /services/{id}/compliance`) | ISO 22301 / ISO 27001 mappings |
| 18 | Reporting & document generation | partial | DR plan (Markdown) | PDF/Word/Excel; BIA, exercise, incident, management and audit reports |
| 19 | Core data model | partial | Tenant → IT service → microservice → DR items; scenarios, strategies, runbooks, tests, runs, action items, audit | Business process; incident as its own object; findings separate from action items |

## MVP (section 20)

| MVP item | Status |
|----------|--------|
| Guided DR workflow | done |
| Scenario mind map + brainstorming | done |
| BIA | done (MVP scope) |
| Service/dependency mapping | done (MVP scope) |
| RTO/RPO | done |
| Recovery strategy | done |
| DR plan builder | done |
| Recovery procedures/runbooks | partial (no UI editor) |
| Roles & task assignment | partial (roles/deputies editable; no task assignment) |
| DR testing/exercises | partial (no UI) |
| Evidence/documentation | partial |
| Readiness dashboard | done |
| Findings & corrective actions | partial (no UI) |
| PDF/Word DR plan generation | open (Markdown only) |
