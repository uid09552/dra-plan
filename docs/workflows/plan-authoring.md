---
title: Plan Authoring Workflow
type: workflow
status: draft
tags: [workflow, authoring, ai]
created: 2026-09-27
updated: 2026-09-27
related: [dr-plan-model, recovery-execution]
---

# Plan Authoring Workflow

A guided, resumable wizard that produces a [[dr-plan-model]] for one IT service.

| Step | User provides | AI support |
|------|---------------|------------|
| 1. Service identification | Name, owner, description, criticality | Suggests a tier based on the description |
| 2. Business impact | Impact over time, RTO/RPO | Explains the terms, checks plausibility against the tier |
| 3. Dependencies | Upstream/downstream services, infrastructure, suppliers | Proposes likely dependencies, flags RTO conflicts |
| 4. Scenarios | Relevant disaster scenarios | Suggests common scenarios (site loss, ransomware, cloud region outage, and others) |
| 5. Contacts & roles | Responsible people, escalation | Detects missing roles |
| 6. Recovery runbook | Ordered steps | Drafts steps, spots missing verifications or ordering issues |
| 7. Validation & failback | Checks, return to normal | Suggests checks |
| 8. Review | Consolidated plan | Completeness and consistency report |
| 9. Approval | Reviewer sign-off | — |

## Rules
- Every step can be saved and resumed.
- AI suggestions are shown as proposals. The user accepts, edits or rejects each one, and the origin is recorded.
- Validation runs continuously. Blocking issues prevent approval, warnings don't.
