-- dra-workflow initial schema. See docs/domain/dr-plan-model.md.
--
-- Conventions
-- * Every tenant-scoped table has tenant_id and a UNIQUE (tenant_id, id). References use composite
--   foreign keys (tenant_id, <ref>_id) so a row can never point into another tenant.
-- * Row-level security is FORCED on tenant-scoped tables (defense in depth; the application also
--   filters by tenant_id). The policy reads app.tenant_id, set per transaction by the backend.
-- * Foreign keys without an ON DELETE action use NO ACTION (checked at end of statement), so deleting
--   a referenced row fails with 23503 -> HTTP 409, while cascades from a parent still work.
-- * The audit_row() trigger writes an append-only audit_log entry for every change.

-- ───────────────────────────── Audit ─────────────────────────────
create table audit_log (
    id          bigint generated always as identity primary key,
    tenant_id   uuid        not null,
    at          timestamptz not null default now(),
    actor       text        not null,
    action      text        not null,
    entity_type text        not null,
    entity_id   uuid,
    diff        jsonb
);
create index audit_log_tenant_at on audit_log (tenant_id, at);
create index audit_log_entity on audit_log (tenant_id, entity_type, entity_id);

create function audit_row() returns trigger language plpgsql as $$
declare
    new_j jsonb := case when tg_op <> 'DELETE' then to_jsonb(new) end;
    old_j jsonb := case when tg_op <> 'INSERT' then to_jsonb(old) end;
    rec   jsonb := coalesce(new_j, old_j);
    diff  jsonb;
begin
    if tg_op = 'UPDATE' then
        select coalesce(jsonb_object_agg(n.key, n.value), '{}'::jsonb) into diff
        from jsonb_each(new_j) n
        where old_j -> n.key is distinct from n.value;
        diff := diff - 'version' - 'updated_at' - 'updated_by';
        if diff = '{}'::jsonb then
            return null;
        end if;
    elsif tg_op = 'INSERT' then
        diff := new_j;
    end if;

    insert into audit_log (tenant_id, actor, action, entity_type, entity_id, diff)
    values (
        coalesce((rec ->> 'tenant_id')::uuid, (rec ->> 'id')::uuid),
        coalesce(nullif(current_setting('app.actor', true), ''), 'system'),
        case tg_op when 'INSERT' then 'create' when 'UPDATE' then 'update' else 'delete' end,
        tg_table_name,
        (rec ->> 'id')::uuid,
        diff
    );
    return null;
end;
$$;

-- Helper to attach RLS + audit to a tenant-scoped table.
create procedure enable_tenant_rls(tbl regclass) language plpgsql as $$
begin
    execute format('alter table %s enable row level security', tbl);
    execute format('alter table %s force row level security', tbl);
    execute format(
        'create policy tenant_isolation on %s using (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid)',
        tbl);
end;
$$;

create procedure enable_audit(tbl regclass) language plpgsql as $$
begin
    execute format('create trigger audit after insert or update or delete on %s for each row execute function audit_row()', tbl);
end;
$$;

-- ───────────────────────────── Tenant & directory ─────────────────────────────
create table tenant (
    id                          uuid primary key,
    name                        text    not null check (length(name) between 1 and 200),
    slug                        text    not null unique check (slug ~ '^[a-z0-9-]{2,63}$'),
    review_interval_months      int     not null default 12 check (review_interval_months >= 1),
    impact_categories           text[]  not null default '{financial,operational,reputational,legal_regulatory,people_safety}',
    impact_time_windows_minutes int[]   not null default '{60,240,1440,4320,10080}',
    impact_tolerance_level      int     not null default 3 check (impact_tolerance_level between 1 and 4),
    default_language            text    not null default 'en',
    ai_enabled                  boolean not null default true,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null
);

create table person (
    id                uuid primary key,
    tenant_id         uuid not null references tenant (id) on delete cascade,
    name              text not null check (length(name) >= 1),
    email             text,
    phone             text,
    alternate_contact text,
    team              text,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id)
);

