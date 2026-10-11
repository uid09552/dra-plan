# Proposal

## Why

During a recovery run a step keeps only one overwritable `note`. There is no way to attach evidence,
discuss a step, or record who approved it. Requirement §8 asks for evidence, comments and approval per
step. Also, the docs say decision points need the authorizer role, but `transition()` never checks
`requiresAuthorizationRoleId`, so the plan's approval requirement has no effect once a run starts.

## What Changes

- **Step comments:** any number of timestamped comments per run step, each with its author.
- **Step evidence:** any number of entries per run step. An entry is a description plus an optional
  reference (a link, or a pointer such as a ticket id) and has a `kind` (`text` or `link` now). The
  `kind` field leaves room for file attachments later without a new concept.
- **Step approval:** recording that a step was approved, with the approver's name (free text, optionally
  linked to a directory person), how the approval was given (in person, phone, chat, email, other), an
  optional note, and the logged-in user who recorded it. Any approver is accepted. The tool records the
  approval but does not verify the approver's identity.
- **Approval gate:** completing (`done`) a step whose plan names an approval role fails with
  **`422 APPROVAL_REQUIRED`** until an approval is recorded. Starting, skipping and failing are not
  gated. **BREAKING** for API and MCP clients that complete such steps today without an approval.
- **Evidence warning:** completing a step whose plan requires evidence (`evidenceRequired`, from
  `add-runbook-editor`) without any evidence entry still succeeds, but the step is flagged
  `evidenceMissing` in the step state and in the recovery status.
- All three records are **append-only** run events tied to the step, streamed live to everyone following
  the run. A correction is a new entry that points to the one it supersedes; nothing is edited or deleted.
- New MCP tool(s) for comments, evidence and approval; `get_recovery_status` reports pending approvals
  and missing evidence.
- Docs are corrected: the approval role gates `done`, and the approval is recorded, not verified.

Not in this change: a run execution UI (that is §9 "Guided execution UI"), file attachments, verifying
the approver's identity against the identity provider, and report or document generation (§18).

## Capabilities

### New Capabilities
- `recovery-step-records`: comments, evidence and approvals recorded on the steps of a recovery run,
  plus the approval gate and the evidence warning on step completion.

### Modified Capabilities
<!-- None: no specs exist yet in openspec/specs/. -->

## Impact

- **Depends on:** `add-runbook-editor` (the `evidenceRequired` step field in snapshot and run state).
- **API:** `api/openapi.yaml`: new step-scoped endpoints under `/recovery-runs/{runId}/steps/{stepId}/…`,
  new `RunEventType` values `evidence` and `approval`, `stepId` on `RunEvent`, and `approval` /
  `evidenceMissing` / counts on `RunStepState`. `APPROVAL_REQUIRED` on the step transition.
- **Database:** migration adding a nullable `step_id` to `run_event` (with a composite foreign key to
  `run_step_state`) and an index on `(run_id, step_id)`.
- **Backend:** `features/recovery_runs` (domain gate, event types, use cases, DTOs, infra), `mcp/tools.rs`.
- **Frontend:** none (the UI comes with the §9 guided execution UI).
- **Docs:** `docs/workflows/recovery-execution.md`, `docs/architecture/mcp.md`,
  `docs/requirements/feature-status.md` (§8, §9), `docs/log.md`.
