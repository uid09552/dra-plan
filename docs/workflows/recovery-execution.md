---
title: Recovery Execution Workflow
type: workflow
status: draft
tags: [workflow, recovery, ai]
created: 2026-09-27
updated: 2026-09-27
related: [dr-plan-model, plan-authoring]
---

# Recovery Execution Workflow

Turns an approved DR plan into an interactive runbook during a real disaster or a DR test.

## Flow
1. **Declare:** Start a recovery run: select the service(s) and scenario, and mark it as real or test. The current approved plan version is pinned to the run.
2. **Mobilize:** Show the contacts and escalation path, and assign roles.
3. **Execute:** Work through the runbook steps in dependency order. Each step can be started, completed (with verification), skipped (with a reason) or failed.
4. **Assist:** AI helps interpret errors, suggests troubleshooting, and summarizes status for stakeholders. It never executes actions.
5. **Validate:** Run the service verification checks.
6. **Close:** Record the outcome and RTO/RPO achieved versus target, and produce the post-incident report and lessons learned. These feed back into [[plan-authoring]].

## Requirements
- Every action is written to an immutable timeline (actor, timestamp, note).
- Multiple responders can see live progress.
- Works with minimal dependencies. An exported plan is the fallback if the tool is unavailable.
