#!/bin/bash
# Applies the redirect/logout URLs of the realm client ${DRA_CLIENT_ID} to an existing realm
# (the realm import in realm-dra.json only runs once). Idempotent. Runs inside the Keycloak
# container: docker exec -i dra_keycloak bash < deploy/keycloak/sync-client.sh
set -euo pipefail
kc=/opt/keycloak/bin/kcadm.sh
export HOME=/tmp
$kc config credentials --server http://localhost:8080 --realm master \
  --user "$KC_BOOTSTRAP_ADMIN_USERNAME" --password "$KC_BOOTSTRAP_ADMIN_PASSWORD" >/dev/null
id=$($kc get clients -r "$KEYCLOAK_REALM" -q clientId="$DRA_CLIENT_ID" --fields id --format csv --noquotes)
$kc update "clients/$id" -r "$KEYCLOAK_REALM" \
  -s "redirectUris=[\"$UI_URL/*\",\"$DRA_PUBLIC_URL/*\"]" \
  -s "webOrigins=[\"$UI_URL\",\"$DRA_PUBLIC_URL\"]" \
  -s serviceAccountsEnabled=true \
  -s authorizationServicesEnabled=true \
  -s "attributes.\"post.logout.redirect.uris\"=$UI_URL/*##$DRA_PUBLIC_URL/*"
echo "Keycloak client $DRA_CLIENT_ID: redirect URIs $UI_URL/*, $DRA_PUBLIC_URL/*"

ensure_role() {
  local name=$1 description=$2
  if ! $kc get "roles/$name" -r "$KEYCLOAK_REALM" >/dev/null 2>&1; then
    $kc create roles -r "$KEYCLOAK_REALM" -s "name=$name" -s "description=$description" >/dev/null
  fi
}

ensure_user() {
  local username=$1 email=$2 role=$3 password=${4:-}
  local user_id
  user_id=$($kc get users -r "$KEYCLOAK_REALM" -q "username=$username" --fields id --format csv --noquotes)
  if [[ -z "$user_id" ]]; then
    user_id=$($kc create users -r "$KEYCLOAK_REALM" -s "username=$username" \
      -s enabled=true -s "email=$email" -s emailVerified=true -i)
  fi
  if [[ -n "$password" ]]; then
    $kc set-password -r "$KEYCLOAK_REALM" --userid "$user_id" --new-password "$password" >/dev/null
  fi
  $kc add-roles -r "$KEYCLOAK_REALM" --uid "$user_id" --rolename "$role" >/dev/null
  if ! $kc get "organizations/$org_id/members/$user_id" -r "$KEYCLOAK_REALM" >/dev/null 2>&1; then
    $kc create "organizations/$org_id/members" -r "$KEYCLOAK_REALM" -b "\"$user_id\"" >/dev/null
  fi
}

ensure_account_roles() {
  local username=$1 user_id
  user_id=$($kc get users -r "$KEYCLOAK_REALM" -q "username=$username" --fields id --format csv --noquotes)
  $kc add-roles -r "$KEYCLOAK_REALM" --uid "$user_id" --cclientid account \
    --rolename view-profile --rolename manage-account --rolename manage-account-links >/dev/null
}

ensure_role dra-viewer 'Read-only API access'
ensure_role dra-admin 'Full API access'
org_id=$($kc get organizations -r "$KEYCLOAK_REALM" -q alias=demo --fields id --format csv --noquotes)
ensure_user demo-user demo-user@demo.local dra-viewer
ensure_user demo-admin demo-admin@demo.local dra-admin "${DEMO_ADMIN_PASSWORD:-}"
ensure_account_roles demo-user
ensure_account_roles demo-admin

role_id() {
  $kc get "roles/$1" -r "$KEYCLOAK_REALM" --fields id --format csv --noquotes
}

client_id=$id
resource_id=${DRA_API_RESOURCE_ID:?set DRA_API_RESOURCE_ID}
existing_resource_id=$($kc get "clients/$client_id/authz/resource-server/resource" -r "$KEYCLOAK_REALM" \
  -q name=dra-api --fields _id --format csv --noquotes)
if [[ -n "$existing_resource_id" && "$existing_resource_id" != "$resource_id" ]]; then
  $kc delete "clients/$client_id/authz/resource-server/resource/$existing_resource_id" -r "$KEYCLOAK_REALM"
  existing_resource_id=
fi
if [[ -z "$existing_resource_id" ]]; then
  $kc create "clients/$client_id/authz/resource-server/resource" -r "$KEYCLOAK_REALM" \
    -s "_id=$resource_id" -s name=dra-api -s type=urn:dra-workflow:api \
    -s 'uris=["/api/*"]' \
    -s 'scopes=[{"name":"GET"},{"name":"HEAD"},{"name":"OPTIONS"},{"name":"POST"},{"name":"PUT"},{"name":"PATCH"},{"name":"DELETE"}]' >/dev/null
else
  $kc update "clients/$client_id/authz/resource-server/resource/$resource_id" -r "$KEYCLOAK_REALM" \
    -s name=dra-api -s type=urn:dra-workflow:api -s 'uris=["/api/*"]' \
    -s 'scopes=[{"name":"GET"},{"name":"HEAD"},{"name":"OPTIONS"},{"name":"POST"},{"name":"PUT"},{"name":"PATCH"},{"name":"DELETE"}]'
fi

ensure_role_policy() {
  local name=$1 role=$2 roles_json body policy_id
  policy_id=$($kc get "clients/$client_id/authz/resource-server/policy/role" -r "$KEYCLOAK_REALM" \
    -q name="$name" --fields id --format csv --noquotes)
  if [[ -z "$policy_id" ]]; then
    roles_json=$(printf '[{"id":"%s","required":false}]' "$(role_id "$role")")
    body=$(printf '{"name":"%s","type":"role","logic":"POSITIVE","decisionStrategy":"UNANIMOUS","roles":%s}' "$name" "$roles_json")
    $kc create "clients/$client_id/authz/resource-server/policy/role" -r "$KEYCLOAK_REALM" -b "$body" >/dev/null
  fi
}

ensure_scope_permission() {
  local name=$1 scopes_json=$2 policies_json=$3 permission_id body
  permission_id=$($kc get "clients/$client_id/authz/resource-server/permission/scope" -r "$KEYCLOAK_REALM" \
    -q name="$name" --fields id --format csv --noquotes)
  if [[ -z "$permission_id" ]]; then
    body=$(printf '{"name":"%s","type":"scope","logic":"POSITIVE","decisionStrategy":"AFFIRMATIVE","resources":["%s"],"scopes":%s,"policies":%s}' \
      "$name" "$resource_id" "$scopes_json" "$policies_json")
    $kc create "clients/$client_id/authz/resource-server/permission/scope" -r "$KEYCLOAK_REALM" -b "$body" >/dev/null
  fi
}

ensure_role_policy dra-viewer-policy dra-viewer
ensure_role_policy dra-admin-policy dra-admin
ensure_scope_permission dra-api-read '["GET","HEAD"]' '["dra-viewer-policy","dra-admin-policy"]'
ensure_scope_permission dra-api-write '["POST","PUT","PATCH","DELETE"]' '["dra-admin-policy"]'
