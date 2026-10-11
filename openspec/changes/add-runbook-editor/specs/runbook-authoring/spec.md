# Spec Delta

## Purpose

Lets plan authors write and maintain the recovery runbooks of an IT service: ordered, phased steps per
component and DR scenario, with the attributes responders need to execute them during a disaster.

## ADDED Requirements

### Requirement: Runbook step attributes
A runbook step SHALL carry a phase (activation, recovery, reconstitution), a title, and optionally
instructions, owner role, expected duration, expected result (verification), dependencies on earlier
steps, a decision-point flag, an approval role, prerequisites and required evidence. Prerequisites and
required evidence SHALL be free text; blank values SHALL be stored as absent.

#### Scenario: Step with prerequisites and required evidence
- **WHEN** an author saves a step with prerequisites "Access to backup vault" and required evidence "Restore log reference"
- **THEN** reading the step returns both values unchanged

#### Scenario: Blank prerequisites are cleared
- **WHEN** an author saves a step whose prerequisites contain only whitespace
- **THEN** the step has no prerequisites

#### Scenario: Missing title is rejected
- **WHEN** an author saves a step without a title
- **THEN** the request fails with 422 and an issue on `/title`

### Requirement: Step order respects dependencies
Steps SHALL run in their order unless a step names explicit dependencies. A step SHALL only depend on
earlier steps of the same runbook and never on itself. Saving or reordering steps that would break this
SHALL be rejected with an issue naming the offending step, and the previous order SHALL be kept.

#### Scenario: Reorder that breaks a dependency
- **WHEN** step "Validate application" depends on "Restore database" and an author moves it above "Restore database"
- **THEN** the reorder fails with 422 `STEP_ORDER_VIOLATES_DEPENDENCY` and the stored order is unchanged

#### Scenario: Deleting a step others depend on
- **WHEN** an author deletes a step that another step depends on
- **THEN** the delete fails with 409 and the step remains

### Requirement: Critical path compared with the RTO
Each runbook SHALL report its critical path: the longest chain of expected step durations along the
effective dependencies, counting steps without a duration as zero. When the component's recovery
objective for the scenario has an RTO shorter than the critical path, the runbook SHALL be flagged as
exceeding the RTO.

#### Scenario: Parallel steps shorten the critical path
- **WHEN** steps B (30 min) and C (20 min) both depend only on A (10 min) and D (5 min) depends on B and C
- **THEN** the critical path is 45 minutes

#### Scenario: Runbook slower than the RTO
- **WHEN** the critical path is 95 minutes and the RTO is 60 minutes
- **THEN** the runbook is flagged as exceeding the RTO and the runbooks workflow step reports `RUNBOOK_EXCEEDS_RTO`

### Requirement: Runbooks section in the Guide
The Guide's Mitigations stage SHALL list runbooks grouped by DR scenario, with one entry per affected
component and scenario. Each entry SHALL show the runbook title, step count, critical path and an RTO
warning, or a create action when that component has no runbook for the scenario.

#### Scenario: Missing runbook is visible
- **WHEN** scenario "Ransomware" affects components DB and App and only DB has a runbook
- **THEN** the Ransomware group shows the DB runbook and a create action for App

#### Scenario: Create a runbook from the section
- **WHEN** an author uses the create action for App in "Ransomware" and enters a title
- **THEN** a runbook for App and Ransomware exists and opens in the step editor

#### Scenario: Runbook without a strategy
- **WHEN** a runbook has no linked recovery strategy
- **THEN** it is still listed under its scenario and component

### Requirement: Step editor
The Guide SHALL offer a step editor for a runbook that groups steps by phase in execution order, adds,
edits, inserts above or below, and deletes steps through dialogs, and reorders them by drag and drop.
Every list action SHALL be reachable by right-click, a ⋮ button and the context-menu key.

#### Scenario: Insert a step above another
- **WHEN** an author chooses "insert above" on step 3 and saves a new step
- **THEN** the new step becomes step 3 and the former step 3 becomes step 4

#### Scenario: Keyboard-only editing
- **WHEN** a user focuses a step and presses the context-menu key
- **THEN** the edit, insert and delete actions are offered without a pointer

#### Scenario: Rejected reorder shown inline
- **WHEN** a drag and drop reorder is rejected by the server
- **THEN** the editor shows the issue message and displays the stored order

#### Scenario: Labels in both languages
- **WHEN** the UI language is German
- **THEN** all editor labels, including "Expected result / verification", prerequisites and required evidence, are shown in German

### Requirement: New step attributes reach the plan and the run
Prerequisites and required evidence SHALL be part of the approved plan snapshot, the Markdown handbook
and the step states of a recovery run started from that snapshot. Snapshots approved before these
attributes existed SHALL remain readable, with both attributes absent.

#### Scenario: Handbook shows prerequisites and evidence
- **WHEN** a plan whose step has prerequisites and required evidence is exported as Markdown
- **THEN** the step's section lists both

#### Scenario: Run step shows the planned evidence
- **WHEN** a recovery run is declared from a plan version whose step requires evidence
- **THEN** the run's step state includes the required evidence text

#### Scenario: Old snapshot still starts a run
- **WHEN** a recovery run is declared from a plan version approved before this change
- **THEN** the run starts and its steps have no prerequisites and no required evidence

### Requirement: AI can author the new step attributes
The MCP tool that creates a mitigation with its runbook SHALL accept prerequisites and required evidence
per step, validated like input from the UI.

#### Scenario: MCP creates a step with prerequisites
- **WHEN** an AI client calls `add_mitigation` with a step that has prerequisites
- **THEN** the created step stores the prerequisites

#### Scenario: Oversized value from MCP is rejected
- **WHEN** an AI client sends required evidence longer than the allowed length
- **THEN** the tool call fails with a validation issue and no runbook is created
