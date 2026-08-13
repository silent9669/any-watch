# any-watch deployment handbook

## Current release state

- Production deploys use the exact green commit from the `master` branch. The deployed SHA is recorded in `/srv/ani-desk/app/.release-sha`.
- GitHub Actions runs only for `master` pushes or manual dispatches. It validates quality/API behavior, browser E2E, dependency audit, and the production image build.
- CI does not call live providers. Provider certification is an explicit pre-deploy or incident-response operation because upstream availability is outside repository control.
- Persistent data is intact: 10 users, 1 favorite, and 41 history rows; SQLite integrity is `ok`.
- Current internal and public health return `{"service":"any-watch","status":"ok"}`.

## Architecture

- Caddy is the only service binding host ports 80 and 443.
- `ani-desk` is reachable only on the Docker `web` network at port 3000.
- User accounts, sessions, favorites, and history are persisted at `/srv/ani-desk/data`.
- Caddy certificates and configuration use named Docker volumes. Do not remove those volumes during an application deploy.
- Cloudflare fronts `ani.dangphuc.me`; its DDNS timer keeps the origin address current.
- The `ani-desk-failover` Worker Route remains `ani.dangphuc.me/*`. It serves the independent maintenance artifact from `https://silent9669.github.io/any-watch/` when the origin times out or returns a 5xx response.
- Recreating Caddy or its certificates does not change the Worker Route. Caddy must still present a valid certificate for `ani.dangphuc.me` so Cloudflare can reach the origin.

## Required safeguards

1. Review a green CI run for the exact 40-character release SHA.
2. Snapshot `/srv/ani-desk/data/web.db` and require `integrity: ok`.
3. Archive `/srv/ani-desk/data` before replacing containers.
4. Retain the previous application tree or image for rollback.
5. Never use `docker compose down -v`, delete `/srv/ani-desk/data`, or remove Caddy volumes during a normal release.
6. Do not put passwords, SSH private keys, Cloudflare tokens, or cookies in commands, logs, commits, or this handbook.

## Deploy procedure

Run as an account that can modify every file under `/srv/ani-desk/app`, or obtain controlled sudo access for the application checkout only.

```sh
set -euo pipefail
APP=/srv/ani-desk/app
ENV=/srv/ani-desk/config/ani-desk.env
DATA=/srv/ani-desk/data
BACKUPS=/srv/ani-desk/backups
SHA=REVIEWED_40_CHARACTER_SHA

install -d -m 0750 "$BACKUPS"
cd "$APP"
git fetch origin master
git checkout --detach "$SHA"

before="$(python3 deploy/homelab/data-guard.py snapshot "$DATA/web.db")"
printf '%s\n' "$before" > "$BACKUPS/pre-deploy-${SHA:0:12}.json"

docker compose --env-file "$ENV" -f deploy/homelab/compose.yml build ani-desk
docker compose --env-file "$ENV" -f deploy/homelab/compose.yml stop ani-desk

backup_path="$BACKUPS/manual-$(date -u +%Y%m%dT%H%M%SZ)-${SHA:0:12}.tar.gz"
tar -C /srv/ani-desk -czf "$backup_path" data
test -s "$backup_path"

docker compose --env-file "$ENV" -f deploy/homelab/compose.yml up -d ani-desk caddy
docker compose --env-file "$ENV" -f deploy/homelab/compose.yml ps

after="$(python3 deploy/homelab/data-guard.py snapshot "$DATA/web.db")"
python3 deploy/homelab/data-guard.py verify "$before" "$after"
```

## Post-deploy verification

```sh
curl -kfsS --resolve ani.dangphuc.me:443:127.0.0.1 \
  https://ani.dangphuc.me/api/health
curl -fsS https://ani.dangphuc.me/api/health
docker compose --env-file /srv/ani-desk/config/ani-desk.env \
  -f /srv/ani-desk/app/deploy/homelab/compose.yml ps
```

Expected health response:

```json
{"service":"any-watch","status":"ok"}
```

The post-deploy SQLite snapshot must report `integrity: ok` and user, favorite, and history counts must not decrease.

## Provider certification

- Standard CI does not perform live provider health, search, or playback calls.
- Before a provider-specific release, run its ignored live test explicitly. AniZone certification verifies search, regular episode resolution, HLS playback, and English subtitle extraction; AniDB certification verifies search, regular episode resolution, and Japanese-audio HLS playback:

  ```sh
  cargo test --test providers_live test_anizone_live_playback -- --ignored --nocapture
  cargo test --test providers_live test_anizone_live_health -- --ignored --nocapture
  cargo test --test providers_live test_anidb_live_playback -- --ignored --nocapture
  cargo test --test providers_live test_anidb_live_health -- --ignored --nocapture
  ```

- Production currently enables AniZone and AniDB for English and OPhim, KKPhim, and Niniyo for Vietnamese. AniDB uses reqwest's native TLS backend and a neutral `ani-desk/1.0` user agent because `anidb.app` challenges browser-claiming user agents issued by non-browser TLS stacks. A provider is selectable only while its application health check is healthy.
- A provider can become unavailable because of rate limits, regional routing, or upstream changes. Treat that as an operational incident, not a reason to delete user data or bypass access controls.

## Cloudflare failover

The Worker source is `deploy/cloudflare/failover-worker.js`. Deploy it to the existing script and route rather than creating a custom domain:

```sh
npx wrangler deploy deploy/cloudflare/failover-worker.js \
  --name ani-desk-failover \
  --route 'ani.dangphuc.me/*' \
  --compatibility-date 2026-08-13
```

Online verification must include `X-Ani-Desk-Mode: app`:

```sh
curl -fsS -D - -o /dev/null https://ani.dangphuc.me/ \
  | grep -i '^x-ani-desk-mode: app'
```

For a controlled failover test, stop Caddy briefly. `/` must serve the maintenance page with `X-Ani-Desk-Mode: maintenance`, while `/api/health` must return JSON `503` with the same mode header. Restart Caddy immediately and confirm the mode returns to `app`.

## Monitoring

- Probe `https://ani.dangphuc.me/api/health` at least once per minute.
- Review `docker logs --tail 100 homelab-ani-desk-1` after deploys and during incidents.
- AniList `429` and provider `502` errors are upstream failures. Record their time and provider, then retry at a controlled rate.
- Keep Cloudflare DDNS timer enabled and check its latest service result after networking changes.

## Rollback

1. Stop only `ani-desk`.
2. Restore the previous known-good application tree or image.
3. Rebuild and run `docker compose up -d ani-desk caddy` without `-v`.
4. Verify internal/public health and database integrity/counts.
5. Restore the data archive only if the application change actually damaged persistent state.

## Host access

The application checkout is owned by `dangphuc`. Docker access is available through the `docker` group; privileged host cleanup requires sudo. Keep SSH private keys on operator machines and install only their public keys in `~/.ssh/authorized_keys`.