create table tenant_member (
    id           uuid primary key,
    tenant_id    uuid not null references tenant (id) on delete cascade,
    user_ref     text not null,
    email        text,
    display_name text,
    tenant_role  text not null check (tenant_role in ('admin', 'author', 'reviewer', 'responder', 'auditor')),
    person_id    uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, user_ref),
    foreign key (tenant_id, person_id) references person (tenant_id, id) on delete set null (person_id)
);
create index tenant_member_user on tenant_member (user_ref);

create table role (
    id          uuid primary key,
    tenant_id   uuid    not null references tenant (id) on delete cascade,
    name        text    not null check (length(name) >= 1),
    description text,
    is_default  boolean not null default false,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    unique (tenant_id, name)
);

-- ───────────────────────────── IT service ─────────────────────────────
create table it_service (
    id                                  uuid primary key,
    tenant_id                           uuid   not null references tenant (id) on delete cascade,
    name                                text   not null check (length(name) between 1 and 200),
    description                         text,
    business_owner_id                   uuid,
    technical_owner_id                  uuid,
    consumers                           text[] not null default '{}',
    protection_requirement_availability text check (protection_requirement_availability in ('normal', 'high', 'very_high')),
    impact_level                        text check (impact_level in ('low', 'moderate', 'high')),
    lifecycle_status                    text   not null default 'active' check (lifecycle_status in ('planned', 'active', 'retired')),
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, business_owner_id) references person (tenant_id, id) on delete set null (business_owner_id),
    foreign key (tenant_id, technical_owner_id) references person (tenant_id, id) on delete set null (technical_owner_id)
);
create index it_service_tenant_name on it_service (tenant_id, name);

create table business_impact_analysis (
    id                      uuid primary key,
    tenant_id               uuid not null references tenant (id) on delete cascade,
    service_id              uuid not null unique,
    mtpd_minutes            int  not null check (mtpd_minutes >= 0),
    service_rto_minutes     int  not null check (service_rto_minutes >= 0),
    service_rpo_minutes     int  not null check (service_rpo_minutes >= 0),
    minimum_operating_level text,
    regulatory_requirements text,
    approved_at             timestamptz,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    check (service_rto_minutes <= mtpd_minutes),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade
);

create table impact_rating (
    tenant_id           uuid not null references tenant (id) on delete cascade,
    bia_id              uuid not null,
    impact_category     text not null,
    time_window_minutes int  not null check (time_window_minutes >= 0),
    level               int  not null check (level between 1 and 4),
    rationale           text,
    primary key (bia_id, impact_category, time_window_minutes),
    foreign key (tenant_id, bia_id) references business_impact_analysis (tenant_id, id) on delete cascade
);

-- ───────────────────────────── Microservice & scenario ─────────────────────────────
create table microservice (
    id                         uuid primary key,
    tenant_id                  uuid   not null references tenant (id) on delete cascade,
    service_id                 uuid   not null,
    name                       text   not null check (length(name) >= 1),
    description                text,
    owner_team                 text,
    platform                   text,
    hosting_location           text,
    data_stores                text[] not null default '{}',
    restore_order              int check (restore_order >= 1),
    criticality_within_service text check (criticality_within_service in ('critical', 'important', 'supporting')),
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade
);
create index microservice_service on microservice (service_id);

create table scenario (
    id                 uuid primary key,
    tenant_id          uuid not null references tenant (id) on delete cascade,
    service_id         uuid not null,
    title              text not null check (length(title) >= 1),
    description        text,
    category           text,
    status             text not null default 'brainstormed' check (status in ('brainstormed', 'merged', 'selected', 'rejected')),
    merged_into_id     uuid,
    likelihood         int check (likelihood between 1 and 4),
    impact             int check (impact between 1 and 4),
    priority           text,
    dr_required        text,
    decision_rationale text,
    decided_by         text,
    decided_at         timestamptz,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade,
    foreign key (tenant_id, merged_into_id) references scenario (tenant_id, id) on delete set null (merged_into_id)
);
create index scenario_service on scenario (service_id);

