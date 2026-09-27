---
title: DR Plan Data Model
type: domain
status: draft
tags: [domain, model]
created: 2026-09-27
updated: 2026-09-27
related: [glossary, plan-authoring, recovery-execution]
---

# DR Plan Data Model

> Draft conceptual model. It will be refined into Rust types in the `domain` crate.

```
ItService
 ├─ name, description, owner, criticality tier
 ├─ dependencies: [ItService | Component | Supplier]
 └─ DrPlan (versioned)
     ├─ status: draft | in_review | approved | retired
     ├─ objectives: RTO, RPO, (MTPD)
     ├─ scope & assumptions
     ├─ disaster scenarios: [Scenario]
     ├─ contacts & escalation: [Contact / Role]
     ├─ prerequisites: backups, standby resources, access (secret *references* only)
     ├─ runbook: [RecoveryStep]
     │    └─ RecoveryStep: order, title, instructions, responsible role,
     │                     expected duration, verification, depends_on
     ├─ validation & return-to-normal (failback) steps
     └─ test history: [DrTest]

RecoveryRun
 ├─ plan version, trigger/scenario, started/ended, mode (real | test)
 ├─ step states: pending | in_progress | done | skipped | failed
 └─ timeline: [Event (actor, timestamp, note, AI suggestion accepted?)]
```

## Validation rules (examples)
- The RTO of a service must be ≥ the RTO of each critical dependency.
- Every runbook step has a responsible role and a verification.
- An approved plan has at least one contact per role and a test within the review period.

**Open question:** Which standard template (ISO 22301 / NIST SP 800-34) should the default structure follow? See [[open-questions]].
