# Tasks

## 1. Contract and schema

- [x] 1.1 Add `prerequisites` and `evidenceRequired` (string, maxLength 5000, optional) to `RunbookStepFields` and read-only to `RunStepState` in `api/openapi.yaml`; verify `npx @redocly/cli lint api/openapi.yaml` passes
- [x] 1.2 Add migration `0006_runbook_step_prerequisites_evidence.sql` with nullable `prerequisites` and `evidence_required` text columns on `runbook_step` and `run_step_state`; verify `dra-server migrate` succeeds on a fresh and on an existing database

## 2. Backend: runbooks feature

- [x] 2.1 Extend `RunbookStep` and `StepInput` in `runbooks/domain.rs` with both fields, normalized through `non_blank`; verify new unit tests for set, blank-clears and unchanged-when-absent pass
- [x] 2.2 Extend `StepFieldsDto` (length ≤ 5000) and `RunbookStepDto` (`serde(default)`, omitted when absent) in `runbooks/api.rs`; verify a `oneshot` API test round-trips both fields and returns 422 for an oversized value
- [x] 2.3 Read and write both columns in `runbooks/infra.rs` (`insert`, `save_steps`, `steps`, `get_step`); verify an integration test against Postgres stores and reads them back

## 3. Backend: snapshot, run and handbook

- [x] 3.1 Carry both fields through `PlanSnapshotDto` encode/decode; verify a test decodes a stored format-1 snapshot JSON without the fields and yields `None`
- [x] 3.2 Add both fields to `RunStepState`, copy them in `instantiate`, persist them in the recovery-runs infra and expose them in the run step DTO; verify a `dr_workflow.rs` test declares a run and sees the required evidence on the step
- [x] 3.3 Render prerequisites and required evidence per step in the Markdown handbook (`plans/infra.rs`, English and German labels); verify a handbook test shows both for a step that has them and omits the lines otherwise

## 4. Backend: MCP

- [x] 4.1 Accept `prerequisites` and `evidenceRequired` per step in the `add_mitigation` schema and pass them through the same validation; verify `tests/mcp.rs` covers a successful call and an oversized-value rejection
- [x] 4.2 Run `make lint` and `make test`; verify both pass

## 5. Frontend: API client

- [x] 5.1 Add `Runbook`, `RunbookDetail`, `RunbookStep` and step-field models plus runbook/step methods (list for service, create, update, delete, list steps, add step with position, update step with `If-Match`, delete step, reorder) to `core/api`; verify `make ui-build` compiles

## 6. Frontend: runbooks section

- [x] 6.1 Create `runbooks-view.ts`: groups per DR scenario, one row per affected component (default component included) with title, step count, critical path, RTO warning, or a create action; verify a Vitest spec covers the missing-runbook row and a runbook without a strategy
- [x] 6.2 Runbook create/edit/delete through `EditDialog`/`ConfirmDialog` with right-click, ⋮ and context-menu key; verify a spec opens the context menu by keyboard and the create action preselects scenario and component
- [x] 6.3 Replace `guide.mitigations.runbookHint` in the Mitigations stage of `service-guide.ts` with the section and remove the unused i18n key from `en.ts` and `de.ts`; verify `make ui-test` passes

## 7. Frontend: step editor

- [x] 7.1 Create `runbook-editor.ts`: header (title, scenario, component, critical path vs. RTO), steps in `seq` order with phase separators, back action, open runbook kept in the `runbook` query parameter; verify a spec renders separators for a three-phase runbook and reopens from the query parameter
- [x] 7.2 Step add/edit/insert above/below/delete via `EditDialog` with all fields from the design (dependencies limited to earlier steps, labels "Expected result / verification", prerequisites, required evidence, hint not to enter secrets); verify a spec checks the dependency options and the insert position sent to the API
- [x] 7.3 Reorder with CDK drag and drop plus "Move up"/"Move down" in the context menu; on 422 show the issue and reload the stored order; verify a spec simulates a rejected reorder
- [x] 7.4 Show workflow gate issues for `runbook`/`runbook_step` entities on the matching rows; verify a spec shows `STEP_WITHOUT_OWNER` on the right step
- [x] 7.5 Add all new i18n keys to `en.ts` and `de.ts`; verify `make ui-lint` and the i18n key-parity check (if present) pass

## 8. Documentation

- [x] 8.1 Update `docs/domain/dr-plan-model.md` (step attributes) and `docs/workflows/plan-authoring.md` step 9 (editor, new fields); verify `make docs` passes
- [x] 8.2 Update `docs/guide/ui-tour.md` for the Runbooks section and add a screenshot in `docs/assets/screenshots/`; mark the editor done in `docs/requirements/feature-status.md` §8 (evidence/comments/approval during runs stays open) and add an entry to `docs/log.md`; verify `make docs` passes
- [x] 8.3 Remove the runbook editor item from `todo.txt` once verified in the running app (`make run` + `make ui`)

## 9. Integration check

- [x] 9.1 Run `make check`, then author a runbook end to end in the UI (create, add steps, reorder, rejected reorder, gate issues clear), approve the plan and declare a test run; verify the run's steps show prerequisites and required evidence via `GET /recovery-runs/{id}/steps`

## Workflow follow-up

- Implement `add-recovery-step-records` (run-time evidence, comments, approval gate) after this change.
- Archive the change after review.