create table scenario_microservice (
    tenant_id       uuid not null references tenant (id) on delete cascade,
    scenario_id     uuid not null,
    microservice_id uuid not null,
    primary key (scenario_id, microservice_id),
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id) on delete cascade,
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade
);

-- ───────────────────────────── Microservice DR items ─────────────────────────────
create table dependency (
    id                     uuid primary key,
    tenant_id              uuid    not null references tenant (id) on delete cascade,
    microservice_id        uuid    not null,
    kind                   text    not null,
    target_microservice_id uuid,
    target_name            text,
    direction              text    not null check (direction in ('upstream', 'downstream')),
    criticality            text    not null check (criticality in ('critical', 'degradable', 'optional')),
    dependency_rto_minutes int check (dependency_rto_minutes >= 0),
    has_own_dr_plan        boolean not null default false,
    notes                  text,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    check (kind <> 'microservice' or target_microservice_id is not null),
    check (target_microservice_id is not null or target_name is not null),
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade,
    foreign key (tenant_id, target_microservice_id) references microservice (tenant_id, id)
);
create index dependency_microservice on dependency (microservice_id);
create index dependency_target on dependency (target_microservice_id);

create table recovery_objective (
    id                  uuid primary key,
    tenant_id           uuid   not null references tenant (id) on delete cascade,
    microservice_id     uuid   not null,
    scenario_id         uuid,
    rto_minutes         int    not null check (rto_minutes >= 0),
    rpo_minutes         int    not null check (rpo_minutes >= 0),
    mttr_target_minutes int check (mttr_target_minutes >= 0),
    restore_priority    int check (restore_priority >= 1),
    first_functions     text[] not null default '{}',
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    unique nulls not distinct (microservice_id, scenario_id),
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade,
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id)
);

create table recovery_strategy (
    id                          uuid primary key,
    tenant_id                   uuid    not null references tenant (id) on delete cascade,
    microservice_id             uuid    not null,
    scenario_id                 uuid    not null,
    type                        text    not null,
    title                       text,
    description                 text,
    estimated_rto_minutes       int     not null check (estimated_rto_minutes >= 0),
    estimated_rpo_minutes       int     not null check (estimated_rpo_minutes >= 0),
    cost_notes                  text,
    prerequisites               text[]  not null default '{}',
    is_selected                 boolean not null default false,
    gap_justification           text,
    accepted_gap_action_item_id uuid,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade,
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id)
);
create index recovery_strategy_microservice on recovery_strategy (microservice_id);
create unique index recovery_strategy_one_selected on recovery_strategy (microservice_id, scenario_id) where is_selected;

create table data_protection (
    id                   uuid primary key,
    tenant_id            uuid    not null references tenant (id) on delete cascade,
    microservice_id      uuid    not null,
    data_store           text    not null,
    method               text    not null,
    frequency_minutes    int     not null check (frequency_minutes >= 0),
    retention_days       int check (retention_days >= 0),
    offsite              boolean not null default false,
    immutable            boolean not null default false,
    encrypted            boolean not null default false,
    last_restore_test_at timestamptz,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade
);
create index data_protection_microservice on data_protection (microservice_id);

create table runbook (
    id              uuid primary key,
    tenant_id       uuid not null references tenant (id) on delete cascade,
    microservice_id uuid not null,
    scenario_id     uuid not null,
    strategy_id     uuid,
    title           text not null check (length(title) >= 1),
    description     text,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade,
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id),
    foreign key (tenant_id, strategy_id) references recovery_strategy (tenant_id, id)
);
create index runbook_microservice on runbook (microservice_id);

