-- Components (table `microservice`) are optional for the user: every IT service has exactly one
-- default component that stands for the service as a whole. Objectives, strategies and runbooks of
-- scenarios without explicitly affected components attach to it (docs/adr/0011-optional-components.md).

alter table microservice add column is_default boolean not null default false;
create unique index microservice_one_default on microservice (tenant_id, service_id) where is_default;

-- Created in the same transaction as the service, so no write path can miss it.
create function create_default_component() returns trigger language plpgsql as $$
begin
    insert into microservice (id, tenant_id, service_id, name, is_default, created_by, updated_by)
    values (gen_random_uuid(), new.tenant_id, new.id, new.name, true, new.created_by, new.created_by);
    return new;
end;
$$;

create trigger it_service_default_component
    after insert on it_service
    for each row execute function create_default_component();

-- Backfill existing services. Forced row-level security would hide all rows from the app role, so
-- lift it for this statement only (inside the migration transaction).
alter table it_service no force row level security;
alter table microservice no force row level security;
insert into microservice (id, tenant_id, service_id, name, is_default, created_by, updated_by)
select gen_random_uuid(), s.tenant_id, s.id, s.name, true, 'system', 'system'
from it_service s
where not exists (select 1 from microservice m where m.service_id = s.id and m.is_default);
alter table microservice force row level security;
alter table it_service force row level security;
