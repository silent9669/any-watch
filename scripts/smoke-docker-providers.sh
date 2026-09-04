#!/usr/bin/env bash
set -euo pipefail

IMAGE="${ANY_WATCH_SMOKE_IMAGE:-any-watch:provider-verification}"
CONTAINER="${ANY_WATCH_SMOKE_CONTAINER:-any-watch-provider-smoke}"
PORT="${ANY_WATCH_SMOKE_PORT:-3299}"
BASE_URL="http://127.0.0.1:${PORT}"
PASSWORD="Provider-Smoke-Password-2026"
COOKIE_JAR="$(mktemp)"
SEGMENT="$(mktemp)"
LIVE_PROVIDERS="${ANY_WATCH_SMOKE_LIVE_PROVIDERS:-0}"
INVIDIOUS_URL="${ANY_WATCH_SMOKE_INVIDIOUS_URL:-}"

PYTHON_BIN="${PYTHON_BIN:-}"
if [[ -z "$PYTHON_BIN" ]]; then
  PYTHON_BIN="$(command -v python3 || command -v python || true)"
  PYTHON_BIN="${PYTHON_BIN:-python3}"
fi

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  rm -f "$COOKIE_JAR" "$SEGMENT"
}
trap cleanup EXIT

docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
docker_args=(
  --detach
  --name "$CONTAINER"
  --publish "127.0.0.1:${PORT}:3000"
  --env ANY_WATCH_ADMIN_USERNAME=provider_admin
  --env "ANY_WATCH_ADMIN_PASSWORD=${PASSWORD}"
  --env ANY_WATCH_SECURE_COOKIES=0
)
if [[ -n "$INVIDIOUS_URL" ]]; then
  docker_args+=(--env "ANY_WATCH_INVIDIOUS_URL=${INVIDIOUS_URL}")
fi
docker run "${docker_args[@]}" "$IMAGE" >/dev/null

for _ in {1..60}; do
  if curl --fail --silent "${BASE_URL}/api/health" >/dev/null; then
    break
  fi
  sleep 1
done
curl --fail --silent "${BASE_URL}/api/health" >/dev/null

curl --fail --silent \
  --cookie-jar "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data "{\"username\":\"provider_admin\",\"password\":\"${PASSWORD}\"}" \
  "${BASE_URL}/api/login" >/dev/null

sources="$(curl --fail --silent --cookie "$COOKIE_JAR" "${BASE_URL}/api/sources")"
"$PYTHON_BIN" - "$sources" "$INVIDIOUS_URL" <<'PY'
import json
import sys

names = [source["name"] for source in json.loads(sys.argv[1])]
expected_base = ["AniDB", "Anikoto", "AnimeHeaven", "AnimeGG", "KKPhim", "OPhim", "Niniyo", "K20"]
expected = ["Invidious"] + expected_base if "Invidious" in names else expected_base
assert names == expected, {"actual": names, "expected": expected}
PY

if [ "$LIVE_PROVIDERS" != "1" ]; then
  echo "Docker container smoke test passed (authenticated configured provider catalog)."
  exit 0
fi

health="$(curl --fail --silent --cookie "$COOKIE_JAR" "${BASE_URL}/api/providers/health")"
"$PYTHON_BIN" - "$health" <<'PY'
import json
import sys

states = {source["name"]: source["status"] for source in json.loads(sys.argv[1])}
assert states["OPhim"] == "healthy", states
assert states["Niniyo"] == "healthy", states
PY

search="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"source":"OPhim","query":"Đảo Hải Tặc"}' \
  "${BASE_URL}/api/source/search")"
anime_id="$("$PYTHON_BIN" - "$search" <<'PY'
import json
import sys

print(next(item["id"] for item in json.loads(sys.argv[1]) if item["title"] == "Đảo Hải Tặc"))
PY
)"

episodes="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data "{\"provider\":\"OPhim\",\"animeId\":\"${anime_id}\"}" \
  "${BASE_URL}/api/anime/episodes")"
episode_id="$("$PYTHON_BIN" - "$episodes" <<'PY'
import json
import sys

episodes = json.loads(sys.argv[1])
assert len(episodes) > 1000, len(episodes)
print(episodes[-2]["id"])
PY
)"

playback="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data "{\"provider\":\"OPhim\",\"episodeId\":\"${episode_id}\"}" \
  "${BASE_URL}/api/playback")"
playback_url="$("$PYTHON_BIN" - "$playback" <<'PY'
import json
import sys

playback = json.loads(sys.argv[1])
assert playback["streamKind"] == "hls", playback
assert playback["playbackUrl"].startswith("/api/media/"), playback
assert "http" not in playback["playbackUrl"], playback
print(playback["playbackUrl"])
PY
)"

manifest="$(curl --fail --silent --cookie "$COOKIE_JAR" "${BASE_URL}${playback_url}")"
resource_path="$("$PYTHON_BIN" - "$manifest" <<'PY'
import sys

manifest = sys.argv[1]
assert manifest.startswith("#EXTM3U"), manifest[:80]
assert "opstream" not in manifest.lower(), manifest
print(next(line.strip() for line in manifest.splitlines() if line.strip() and not line.startswith("#")))
PY
)"
media_playlist="$(curl --fail --silent --cookie "$COOKIE_JAR" "${BASE_URL}${resource_path}")"
segment_path="$("$PYTHON_BIN" - "$media_playlist" <<'PY'
import sys

playlist = sys.argv[1]
assert playlist.startswith("#EXTM3U"), playlist[:80]
assert "opstream" not in playlist.lower(), playlist
print(next(line.strip() for line in playlist.splitlines() if line.strip() and not line.startswith("#")))
PY
)"
curl --fail --silent --range 0-4095 --cookie "$COOKIE_JAR" \
  "${BASE_URL}${segment_path}" --output "$SEGMENT"
test "$(wc -c < "$SEGMENT")" -gt 0

search="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data '{"source":"AniDB","query":"One Piece"}' \
  "${BASE_URL}/api/source/search")"
anime_id="$("$PYTHON_BIN" - "$search" <<'PY'
import json
import sys

print(next(item["id"] for item in json.loads(sys.argv[1]) if item["title"].casefold() == "one piece"))
PY
)"
episodes="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data "{\"provider\":\"AniDB\",\"animeId\":\"${anime_id}\"}" \
  "${BASE_URL}/api/anime/episodes")"
episode_id="$("$PYTHON_BIN" - "$episodes" <<'PY'
import json
import sys

episodes = json.loads(sys.argv[1])
assert len(episodes) > 1000, len(episodes)
assert episodes[-1]["number"] == len(episodes), episodes[-1]
print(episodes[-1]["id"])
PY
)"
playback="$(curl --fail --silent \
  --cookie "$COOKIE_JAR" \
  --header 'Content-Type: application/json' \
  --header 'X-Any-Watch-Request: 1' \
  --data "{\"provider\":\"AniDB\",\"episodeId\":\"${episode_id}\"}" \
  "${BASE_URL}/api/playback")"
playback_url="$("$PYTHON_BIN" - "$playback" <<'PY'
import json
import sys

playback = json.loads(sys.argv[1])
assert playback["streamKind"] == "hls", playback
assert playback["playbackUrl"].startswith("/api/media/"), playback
assert "http" not in playback["playbackUrl"], playback
print(playback["playbackUrl"])
PY
)"
manifest="$(curl --fail --silent --cookie "$COOKIE_JAR" "${BASE_URL}${playback_url}")"
test "${manifest:0:7}" = "#EXTM3U"

echo "Docker provider certification passed (opaque OPhim proxy and AniDB HLS)."
