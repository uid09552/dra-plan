-- The application role must NOT be a superuser: superusers bypass row-level security.
create role dra login password 'dra';
create database dra owner dra;
