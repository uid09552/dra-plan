#!/bin/sh
# Keycloak's own database and role. Runs on first initialization of the volume and again on every
# `make up` (`make db-sync`), so a changed KEYCLOAK_DB_PASSWORD in .env is applied to an existing
# volume too. Idempotent. The password is passed as a psql variable (quoted by psql, never interpolated
# into SQL text).
set -eu
psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER:-postgres}" -v pw="$KEYCLOAK_DB_PASSWORD" <<'SQL'
select 'create role keycloak login' where not exists (select from pg_roles where rolname = 'keycloak') \gexec
alter role keycloak login password :'pw';
select 'create database keycloak owner keycloak' where not exists (select from pg_database where datname = 'keycloak') \gexec
SQL
