#!/bin/sh
# Creates Keycloak's own database and role (runs once, on first initialization of the volume).
set -eu
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" <<SQL
create role keycloak login password '${KEYCLOAK_DB_PASSWORD}';
create database keycloak owner keycloak;
SQL
