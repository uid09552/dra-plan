# Spec Delta

## Purpose

Lets responders record comments, evidence and approvals on the steps of a recovery run as an
append-only, auditable trail, and makes the plan's approval requirement effective during execution.

## ADDED Requirements

### Requirement: Step comments
Responders SHALL be able to add any number of comments to a step of an active recovery run. Each comment
SHALL record its text, the time and the authenticated user who wrote it. Blank comments SHALL be rejected.

#### Scenario: Add a comment
- **WHEN** a responder adds the comment "DB restore slower than expected" to a step
- **THEN** the step's records include the comment with the responder as author and the current time

#### Scenario: Blank comment
- **WHEN** a responder submits a comment containing only whitespace
- **THEN** the request fails with 422 and an issue on `/message`

### Requirement: Step evidence
Responders SHALL be able to add any number of evidence entries to a step of an active recovery run. An
entry SHALL have a kind (`text` or `link`), a description and, for `link`, a reference. Each entry SHALL
record the time and the authenticated user. Evidence SHALL hold references, never file contents.

#### Scenario: Add a reference as evidence
- **WHEN** a responder adds evidence of kind `link` with description "Restore log" and reference "INC-4711"
- **THEN** the step's records include that evidence entry with author and time

#### Scenario: Link without reference
- **WHEN** a responder adds evidence of kind `link` without a reference
- **THEN** the request fails with 422 and an issue on `/reference`

#### Scenario: Unknown kind
- **WHEN** a responder adds evidence of kind `file`
- **THEN** the request fails with 422 and an issue on `/kind`

### Requirement: Step approval
Responders SHALL be able to record an approval on a step of an active recovery run. An approval SHALL
name the approver (free text, optionally a directory person), how it was given (in person, phone, chat,
email, other) and an optional note, and SHALL record the time and the authenticated user who recorded it.

#### Scenario: Record a phone approval
- **WHEN** a responder records an approval by "J. Doe" given by phone
- **THEN** the step shows as approved by "J. Doe" via phone, recorded by that responder at the current time

#### Scenario: Approver who holds no plan role
- **WHEN** a responder records an approval naming a person who is not assigned to the step's approval role
- **THEN** the approval is accepted and recorded as given

#### Scenario: Approver without a name
- **WHEN** a responder records an approval without an approver name
- **THEN** the request fails with 422 and an issue on `/approverName`

### Requirement: Approval gate on completion
Completing a run step whose plan names an approval role SHALL fail with 422 `APPROVAL_REQUIRED` until an
approval is recorded on that step. Starting, skipping and failing such a step SHALL not require approval.
The gate SHALL apply to real and test runs alike.

#### Scenario: Complete without approval
- **WHEN** a responder completes a step that requires the "CISO" role's approval and no approval is recorded
- **THEN** the request fails with 422 `APPROVAL_REQUIRED` and the step stays in its current status

#### Scenario: Complete after approval
- **WHEN** an approval is recorded on that step and the responder completes it with a passed verification
- **THEN** the step is done

#### Scenario: Skip without approval
- **WHEN** a responder skips a step that requires approval, giving a reason
- **THEN** the step is skipped without an approval

#### Scenario: Step without approval role
- **WHEN** a responder completes a step whose plan names no approval role
- **THEN** no approval is required

### Requirement: Missing evidence warning
Completing a run step whose plan requires evidence but has no evidence entry SHALL succeed, and the step
SHALL be flagged as missing evidence until an entry is added. The recovery status SHALL list steps that
are missing evidence and steps waiting for approval.

#### Scenario: Complete without required evidence
- **WHEN** a responder completes a step whose plan requires evidence and no evidence is recorded
- **THEN** the step is done and flagged as missing evidence

#### Scenario: Evidence added afterwards
- **WHEN** a responder adds evidence to that done step while the run is still active
- **THEN** the step is no longer flagged as missing evidence

#### Scenario: Status lists open items
- **WHEN** an AI client asks for the recovery status of a run with such steps
- **THEN** the status lists the steps missing evidence and the steps that need approval before completion

### Requirement: Append-only step records
Comments, evidence and approvals SHALL never be changed or deleted. A correction SHALL be a new record
that references the record it supersedes. Superseded records SHALL stay visible and marked as superseded.
A superseded approval SHALL no longer count for the approval gate.

#### Scenario: Correct an evidence entry
- **WHEN** a responder adds evidence that supersedes an earlier entry on the same step
- **THEN** both entries are listed and the earlier one is marked as superseded

#### Scenario: Supersede an approval
- **WHEN** a responder adds the comment "approval was given for the wrong system" superseding the only approval of an unfinished step
- **THEN** completing the step fails with `APPROVAL_REQUIRED`

#### Scenario: Supersede a record of another step
- **WHEN** a responder references a record that belongs to a different step
- **THEN** the request fails with 422

### Requirement: Records are scoped, live and audited
Step records SHALL only be added to existing steps of active runs in the caller's tenant. Records SHALL
appear in the run's event timeline and live stream with the step they belong to, and SHALL be listable
per step. Plan contents and record texts SHALL not be written to logs.

#### Scenario: Closed run
- **WHEN** a responder adds a comment to a step of a closed run
- **THEN** the request fails with 409

#### Scenario: Other tenant
- **WHEN** a user adds evidence to a step of a run that belongs to another tenant
- **THEN** the request fails with 404

#### Scenario: Live stream
- **WHEN** a responder records an approval while another client follows the run's stream
- **THEN** the other client receives an approval event carrying the step id

#### Scenario: Records per step
- **WHEN** a client lists the records of one step
- **THEN** it receives that step's comments, evidence and approvals in chronological order and nothing from other steps

### Requirement: AI can record and read step records
MCP tools SHALL let an AI client add comments, evidence and approvals to run steps with the same
validation and gates as the REST API, recording the authenticated user as author.

#### Scenario: MCP records evidence
- **WHEN** an AI client records evidence on a step through MCP
- **THEN** the entry is stored with the authenticated user as author and appears in the run's timeline

#### Scenario: MCP completion blocked
- **WHEN** an AI client completes a step that requires approval and none is recorded
- **THEN** the tool call fails with `APPROVAL_REQUIRED`
