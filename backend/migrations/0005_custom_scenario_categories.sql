-- Tenant-defined scenario categories, in addition to the 15 built-in ones
-- (docs/requirements/README.md §2). Settings > Categories in the UI.
create table scenario_category (
    id         uuid primary key,
    tenant_id  uuid not null references tenant (id) on delete cascade,
    key        text not null check (key ~ '^[a-z0-9_-]{2,40}$'),
    label      text not null check (length(label) between 1 and 80),
    version    int         not null default 1,
    created_at timestamptz not null default now(),
    created_by text        not null,
    updated_at timestamptz not null default now(),
    updated_by text        not null,
    unique (tenant_id, id),
    unique (tenant_id, key)
);
call enable_tenant_rls('scenario_category');
call enable_audit('scenario_category');

-- scenario.category can now also reference a custom category key; validated in the application
-- (it needs to check against the tenant's custom categories, which a CHECK constraint cannot do).
alter table scenario drop constraint scenario_category_check;
