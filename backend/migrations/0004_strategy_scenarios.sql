-- A recovery strategy ("measure", Maßnahme) can cover several scenarios of its IT service
-- (e.g. one backup restore for "database failure" and "ransomware").

create table recovery_strategy_scenario (
    tenant_id   uuid not null references tenant (id) on delete cascade,
    strategy_id uuid not null,
    scenario_id uuid not null,
    primary key (strategy_id, scenario_id),
    foreign key (tenant_id, strategy_id) references recovery_strategy (tenant_id, id) on delete cascade,
    -- A scenario that is covered by a measure cannot be deleted (409), as before.
    foreign key (tenant_id, scenario_id) references scenario (tenant_id, id)
);
create index recovery_strategy_scenario_scenario on recovery_strategy_scenario (scenario_id);
call enable_tenant_rls('recovery_strategy_scenario');
call enable_audit('recovery_strategy_scenario');

-- Backfill the single scenario of each strategy. Forced RLS would hide all rows from the app role.
alter table recovery_strategy no force row level security;
alter table recovery_strategy_scenario no force row level security;
insert into recovery_strategy_scenario (tenant_id, strategy_id, scenario_id)
select tenant_id, id, scenario_id from recovery_strategy;
alter table recovery_strategy_scenario force row level security;
alter table recovery_strategy force row level security;

-- "One selected strategy per component and scenario" is now enforced when selecting (application).
drop index recovery_strategy_one_selected;
alter table recovery_strategy drop column scenario_id;
