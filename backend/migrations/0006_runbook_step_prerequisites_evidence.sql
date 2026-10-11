-- Runbook steps: prerequisites and the evidence a responder must record (docs/requirements/README.md §8).
-- Free text; run step states carry copies from the pinned plan snapshot.
alter table runbook_step
    add column prerequisites     text check (length(prerequisites) <= 5000),
    add column evidence_required text check (length(evidence_required) <= 5000);

alter table run_step_state
    add column prerequisites     text,
    add column evidence_required text;
