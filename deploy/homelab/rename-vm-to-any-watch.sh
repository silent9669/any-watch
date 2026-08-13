#!/usr/bin/env bash
# Rename the ani-desk homelab to any-watch on the host (root-only steps).
# Idempotent: safe to run from either /srv/ani-desk or /srv/any-watch.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NEW_ROOT="/srv/any-watch"
OLD_ROOT="/srv/ani-desk"

if [ -d "$OLD_ROOT" ] && [ ! -d "$NEW_ROOT" ]; then
  mv "$OLD_ROOT" "$NEW_ROOT"
  echo "Moved $OLD_ROOT -> $NEW_ROOT"
fi

APP_DIR="$NEW_ROOT/app"
if [ ! -d "$APP_DIR" ]; then
  echo "error: expected app directory at $APP_DIR" >&2
  exit 1
fi

if [ -f "$NEW_ROOT/config/ani-desk.env" ] && [ ! -f "$NEW_ROOT/config/any-watch.env" ]; then
  sed -e 's/ANI_DESK_/ANY_WATCH_/g' -e 's/ani-desk/any-watch/g' \
    "$NEW_ROOT/config/ani-desk.env" > "$NEW_ROOT/config/any-watch.env"
  rm "$NEW_ROOT/config/ani-desk.env"
  echo "Renamed config env file and keys -> any-watch.env"
fi

for old in ani-desk-ddns.env ani-desk-cloudflare-ddns.env; do
  new="${old/ani-desk/any-watch}"
  if [ -f "/etc/$old" ] && [ ! -f "/etc/$new" ]; then
    mv "/etc/$old" "/etc/$new"
    echo "Moved /etc/$old -> /etc/$new"
  fi
done

for unit in any-watch-ddns.service any-watch-ddns.timer \
  any-watch-cloudflare-ddns.service any-watch-cloudflare-ddns.timer; do
  if [ -f "$SCRIPT_DIR/$unit" ]; then
    cp "$SCRIPT_DIR/$unit" "/etc/systemd/system/$unit"
    chmod 644 "/etc/systemd/system/$unit"
    echo "Installed unit: $unit"
  fi
done

for old in /etc/systemd/system/ani-desk-ddns.service /etc/systemd/system/ani-desk-ddns.timer \
  /etc/systemd/system/ani-desk-cloudflare-ddns.service /etc/systemd/system/ani-desk-cloudflare-ddns.timer \
  /etc/systemd/system/ani-desk-deploy.service /etc/systemd/system/ani-desk-deploy.timer; do
  if [ -e "$old" ]; then
    systemctl disable "$(basename "$old")" >/dev/null 2>&1 || true
    rm -f "$old"
    echo "Removed old unit: $(basename "$old")"
  fi
done

systemctl daemon-reload
systemctl enable --now any-watch-cloudflare-ddns.timer >/dev/null
echo "Enabled any-watch-cloudflare-ddns.timer"

if [ "$(hostname)" != "any-watch-prod" ]; then
  hostnamectl set-hostname any-watch-prod
  echo "Hostname set to any-watch-prod (SSH alias will change; use 192.168.1.181 meanwhile)"
fi

COMPOSE_FILE="$APP_DIR/deploy/homelab/compose.yml"
if [ -f "$COMPOSE_FILE" ]; then
  docker compose -p homelab -f "$COMPOSE_FILE" up -d --build
  echo "Recreated homelab stack from $COMPOSE_FILE"
fi

echo "Rename complete."