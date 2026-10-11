# Design

## Context

A recovery run copies its steps from the pinned plan snapshot into `run_step_state`
(primary key `(run_id, runbook_step_id)`), including `requires_authorization_role_id` and, after
`add-runbook-editor`, `evidence_required`. The run's timeline is `run_event`: append-only, with `type`,
`message`, `payload jsonb` and `actor`, streamed over SSE through an in-process bus, and written in the
same transaction as step state changes (`RecoveryRunRepository::save`). `transition()` in
`recovery_runs/domain.rs` enforces dependencies, a passed verification for `done` and a note for
`skipped`/`failed`, but ignores the approval role. Step transitions already put `stepId` into the event
payload. Requirements: see `specs/recovery-step-records/spec.md`.

## Goals / Non-Goals

**Goals:**
- One append-only source of truth for comments, evidence and approvals, reused by the timeline, the live
  stream and the gate.
- A gate that is decided in the pure domain and tested without a database.

**Non-Goals:**
- Any UI (see proposal).
- Verifying approvers against role assignments or the identity provider (decided: any approver).
- Editing or deleting records.

## Decisions

### Records are run events with a `step_id`
Migration adds `run_event.step_id uuid null` with
`foreign key (run_id, step_id) references run_step_state (run_id, runbook_step_id)` and an index on
`(run_id, step_id, id)`. New `RunEventType` values: `evidence` and `approval`. Comments use the existing
`note` type with a `step_id`. Step transitions now also fill `step_id`.
*Alternatives:* separate `run_step_evidence` / `run_step_approval` tables. Rejected: they would duplicate
the timeline, need their own audit and streaming, and invite updates. A typed column instead of payload
keeps per-step queries indexed and lets the foreign key reject unknown steps.

### Typed records in the domain, JSON only in infra
The domain gets a `StepRecord` value: `Comment { text }`, `Evidence { kind, description, reference }`,
`Approval { approver_name, approver_person_id, channel, note }`, each with an optional
`supersedes: Option<i64>` and constructors that validate (non-blank texts, `link` needs a reference,
lengths). Infra maps it to `type` + `message` + `payload`. `EvidenceKind` (`text`, `link`) and
`ApprovalChannel` (`in_person`, `phone`, `chat`, `email`, `other`) are `str_enum!`s; adding
`attachment` later is a new variant, not a new concept.

### Gate derived from the step's records, not denormalized
A pure function `summarize(records) -> StepRecordSummary { approval: Option<…>, evidence_count,
comment_count }` ignores superseded records. `transition()` takes the summary of the step: `done` with
`requires_authorization_role_id` set and no effective approval adds a blocking issue
`APPROVAL_REQUIRED` on `/to` (422, alongside `VERIFICATION_REQUIRED`). `evidence_missing` is computed as
`evidence_required.is_some() && evidence_count == 0 && status == done`.
*Alternative:* `approved_at` / `evidence_count` columns on `run_step_state` updated with each record.
Rejected: two sources of truth; the per-run event volume is small enough to summarize on read.

### Superseding
`supersedes` must name an event of the same run and step, else 422 `INVALID_REFERENCE` on
`/supersedes`. Any record type may supersede any other on the same step (a comment can retract an
approval). Superseded records are listed with `supersededBy`.

### Endpoints: step-scoped actions
- `POST /recovery-runs/{runId}/steps/{stepId}/comments` → `RunEvent`
- `POST /recovery-runs/{runId}/steps/{stepId}/evidence` → `RunEvent`
- `POST /recovery-runs/{runId}/steps/{stepId}/approval` → `RunEvent`
- `GET  /recovery-runs/{runId}/steps/{stepId}/records` → records in order, with `supersededBy`

`RunEvent` gains `stepId`; `RunStepState` gains `approval` (latest effective, or null),
`approvalRequired`, `evidenceCount`, `commentCount`, `evidenceMissing`. The generic
`POST …/events` keeps accepting `note`, `decision`, `communication_sent` and rejects `evidence` and
`approval` so validation lives in one place.
*Alternative:* only extending `POST …/events` with `stepId` and a typed payload. Rejected: a loose
`payload: object` cannot be validated with `deny_unknown_fields`.

### Author and concurrency
`actor` is the principal from the `TenantContext`; clients cannot set it. The approval's
`recordedBy` is that actor; `approverName` is what the responder typed. The use case reads the step's
records and decides the gate before the save transaction opens (as transitions do today), so a lock
inside `save` alone would not help. Instead the transition passes the highest record id it summarized;
`save` checks inside its transaction that the step has no newer record and otherwise fails with 409
(retry reads the fresh state). This keeps "supersede approval" and "complete step" from interleaving.

### MCP
One use-case tool `add_recovery_step_record` with `record: comment | evidence | approval` and the matching
fields; it calls the same application use case. `update_recovery_step` returns `APPROVAL_REQUIRED` like
the API. `get_recovery_status` adds `awaitingApproval` (unfinished steps that require approval and have
none) and `evidenceMissing` lists.

## Risks / Trade-offs

- [Existing clients now get 422 when completing approval-gated steps] → called out as BREAKING in the
  proposal; the problem detail names the role so a client can prompt for approval. Only runs whose plan
  sets an approval role are affected.
- [Approvals are attestations, not proof] → documented in recovery-execution.md and visible in the record
  (`recordedBy` vs. `approverName`).
- [Evidence references may leak secrets (for example a URL with a token)] → field hint and docs: link to
  the system of record, never paste credentials; records are never logged.
- [Summarizing records on every step list] → one query per run grouped by `step_id`; runs have at most
  hundreds of events.
- [Superseded approval races a completion] → optimistic record-id check described above; a 409 under
  heavy parallel work is acceptable and retried by the client.

## Migration Plan

1. Migration `0007_run_event_step_records.sql` (after `add-runbook-editor`'s `0006`): add `step_id`,
   foreign key, index; backfill `step_id` for existing `step_transition` events from `payload->>'stepId'`.
2. Deploy backend; announce the `APPROVAL_REQUIRED` behavior to MCP/API users. Rollback: the column is
   nullable and older code ignores it; older code would no longer enforce the gate.
