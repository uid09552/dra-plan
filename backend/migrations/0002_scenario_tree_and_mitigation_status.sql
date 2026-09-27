-- Feature increment (docs/requirements/README.md):
-- * scenario brainstorming as a tree (sub-scenarios) with the 15 requirement categories
-- * recovery capability of mitigations: implementation status and last test date

-- Sub-scenarios (mind map). Deleting a parent keeps its children as top-level scenarios.
alter table scenario add column parent_scenario_id uuid;
alter table scenario
    add foreign key (tenant_id, parent_scenario_id) references scenario (tenant_id, id)
        on delete set null (parent_scenario_id);
alter table scenario add check (parent_scenario_id is null or parent_scenario_id <> id);
create index scenario_parent on scenario (parent_scenario_id);

-- Scenario categories as listed in the requirements (was: 7 coarse categories).
-- Row-level security is forced for the table owner too; lift it for this data fix only
-- (the migration runs in one transaction, so no tenant query can observe the gap).
alter table scenario no force row level security;
update scenario set category = case category
    when 'data' then 'database'
    when 'dependency' then 'supplier'
    when 'security' then 'cybersecurity'
    when 'people_facility' then 'people'
    else category
end
where category in ('data', 'dependency', 'security', 'people_facility');
alter table scenario force row level security;

alter table scenario add constraint scenario_category_check check (category is null or category in (
    'infrastructure', 'hardware', 'network', 'cloud', 'application', 'database', 'storage', 'backup',
    'cybersecurity', 'people', 'supplier', 'facility', 'power', 'environmental', 'operational'));

-- Recovery capability: is the mitigation actually in place, and when was it last tested?
alter table recovery_strategy
    add column implementation_status text not null default 'not_implemented'
        check (implementation_status in ('not_implemented', 'in_progress', 'implemented')),
    add column last_tested_at timestamptz;