create table runbook_step (
    id                             uuid primary key,
    tenant_id                      uuid    not null references tenant (id) on delete cascade,
    runbook_id                     uuid    not null,
    seq                            int     not null check (seq >= 1),
    phase                          text    not null check (phase in ('activation', 'recovery', 'reconstitution')),
    title                          text    not null check (length(title) >= 1),
    instructions                   text,
    owner_role_id                  uuid,
    expected_duration_minutes      int check (expected_duration_minutes >= 0),
    verification                   text,
    depends_on                     uuid[]  not null default '{}',
    is_decision_point              boolean not null default false,
    requires_authorization_role_id uuid,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    constraint runbook_step_seq unique (runbook_id, seq) deferrable initially deferred,
    foreign key (tenant_id, runbook_id) references runbook (tenant_id, id) on delete cascade,
    foreign key (tenant_id, owner_role_id) references role (tenant_id, id),
    foreign key (tenant_id, requires_authorization_role_id) references role (tenant_id, id)
);

-- ───────────────────────────── Roles & communication ─────────────────────────────
create table role_assignment (
    id               uuid primary key,
    tenant_id        uuid    not null references tenant (id) on delete cascade,
    service_id       uuid    not null,
    role_id          uuid    not null,
    person_id        uuid    not null,
    is_deputy        boolean not null default false,
    escalation_order int check (escalation_order >= 1),
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    unique (service_id, role_id, person_id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade,
    foreign key (tenant_id, role_id) references role (tenant_id, id),
    foreign key (tenant_id, person_id) references person (tenant_id, id)
);

create table communication_rule (
    id                  uuid primary key,
    tenant_id           uuid not null references tenant (id) on delete cascade,
    service_id          uuid not null,
    trigger             text not null,
    audience            text not null,
    channel             text not null,
    frequency_minutes   int check (frequency_minutes >= 1),
    responsible_role_id uuid not null,
    authorizer_role_id  uuid,
    template            text,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade,
    foreign key (tenant_id, responsible_role_id) references role (tenant_id, id),
    foreign key (tenant_id, authorizer_role_id) references role (tenant_id, id)
);

-- ───────────────────────────── Workflow & plan versions ─────────────────────────────
create table workflow_progress (
    tenant_id    uuid        not null references tenant (id) on delete cascade,
    service_id   uuid        not null,
    step_key     text        not null,
    status       text        not null check (status in ('not_started', 'in_progress', 'complete', 'needs_review')),
    comment      text,
    completed_by text,
    completed_at timestamptz,
    updated_at   timestamptz not null default now(),
    primary key (service_id, step_key),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade
);

create table plan_version (
    id              uuid primary key,
    tenant_id       uuid        not null references tenant (id) on delete cascade,
    service_id      uuid,
    plan_number     int         not null check (plan_number >= 1),
    status          text        not null check (status in ('draft', 'in_review', 'approved', 'retired')),
    snapshot        jsonb       not null,
    submitted_by    text        not null,
    submitted_at    timestamptz not null default now(),
    approved_by     text,
    approved_at     timestamptz,
    next_review_due date,
    comment         text,
    review_comment  text,
    unique (tenant_id, id),
    unique (service_id, plan_number),
    -- Approved versions are kept for audit when the service is deleted.
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete set null (service_id)
);
create unique index plan_version_one_in_review on plan_version (service_id) where status = 'in_review';
create unique index plan_version_one_approved on plan_version (service_id) where status = 'approved';

-- ───────────────────────────── Action items & DR tests ─────────────────────────────
create table action_item (
    id                  uuid primary key,
    tenant_id           uuid not null references tenant (id) on delete cascade,
    service_id          uuid not null,
    source              text not null,
    source_id           uuid,
    title               text not null check (length(title) >= 1),
    description         text,
    owner_person_id     uuid,
    due_date            date,
    status              text not null default 'open' check (status in ('open', 'in_progress', 'done', 'wont_fix')),
    related_entity_type text,
    related_entity_id   uuid,
    origin           text not null default 'user',
    ai_suggestion_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade,
    foreign key (tenant_id, owner_person_id) references person (tenant_id, id) on delete set null (owner_person_id)
);
create index action_item_service on action_item (service_id);

alter table recovery_strategy
    add foreign key (tenant_id, accepted_gap_action_item_id) references action_item (tenant_id, id)
        on delete set null (accepted_gap_action_item_id);

create table dr_test (
    id              uuid primary key,
    tenant_id       uuid   not null references tenant (id) on delete cascade,
    service_id      uuid   not null,
    type            text   not null,
    scenario_id     uuid   not null,
    plan_version_id uuid,
    planned_at      timestamptz not null,
    executed_at     timestamptz,
    participant_ids uuid[] not null default '{}',
    outcome         text   not null default 'planned',
    report          text,
    recovery_run_id uuid,
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade,
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id),
    foreign key (tenant_id, plan_version_id) references plan_version (tenant_id, id)
);
create index dr_test_service on dr_test (service_id);

