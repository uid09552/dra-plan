# Proposal

## Why

Runbooks (workflow step 9) can only be written through the REST API or the MCP tool `add_mitigation`; the
Guide's Mitigations stage just shows a hint (`guide.mitigations.runbookHint`). Plan authors without an AI
client cannot complete step 9, and the step model lacks two attributes requirement §8 asks for:
**prerequisites** and the **evidence** a responder must produce.

## What Changes

- New **Runbooks section** in the Guide's Mitigations stage, grouped by DR scenario, listing one row per
  affected component × scenario pair with its runbook (step count, critical path, RTO warning) or a
  "create runbook" action where it is missing. Replaces the API/MCP hint.
- New **runbook step editor** panel: steps grouped in the phases Activation → Recovery → Reconstitution,
  drag & drop reordering, right-click / ⋮ context menu (edit, insert above/below, delete), `EditDialog`
  for step fields, inline 422 issues (for example a reorder that violates `dependsOnStepIds`), critical
  path and RTO comparison in the header.
- Runbook create/edit/delete (title, description, scenario, optional strategy) from the section.
- Two **new optional step fields** across database, domain, API and MCP:
  - `prerequisites` (free text): conditions outside the runbook that must hold before the step starts.
  - `evidenceRequired` (free text): what evidence a responder must record when completing the step.
- "Expected result" in §8 maps to the existing `verification` field; the UI labels it
  "Expected result / verification". No rename in the API.
- The new fields flow into plan snapshots, recovery-run step states (read-only) and the Markdown handbook.
  Snapshots written before this change stay readable (fields default to empty).
- `add_mitigation` (MCP) accepts `prerequisites` and `evidenceRequired` per step.

Not in this change: recording evidence, comments and approvals during a recovery run, and the
`APPROVAL_REQUIRED` gate. Those follow in `add-recovery-step-records`. Also out of scope: file
attachments, structured prerequisite checklists, a link from measure cards to their runbook.

## Capabilities

### New Capabilities
- `runbook-authoring`: authoring runbooks and their steps for a component and DR scenario, including step
  attributes, ordering and dependency rules, critical path, and the Guide editor.

### Modified Capabilities
<!-- None: no specs exist yet in openspec/specs/. -->

## Impact

- **API:** `api/openapi.yaml` — `RunbookStepFields` gains `prerequisites` and `evidenceRequired`;
  `RunStepState` exposes both read-only. Additive, not breaking.
- **Database:** new migration adding two nullable `text` columns to `runbook_step`.
- **Backend:** `features/runbooks` (domain `StepInput`/`RunbookStep`, DTOs, infra queries),
  `features/plans` (snapshot DTO, Markdown handbook), `features/recovery_runs` (`RunStepState` copy),
  `mcp/tools.rs` (`add_mitigation` schema).
- **Frontend:** new runbook models and API methods in `core/api`, new runbook list and step editor under
  `features/services/tabs/`, Mitigations stage in `service-guide.ts`, i18n keys in `en.ts` and `de.ts`.
- **Docs:** `docs/domain/dr-plan-model.md`, `docs/workflows/plan-authoring.md` (step 9),
  `docs/requirements/feature-status.md` (§8), `docs/guide/` user guide if it covers the Mitigations stage,
  screenshot.
