#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ANY_WATCH_SMOKE_PORT:-3199}"
DATA_DIR="$(mktemp -d)"
COOKIE_JAR="$(mktemp)"
VIEWER_COOKIE_JAR="$(mktemp)"
SERVER_LOG="$(mktemp)"
SERVER_PID=""

PYTHON_BIN="${PYTHON_BIN:-}"
if [[ -z "$PYTHON_BIN" ]]; then
  PYTHON_BIN="$(command -v python3 || command -v python || true)"
  PYTHON_BIN="${PYTHON_BIN:-python3}"
fi

SERVER_BIN="$ROOT_DIR/target/debug/any-watch-server"
if [[ -e "${SERVER_BIN}.exe" ]]; then
  SERVER_BIN="${SERVER_BIN}.exe"
fi

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$DATA_DIR" "$COOKIE_JAR" "$VIEWER_COOKIE_JAR" "$SERVER_LOG"
}
trap cleanup EXIT

export ANY_WATCH_ADMIN_USERNAME="smoke_admin"
export ANY_WATCH_ADMIN_PASSWORD="Smoke-Test-Password-2026"
export ANY_WATCH_DATA_DIR="$DATA_DIR"
export ANY_WATCH_WEB_DIR="$ROOT_DIR/web/dist"
export ANY_WATCH_SECURE_COOKIES=0
export PORT

"$SERVER_BIN" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

for _ in {1..60}; do
  if curl --fail --silent "http://127.0.0.1:$PORT/api/health" >/dev/null; then
    break
  fi
  sleep 0.25
done

curl --fail --silent "http://127.0.0.1:$PORT/api/health" >/dev/null || {
  sed -n '1,160p' "$SERVER_LOG" >&2
  exit 1
}

unauthenticated_status="$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$PORT/api/session")"
[[ "$unauthenticated_status" == "401" ]]

curl --fail --silent \
  --cookie-jar "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_admin","password":"Smoke-Test-Password-2026"}' \
  "http://127.0.0.1:$PORT/api/login" >/dev/null

curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_viewer","password":"Smoke-Viewer-Password-2026","role":"user"}' \
  "http://127.0.0.1:$PORT/api/admin/users" >/dev/null

users="$(curl --fail --silent --cookie "$COOKIE_JAR" "http://127.0.0.1:$PORT/api/admin/users")"
"$PYTHON_BIN" - "$users" <<'PY'
import json
import sys

users = json.loads(sys.argv[1])
assert {user["username"] for user in users} == {"smoke_admin", "smoke_viewer"}
root = next(user for user in users if user["username"] == "smoke_admin")
viewer = next(user for user in users if user["username"] == "smoke_viewer")
assert root["protected"] is True
assert viewer["protected"] is False
PY

root_id="$("$PYTHON_BIN" - "$users" <<'PY'
import json, sys
print(next(user["id"] for user in json.loads(sys.argv[1]) if user["username"] == "smoke_admin"))
PY
)"
viewer_id="$("$PYTHON_BIN" - "$users" <<'PY'
import json, sys
print(next(user["id"] for user in json.loads(sys.argv[1]) if user["username"] == "smoke_viewer"))
PY
)"

root_update_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --request PUT \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"changed_root","password":"Changed-Root-Password","role":"user","enabled":false}' \
  "http://127.0.0.1:$PORT/api/admin/users/$root_id")"
[[ "$root_update_status" == "400" ]]

curl --fail --silent \
  --request PUT \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_viewer_renamed","password":"Smoke-Viewer-Updated-2026","role":"user","enabled":true}' \
  "http://127.0.0.1:$PORT/api/admin/users/$viewer_id" >/dev/null

favorite_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"id":"one-piece","catalogId":21,"provider":"AllAnime","title":"One Piece","coverUrl":"https://example.com/one-piece.jpg"}' \
  "http://127.0.0.1:$PORT/api/my-list")"
[[ "$favorite_status" == "204" ]]

favorites="$(curl --fail --silent --cookie "$COOKIE_JAR" "http://127.0.0.1:$PORT/api/my-list")"
"$PYTHON_BIN" - "$favorites" <<'PY'
import json
import sys

favorites = json.loads(sys.argv[1])
assert [item["title"] for item in favorites] == ["One Piece"]
PY

curl --fail --silent \
  --cookie-jar "$VIEWER_COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_viewer_renamed","password":"Smoke-Viewer-Updated-2026"}' \
  "http://127.0.0.1:$PORT/api/login" >/dev/null

viewer_admin_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --cookie "$VIEWER_COOKIE_JAR" \
  "http://127.0.0.1:$PORT/api/admin/users")"
[[ "$viewer_admin_status" == "403" ]]

curl --fail --silent \
  --request PUT \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_viewer_renamed","role":"admin","enabled":true}' \
  "http://127.0.0.1:$PORT/api/admin/users/$viewer_id" >/dev/null

viewer_admin_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --cookie "$VIEWER_COOKIE_JAR" \
  "http://127.0.0.1:$PORT/api/admin/users")"
[[ "$viewer_admin_status" == "200" ]]

curl --fail --silent \
  --request PUT \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"username":"smoke_viewer_renamed","role":"user","enabled":true}' \
  "http://127.0.0.1:$PORT/api/admin/users/$viewer_id" >/dev/null

viewer_favorites="$(curl --fail --silent --cookie "$VIEWER_COOKIE_JAR" "http://127.0.0.1:$PORT/api/my-list")"
"$PYTHON_BIN" - "$viewer_favorites" <<'PY'
import json
import sys

assert json.loads(sys.argv[1]) == []
PY

root_delete_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --request DELETE \
  --cookie "$COOKIE_JAR" \
  --header 'X-Any-Watch-Request: 1' \
  "http://127.0.0.1:$PORT/api/admin/users/$root_id")"
[[ "$root_delete_status" == "400" ]]

viewer_delete_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --request DELETE \
  --cookie "$COOKIE_JAR" \
  --header 'X-Any-Watch-Request: 1' \
  "http://127.0.0.1:$PORT/api/admin/users/$viewer_id")"
[[ "$viewer_delete_status" == "204" ]]

deleted_session_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  --cookie "$VIEWER_COOKIE_JAR" \
  "http://127.0.0.1:$PORT/api/session")"
[[ "$deleted_session_status" == "401" ]]

users="$(curl --fail --silent --cookie "$COOKIE_JAR" "http://127.0.0.1:$PORT/api/admin/users")"
"$PYTHON_BIN" - "$users" <<'PY'
import json
import sys

users = json.loads(sys.argv[1])
assert [user["username"] for user in users] == ["smoke_admin"]
PY

echo "Hosted web smoke test passed"