create table dr_test_result (
    tenant_id            uuid not null references tenant (id) on delete cascade,
    dr_test_id           uuid not null,
    microservice_id      uuid not null,
    target_rto_minutes   int,
    target_rpo_minutes   int,
    achieved_rto_minutes int check (achieved_rto_minutes >= 0),
    achieved_rpo_minutes int check (achieved_rpo_minutes >= 0),
    notes                text,
    primary key (dr_test_id, microservice_id),
    foreign key (tenant_id, dr_test_id) references dr_test (tenant_id, id) on delete cascade,
    foreign key (tenant_id, microservice_id) references microservice (tenant_id, id) on delete cascade
);

-- ───────────────────────────── AI suggestions ─────────────────────────────
create table ai_suggestion (
    id               uuid primary key,
    tenant_id        uuid        not null references tenant (id) on delete cascade,
    service_id       uuid        not null,
    kind             text        not null,
    status           text        not null,
    microservice_id  uuid,
    scenario_id      uuid,
    runbook_id       uuid,
    recovery_run_id  uuid,
    runbook_step_id  uuid,
    prompt           text,
    language         text        not null default 'en',
    proposals        jsonb       not null default '[]',
    summary          text,
    model            text,
    error            text,
    created_by       text        not null,
    created_at       timestamptz not null default now(),
    decided_by       text,
    decided_at       timestamptz,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete cascade
);
create index ai_suggestion_service on ai_suggestion (tenant_id, service_id, created_at);

-- ───────────────────────────── Recovery runs ─────────────────────────────
create table recovery_run (
    id                              uuid primary key,
    tenant_id                       uuid        not null references tenant (id) on delete cascade,
    service_id                      uuid,
    plan_version_id                 uuid        not null,
    scenario_id                     uuid,
    dr_test_id                      uuid,
    mode                            text        not null check (mode in ('real', 'test')),
    status                          text        not null,
    microservice_ids                uuid[]      not null default '{}',
    declared_by                     text        not null,
    declared_at                     timestamptz not null default now(),
    recovered_at                    timestamptz,
    closed_at                       timestamptz,
    outcome                         text,
    summary                         text,
    note                            text,
    service_rto_minutes             int,
    mtpd_minutes                    int,
    status_update_frequency_minutes int,
    unique (tenant_id, id),
    foreign key (tenant_id, service_id) references it_service (tenant_id, id) on delete set null (service_id),
    foreign key (tenant_id, plan_version_id) references plan_version (tenant_id, id),
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id) on delete set null (scenario_id),
    foreign key (tenant_id, dr_test_id) references dr_test (tenant_id, id) on delete set null (dr_test_id)
);
create index recovery_run_service on recovery_run (service_id, declared_at);
create unique index recovery_run_one_active on recovery_run (service_id)
    where status in ('declared', 'in_progress', 'recovered', 'reconstituting');

