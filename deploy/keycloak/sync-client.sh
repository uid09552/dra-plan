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
  -s "attributes.\"post.logout.redirect.uris\"=$UI_URL/*##$DRA_PUBLIC_URL/*"
echo "Keycloak client $DRA_CLIENT_ID: redirect URIs $UI_URL/*, $DRA_PUBLIC_URL/*"
