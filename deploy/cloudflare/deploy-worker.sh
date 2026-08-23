#!/usr/bin/env bash
# Deploy deploy/cloudflare/failover-worker.js to Cloudflare Workers.
# Needs: node + npx (wrangler), and a workers-capable API token.
#
# Token sources, in order:
#   1. $CLOUDFLARE_API_TOKEN already exported
#   2. /etc/any-watch-worker.env (if present and readable)
#   3. The VM at 192.168.1.181 (reads root-owned /etc/any-watch-worker.env
#      via the docker-group trick; needs ~/.ssh/ani-desk-homelab key)
set -euo pipefail
cd "$(dirname "$0")"

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  if [[ -r /etc/any-watch-worker.env ]]; then
    set -a; source /etc/any-watch-worker.env; set +a
  else
    ssh dangphuc@192.168.1.66 \
      'docker run --rm -v /:/host alpine cat /host/etc/any-watch-worker.env' \
      > /tmp/any-watch-worker.env
    set -a; source /tmp/any-watch-worker.env; rm -f /tmp/any-watch-worker.env; set +a
  fi
fi

export CLOUDFLARE_API_TOKEN
export CLOUDFLARE_ACCOUNT_ID="${CLOUDFLARE_ACCOUNT_ID:?missing}"
npx --yes wrangler@latest deploy
