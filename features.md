# Features: user journeys and what's left to build

Working notes on how people actually use dra-workflow and what's still missing. Source of truth for
implementation status stays [docs/requirements/feature-status.md](docs/requirements/feature-status.md)
and the backlog in [todo.txt](todo.txt); this file adds the user-journey view and flags gaps not yet
tracked there.

## Personas

- **Service owner / IT architect** — authors and maintains the DR plan for their service(s).
- **DR / BCM coordinator** — oversees coverage across all services, chases reviews and tests, reports to management.
- **On-call / operations engineer** — executes the runbook during a real incident.
- **Auditor / compliance reviewer** — checks plan completeness, evidence and standards mapping.
- **Executive / management** — wants a one-page view of "are we ready?" and sign-off on approved plans.

## Typical user journeys

### 1. Author a new plan from scratch (service owner)
Cockpit → "New IT service" → Guide: describe the service and owners → add components (optional) →
map dependencies → run the BIA → brainstorm scenarios in the mind map → consolidate/categorize →
rate likelihood × impact in the risk matrix and decide DR-required/HA/accepted/degraded → set
recovery objectives (RTO/RPO) per component/scenario → define recovery strategies and close gaps →
write runbooks → assign roles and deputies → define communication rules → review the handbook
preview → approve a plan version. *Fully supported today, end to end, with gaps noted below (no UI
for runbook editing detail, RACI, or PDF export).*

### 2. Keep a plan current (service owner, recurring)
Cockpit → service shows "needs review" or a dependency/scenario changed → Guide reopens the
affected step (steps depending on an earlier one are marked `needs_review`) → re-approve a new plan
version. *Supported by the gate/needs_review mechanism; there is no reminder/scheduling layer yet
(see "Review scheduling" below).*

### 3. Run a tabletop or technical exercise (coordinator + service owner)
Guide → "Test & improve" → plan a test (type, scenario) → run it → record measured RTO/RPO versus
target → capture lessons learned → turn findings into action items with owners and due dates →
loop back into the plan. *API/MCP only today — no UI to plan, run or report an exercise (todo.txt
§12).*

### 4. Declare and run a real recovery (on-call engineer)
Recovery → declare a DR incident against an approved plan version → step through the runbook
(Activation → Recovery → Reconstitution), checking off steps, recording decisions and evidence →
close the run with lessons learned. *Read-only recovery overview exists; the guided, checkable
execution UI with escalation and a decision log/timeline is still open (todo.txt §9, §13).*

### 5. Check organization-wide readiness (coordinator / executive)
Cockpit → readiness score, critical gaps, services needing review, untested scenarios, next
actions across all services. *Supported.*

### 6. Prove compliance (auditor)
Service → Compliance tab → NIST SP 800-34 / BSI 200-4 / IT-Grundschutz requirement → what to do →
evidence in the tool → status. Export the handbook as Markdown for an audit package.
*Supported for the two mapped standards; no evidence-attachment package or ISO 22301/27001 mapping
yet (todo.txt, §14, §17).*

### 7. Onboard a new service quickly from a known pattern (service owner)
Pick a template ("web application", "batch job", "customer-facing API"...) that pre-fills typical
components, dependencies and scenario suggestions. *Not built — see "Templates" below (§1).*

### 8. Escalate and notify during an incident (on-call + coordinator)
Recovery run triggers notifications to the roles/contacts defined in communication rules (Slack,
Teams, email, SMS, paging), with status updates at the configured interval. *Communication rules
are authored today; nothing actually sends a notification (see "Notifications" below, §11).*

## What else to implement

Grouped roughly by how much they change the core journeys above. Item references are to
[docs/requirements/README.md](docs/requirements/README.md) sections where one exists.

### Close existing MVP gaps (highest value, already scoped)
- Runbook editor in the UI: steps with owner, prerequisites, instructions, expected result, evidence, approval (§8)
- Guided recovery execution UI: step-by-step run, escalation, decision log, timeline (§9)
- DR tests & exercises UI: schedule, run, record results, measure RTO/RPO, report (§12)
- Recovery validation chain: infrastructure → application → data → dependencies → business → acceptance (§13)
- Evidence: attach files/screenshots, evidence package export (§14)
- Findings & corrective actions UI: owner, deadline, status, retest (§10, §15)
- RACI matrix and recovery teams (§10)
- PDF/Word plan generation (Markdown only today) (§18)

### Not yet scoped anywhere, worth adding to the backlog
- **Review scheduling & reminders**: next-review date per plan, recurring test cadence, notify the
  owner before a plan/test goes overdue (ties into Cockpit "needs review").
- **Notification delivery**: turn communication rules into actual Slack/Teams/email/SMS/pager
  integrations instead of being documentation-only.
- **Templates per organization/service type** and **role-based task lists** (§1) — biggest lever for
  faster onboarding of new services.
- **Offline/print mode for the handbook**: a single-file, no-JS view usable when the tool itself is
  degraded (the vision doc already calls for this; only Markdown export exists).
- **Plan diff/versioning view**: show what changed between two approved plan versions, not just the
  latest snapshot.
- **CMDB/dependency auto-discovery integration**: import components and dependencies instead of
  hand-entering them (reduces the biggest data-entry burden in journey 1).
- **Scenario library sharing across tenants**: promote a vetted scenario from one tenant's mind map
  into the shared catalog ([[scenario-catalog]]) for reuse.
- **Benchmarking/maturity scoring**: compare a tenant's readiness score and test cadence against
  industry baselines (NIST/BSI maturity levels), for management reporting.
- **Fine-grained RBAC**: today auth is role-based at the tenant level (`dra-viewer`/`dra-admin`);
  per-service ownership permissions (only the service owner or their deputy can approve) are not
  enforced yet.
- **AI provider integration**: LLM-based scenario suggestions and drafting are designed for but not
  wired up (open question in [docs/overview/open-questions.md](docs/overview/open-questions.md)).
- **Multi-organization tenant switching** in the UI (open question, same doc).
- **Localization of backend gate/validation messages** (currently English only; the UI is already i18n).
- **Mobile-friendly recovery view**: the recovery execution UI (once built) needs to work well on a
  phone, since responders may not be at a desk during a disaster.
