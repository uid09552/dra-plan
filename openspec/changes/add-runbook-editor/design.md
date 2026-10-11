# Design

## Context

The backend already supports runbooks: the `runbooks` feature has full CRUD for runbooks and steps,
`PUT /runbooks/{id}/steps/order`, insert by `position`, dependency validation (`validate_steps`,
`ensure_not_depended_on`) and `critical_path`. The workflow gate for step 9 (`workflow/domain.rs`) blocks
on `MISSING_RUNBOOK`, `EMPTY_RUNBOOK`, `STEP_WITHOUT_OWNER`, `STEP_WITHOUT_VERIFICATION` and
`RUNBOOK_EXCEEDS_RTO`. The frontend has no runbook models or API methods; the Mitigations stage renders
`MeasuresView` plus a hint. Plan versions store the snapshot as `jsonb` encoded through the API DTOs
(`PlanSnapshotDto`, `SNAPSHOT_FORMAT_VERSION = 1`), and recovery runs copy step fields into
`RunStepState`. Requirements: see `specs/runbook-authoring/spec.md`.

## Goals / Non-Goals

**Goals:**
- One place in the Guide to author runbooks end to end, without leaving the Mitigations stage.
- Add the two step attributes with no breaking change to the API, the snapshot or existing runs.

**Non-Goals:**
- Changing gate rules of workflow step 9 (the new fields are optional and not gated).
- Enforcing phase order (an activation step after a recovery step stays allowed, as today).
- A runbook view outside the Guide (the handbook preview already shows runbooks read-only).

## Decisions

### Two nullable text columns, not a structured model
`runbook_step.prerequisites text` and `runbook_step.evidence_required text`, validated in the DTO
(max 5,000 characters, like `verification`) and normalized with `non_blank` in the domain.
*Alternatives:* a prerequisite checklist table (would allow ticking items during a run) and a structured
evidence spec (kind + description). Both were rejected for now: prerequisites usually describe conditions
outside the tool, and a required checklist adds friction under stress. Free text can later be split into
items without losing data.

### Keep `verification` as the API name for "expected result"
§8's "expected result" is what `verification` already means ("how to confirm the step succeeded"), and
the run transition already requires `verificationPassed`. Renaming would break the API, MCP and snapshot
for no behavioral gain. Only the UI label changes to "Expected result / verification".

### Snapshot stays at format version 1
The new fields are additive. `RunbookStepDto` reads them with `#[serde(default)]` and omits them when
absent (`skip_serializing_none`), so old snapshots decode unchanged and new snapshots stay readable by
the same version. `RunStepState` gains the two fields; the run-step instantiation copies them, and the
`run_step_state` table gets the same two nullable columns so a run does not depend on re-reading the
snapshot.
*Alternative:* bump the format version with an upgrade step. Not needed for optional fields.

### Runbooks section as its own component, master-detail in the Mitigations stage
A new `runbooks-view.ts` renders below `MeasuresView` in the Mitigations stage: groups per DR scenario,
rows per affected component (the same scenario × component pairs the gate uses). Clicking a row (or
"create") switches the section to `runbook-editor.ts`, a full-width panel with a back action; the open
runbook id is kept in the `runbook` query parameter so a reload or a shared link reopens it.
*Alternatives:* a dialog (too small for long runbooks), nesting under measure cards (misses runbooks
without a strategy; rejected during exploration).

The scenario × component pairs come from the data the Guide already loads (scenarios with DR decision,
affected components, default component). Runbooks are fetched with `GET /services/{id}/runbooks` and
steps per opened runbook with `GET /runbooks/{id}/steps`.

### Step list in execution order with phase separators
Steps are shown in `seq` order. A phase header is inserted wherever the phase changes, so a well-formed
runbook reads as three lanes. Phase is edited in the step dialog; dragging never changes the phase.
This keeps a single source of truth (`seq`) instead of per-lane ordering the backend does not model.

### Drag and drop with CDK, plus keyboard moves
`@angular/cdk/drag-drop` (already a dependency through Material) reorders the list and sends the full id
list to `PUT …/steps/order`. The context menu adds "Move up" / "Move down" so reordering works without a
pointer (WCAG 2.1.1). On a 422 the editor reloads the stored order and shows the issue through
`injectMutation()`.

### Step dialog through `EditDialog`
Fields: phase (select), title, instructions (textarea, Markdown), owner role (select from tenant roles),
expected duration (number), expected result / verification (textarea), prerequisites (textarea),
required evidence (textarea), dependencies (multiselect of *earlier* steps only), decision point
(checkbox), approval role (select). Offering only earlier steps prevents most order violations up front;
the server stays authoritative. Updates send `If-Match` with the step's version.

### Gate issues shown on rows
Workflow issues carrying `entity` `runbook` / `runbook_step` are shown as an icon with tooltip on the
matching row (for example `STEP_WITHOUT_OWNER`), so authors see what blocks step 9 where they fix it.

## Risks / Trade-offs

- [Concurrent edits by two authors] → `If-Match` on step updates; reorder sends the complete id list and
  the server rejects an incomplete one. The editor reloads on 409/412.
- [Runbooks with many steps get slow to reorder by drag] → keyboard moves and "insert above/below" cover
  precise placement; no virtualization needed at expected sizes (tens of steps).
- [Free text invites secrets in prerequisites/evidence ("password is …")] → field hints say to reference
  where a credential is stored, never the credential; same rule as for instructions.
- [Phase separators repeat when phases interleave] → accepted; it makes the odd order visible instead of
  hiding it.

## Migration Plan

1. Migration `0006_runbook_step_prerequisites_evidence.sql`: add nullable columns to `runbook_step` and
   `run_step_state`. No backfill.
2. Deploy backend (additive API) before frontend. Rollback: the columns are nullable and ignored by older
   code; dropping them is not required.

## Open Questions

- Whether the handbook should print prerequisites as a list when the text contains line breaks — a
  rendering detail, decided during implementation.
