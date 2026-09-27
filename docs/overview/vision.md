---
title: Vision and Scope
type: overview
status: draft
tags: [vision, scope]
created: 2026-09-27
updated: 2026-09-27
related: [open-questions, plan-authoring, recovery-execution]
---

# Vision and Scope

## Problem
DR plans for IT services are often incomplete, outdated, or stored in formats that are hard to use under pressure. When a disaster happens, responders need a clear, current, step-by-step guide, not a 60-page document.

## Vision
A workflow-driven tool that:
1. **Guides plan creation** through a structured, step-by-step workflow ([[plan-authoring]]), with AI assistance that suggests content, detects gaps and inconsistencies, and asks the right questions.
2. **Supports recovery** during an incident ([[recovery-execution]]) by turning the plan into an interactive, trackable runbook with AI-assisted troubleshooting.

## Target users
- **Service owners / IT architects:** author and maintain DR plans.
- **Operations / on-call engineers:** run recoveries.
- **DR / BCM coordinators:** oversee coverage, reviews and tests.
- **Auditors:** check plan completeness and test evidence.

## In scope (initial)
- DR plan authoring for individual IT services
- Service dependency capture
- RTO/RPO definition and consistency checks
- Guided recovery runbook execution with progress tracking and a timeline
- Plan export (for example Markdown/PDF) for offline availability

## Out of scope (initial)
- Automated execution of recovery actions against infrastructure
- Full Business Continuity Management (BCM) beyond IT services
- Monitoring and alerting (integrations may come later)

## Guiding principles
- **Usable in a crisis:** works under stress and degraded conditions.
- **Human in control:** AI advises, humans decide.
- **Plans stay portable:** export always works, and there is no lock-in.
