# Tasks

Prerequisite: `add-runbook-editor` is implemented (`evidenceRequired` exists on run step states).

## 1. Contract and schema

- [ ] 1.1 In `api/openapi.yaml` add the step-scoped endpoints (`…/steps/{stepId}/comments`, `/evidence`, `/approval`, `/records`), request schemas (`StepCommentCreate`, `StepEvidenceCreate`, `StepApprovalCreate` with `supersedes`), `RunEventType` values `evidence` and `approval`, `stepId` on `RunEvent`, `approval`/`approvalRequired`/`evidenceCount`/`commentCount`/`evidenceMissing` on `RunStepState`, and document `APPROVAL_REQUIRED` on the step transition; verify `npx @redocly/cli lint api/openapi.yaml` passes
- [ ] 1.2 Add migration `0007_run_event_step_records.sql`: nullable `run_event.step_id`, foreign key `(run_id, step_id)` → `run_step_state (run_id, runbook_step_id)`, index `(run_id, step_id, id)`, backfill from `payload->>'stepId'` for `step_transition` events; verify `dra-server migrate` succeeds on a database with existing runs and the backfilled ids match the payloads

## 2. Domain

- [ ] 2.1 Add `StepRecord` (comment, evidence, approval, optional `supersedes`), `EvidenceKind`, `ApprovalChannel` and their validating constructors to `recovery_runs/domain.rs`; verify unit tests for blank comment, `link` without reference, unknown kind and missing approver name
- [ ] 2.2 Add `summarize(records)` ignoring superseded records and the `evidence_missing` rule; verify unit tests for supersede chains (approval retracted by comment) and evidence added after completion
- [ ] 2.3 Extend `transition()` with the step summary and the `APPROVAL_REQUIRED` blocking issue for `done` only; verify unit tests for done-without-approval, done-after-approval, skip/fail/start without approval and steps without an approval role

## 3. Application and infra

- [ ] 3.1 Add the `add_step_record` use case (active run, step exists in run, `supersedes` same run and step, actor from `TenantContext`) and publish the stored event on the bus; verify tests with in-memory fakes for closed run (409), unknown step (404) and cross-step supersede (422)
- [ ] 3.2 Load step records in `transition` and `steps`, pass summaries to the domain and the step views; fill `step_id` on transition events; verify fake-port tests for the gate and for `evidenceMissing` in the step list
- [ ] 3.3 Infra: write records as `run_event` rows with `step_id`, map payloads to and from `StepRecord`, list records per step and grouped per run, and enforce the "no newer record" check inside `save`; verify Postgres integration tests for round-trip, tenant isolation (other tenant → 404) and the optimistic 409

## 4. API and MCP

- [ ] 4.1 Add DTOs (`deny_unknown_fields`, length limits) and handlers for the four step endpoints, extend `RunEventDto` and the step DTO, and reject `evidence`/`approval` on the generic events endpoint; verify `oneshot` tests for 201, 422 issues, `APPROVAL_REQUIRED` on transition and that the SSE event carries `stepId`
- [ ] 4.2 Add MCP tool `add_recovery_step_record`, return `APPROVAL_REQUIRED` from `update_recovery_step`, and add `awaitingApproval`/`evidenceMissing` to `get_recovery_status`; update the server instructions text; verify `tests/mcp.rs` covers recording evidence and a blocked completion
- [ ] 4.3 Extend `tests/dr_workflow.rs`: approve a plan with an approval-gated step that requires evidence, declare a run, hit `APPROVAL_REQUIRED`, record approval, complete, see `evidenceMissing`, add evidence, see it cleared; verify `make test` and `make lint` pass

## 5. Documentation

- [ ] 5.1 Update `docs/workflows/recovery-execution.md` (step records, approval as recorded attestation gating `done`, evidence warning, append-only corrections) and `docs/architecture/mcp.md` (new tool, status fields); verify `make docs` passes
- [ ] 5.2 Update `docs/requirements/feature-status.md` §8 (evidence/comments/approval during runs via API/MCP) and §9 (decision log per step; guided execution UI still open), add a `docs/log.md` entry; verify `make docs` passes

## 6. Integration check

- [ ] 6.1 Run `make check`, then drive a test run end to end via MCP (`declare_recovery` → `add_recovery_step_record` → `update_recovery_step` → `get_recovery_status`) against `make run`; verify the timeline from `GET /recovery-runs/{id}/events` shows comments, evidence and the approval with step ids

## Workflow follow-up

- The run execution UI for these records belongs to the §9 guided execution UI change.
- Archive the change after review.