alter table dr_test
    add foreign key (tenant_id, recovery_run_id) references recovery_run (tenant_id, id)
        on delete set null (recovery_run_id);

-- Step states are copies of the pinned plan snapshot, so they carry no FK to runbook_step.
create table run_step_state (
    tenant_id                      uuid    not null references tenant (id) on delete cascade,
    run_id                         uuid    not null,
    runbook_step_id                uuid    not null,
    runbook_id                     uuid    not null,
    microservice_id                uuid    not null,
    ord                            int     not null,
    seq                            int     not null,
    phase                          text    not null,
    title                          text    not null,
    instructions                   text,
    verification                   text,
    owner_role_id                  uuid,
    expected_duration_minutes      int,
    depends_on                     uuid[]  not null default '{}',
    is_decision_point              boolean not null default false,
    requires_authorization_role_id uuid,
    status                         text    not null default 'pending',
    assignee_person_id             uuid,
    started_at                     timestamptz,
    finished_at                    timestamptz,
    note                           text,
    primary key (run_id, runbook_step_id),
    foreign key (tenant_id, run_id) references recovery_run (tenant_id, id) on delete cascade
);

create table run_event (
    id        bigint generated always as identity primary key,
    tenant_id uuid        not null references tenant (id) on delete cascade,
    run_id    uuid        not null,
    at        timestamptz not null default now(),
    actor     text        not null,
    type      text        not null,
    message   text,
    payload   jsonb,
    foreign key (tenant_id, run_id) references recovery_run (tenant_id, id) on delete cascade
);
create index run_event_run on run_event (run_id, id);

-- ───────────────────────────── RLS & audit wiring ─────────────────────────────
-- tenant, tenant_member and audit_log stay without RLS: they are read across tenants for membership
-- lookup and tenant creation, and are always filtered explicitly by the application.
call enable_tenant_rls('person');
call enable_tenant_rls('role');
call enable_tenant_rls('it_service');
call enable_tenant_rls('business_impact_analysis');
call enable_tenant_rls('impact_rating');
call enable_tenant_rls('microservice');
call enable_tenant_rls('scenario');
call enable_tenant_rls('scenario_microservice');
call enable_tenant_rls('dependency');
call enable_tenant_rls('recovery_objective');
call enable_tenant_rls('recovery_strategy');
call enable_tenant_rls('data_protection');
call enable_tenant_rls('runbook');
call enable_tenant_rls('runbook_step');
call enable_tenant_rls('role_assignment');
call enable_tenant_rls('communication_rule');
call enable_tenant_rls('workflow_progress');
call enable_tenant_rls('plan_version');
call enable_tenant_rls('action_item');
call enable_tenant_rls('dr_test');
call enable_tenant_rls('dr_test_result');
call enable_tenant_rls('ai_suggestion');
call enable_tenant_rls('recovery_run');
call enable_tenant_rls('run_step_state');
call enable_tenant_rls('run_event');

call enable_audit('tenant');
call enable_audit('tenant_member');
call enable_audit('person');
call enable_audit('role');
call enable_audit('it_service');
call enable_audit('business_impact_analysis');
call enable_audit('impact_rating');
call enable_audit('microservice');
call enable_audit('scenario');
call enable_audit('scenario_microservice');
call enable_audit('dependency');
call enable_audit('recovery_objective');
call enable_audit('recovery_strategy');
call enable_audit('data_protection');
call enable_audit('runbook');
call enable_audit('runbook_step');
call enable_audit('role_assignment');
call enable_audit('communication_rule');
call enable_audit('plan_version');
call enable_audit('action_item');
call enable_audit('dr_test');
call enable_audit('dr_test_result');
call enable_audit('ai_suggestion');
call enable_audit('recovery_run');
call enable_audit('run_step_state');
