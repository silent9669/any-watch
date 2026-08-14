# any-watch deployment handbook

## Current release state

- Production deploys use the exact green commit from the `master` branch. The deployed SHA is recorded in `/srv/any-watch/app/.release-sha`.
- CI runs for `master` pushes or manual dispatches. It validates quality/API behavior, browser E2E, dependency audit, and the production image build. The immutable GitHub Pages maintenance shell deploys only when maintenance assets or their workflow change, or through manual dispatch.
- CI does not call live providers. Provider certification is an explicit pre-deploy or incident-response operation because upstream availability is outside repository control.
- Persistent data counts are deployment-specific. Record a pre-deploy snapshot and require that user, favorite, and history counts do not decrease unexpectedly; SQLite integrity must remain `ok`.
- Current internal and public health return `{"service":"any-watch","status":"ok"}`.

## Architecture

- Caddy is the only service binding host ports 80 and 443.
- `any-watch` is reachable only on the Docker `web` network at port 3000.
- User accounts, sessions, favorites, and history are persisted at `/srv/any-watch/data`.
- Caddy certificates and configuration use named Docker volumes. Do not remove those volumes during an application deploy.
- Cloudflare fronts `ani.dangphuc.me`; its DDNS timer keeps the origin address current.
- The `any-watch-failover` Worker Route remains `ani.dangphuc.me/*`. Ordinary requests receive a four-second origin window; `/api/providers/health` receives 70 seconds and its endpoint errors never select whole-site maintenance. Ordinary origin timeouts and 5xx responses serve the independent maintenance shell from `https://silent9669.github.io/any-watch/`.
- The hostname-scoped `OUTAGE_STATE` Durable Object globally preserves the first Worker-observed outage time. Worker-served `/status.json` probes `/api/health`, reports that stable timestamp during the incident, and clears it after recovery.
- Recreating Caddy or its certificates does not change the Worker Route. Caddy must still present a valid certificate for `ani.dangphuc.me` so Cloudflare can reach the origin.

## Required safeguards

1. Review a green CI run for the exact 40-character release SHA.
2. Snapshot `/srv/any-watch/data/web.db` and require `integrity: ok`.
3. Archive `/srv/any-watch/data` before replacing containers.
4. Retain an archive of the previous application source and the prior image for rollback.
5. Never use `docker compose down -v`, delete `/srv/any-watch/data`, or remove Caddy volumes during a normal release.
6. Do not put passwords, SSH private keys, Cloudflare tokens, or cookies in commands, logs, commits, or this handbook.

An incident deployment from a dirty or uncommitted worktree is not a complete
release. After restoring service, commit the exact source, obtain green CI, and
redeploy that reviewed SHA so `/srv/any-watch/app/.release-sha`, Git history,
and the running image describe the same release.

## Deploy procedure

`/srv/any-watch/app` is a user-owned release tree, not a Git checkout. Its parent
is root-owned, so the deployment does not replace `app` with a symlink. Build an
immutable archive from the reviewed commit, stage and build it under the
user-owned `deployer` directory, archive the active source, and only then rsync
the candidate into `app` while the application container is stopped.

On the workstation:

```sh
set -euo pipefail
SHA=REVIEWED_40_CHARACTER_SHA
git diff --quiet
git diff --cached --quiet
test "$(git rev-parse "$SHA")" = "$SHA"
git archive --format=tar.gz --output="/tmp/any-watch-${SHA}.tar.gz" "$SHA"
rsync -av "/tmp/any-watch-${SHA}.tar.gz" \
  dangphuc@192.168.1.181:/srv/any-watch/deployer/
```

On the VM, run as `dangphuc`, which owns `app`, `backups`, and `deployer`:

```sh
set -euo pipefail
ROOT=/srv/any-watch
APP="$ROOT/app"
ENV=/srv/any-watch/config/any-watch.env
DATA=/srv/any-watch/data
BACKUPS=/srv/any-watch/backups
DEPLOYER=/srv/any-watch/deployer
SHA=REVIEWED_40_CHARACTER_SHA
NEXT="$DEPLOYER/releases/$SHA"
COMPOSE="docker compose --project-name homelab --env-file $ENV --file $APP/deploy/homelab/compose.yml"
CANDIDATE_COMPOSE="docker compose --project-name homelab --env-file $ENV --file $NEXT/deploy/homelab/compose.yml"

install -d -m 0750 "$BACKUPS" "$DEPLOYER/releases"
test ! -e "$NEXT"
install -d -m 0750 "$NEXT"
tar -xzf "$DEPLOYER/any-watch-${SHA}.tar.gz" -C "$NEXT"
$CANDIDATE_COMPOSE build any-watch

before="$(python3 "$APP/deploy/homelab/data-guard.py" snapshot "$DATA/web.db")"
test "$(printf '%s' "$before" | jq -r .exists)" = true
test "$(printf '%s' "$before" | jq -r .integrity)" = ok
printf '%s\n' "$before" > "$BACKUPS/pre-deploy-${SHA:0:12}.json"

$COMPOSE stop any-watch

backup_path="$BACKUPS/manual-$(date -u +%Y%m%dT%H%M%SZ)-${SHA:0:12}.tar.gz"
tar -C "$ROOT" -czf "$backup_path" data
tar -tzf "$backup_path" >/dev/null
sha256sum "$backup_path" > "$backup_path.sha256"

source_backup="$BACKUPS/source-$(date -u +%Y%m%dT%H%M%SZ)-${SHA:0:12}.tar.gz"
tar --exclude=./target -C "$APP" -czf "$source_backup" .
tar -tzf "$source_backup" >/dev/null
sha256sum "$source_backup" > "$source_backup.sha256"

rsync -a --delete --exclude target/ "$NEXT/" "$APP/"
$COMPOSE up -d --force-recreate any-watch
$COMPOSE ps

after="$(python3 "$APP/deploy/homelab/data-guard.py" snapshot "$DATA/web.db")"
python3 "$APP/deploy/homelab/data-guard.py" verify "$before" "$after"
printf '%s\n' "$SHA" > "$APP/.release-sha.tmp"
mv "$APP/.release-sha.tmp" "$APP/.release-sha"
```

Retain the source and data archives until post-deploy verification is complete.
Copy backups to encrypted off-host storage; `data-guard.py` validates SQLite
integrity and non-decreasing user/favorite/history counts, but it is not a
backup tool and does not validate sessions, `catalog.db`, or archive contents.
The disposable `target/` build cache is excluded because prior containerized
builds may leave root-owned cache locks in the user-owned release tree.

## Manual restart

```sh
COMPOSE="docker compose --project-name homelab \
  --env-file /srv/any-watch/config/any-watch.env \
  --file /srv/any-watch/app/deploy/homelab/compose.yml"

$COMPOSE restart any-watch
$COMPOSE up -d --force-recreate any-watch  # changed image or environment
```

Do not restart Caddy for an application-only restart or rollback. Stop Caddy
only for a deliberate failover test, then restore it with `$COMPOSE up -d caddy`.

## Post-deploy verification

```sh
curl -kfsS --resolve ani.dangphuc.me:443:127.0.0.1 \
  https://ani.dangphuc.me/api/health
curl -fsS https://ani.dangphuc.me/api/health
docker compose --env-file /srv/any-watch/config/any-watch.env \
  -f /srv/any-watch/app/deploy/homelab/compose.yml ps
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

- Production defaults enable AniZone, AniDB, and AnimeGG for English and OPhim, KKPhim, and Niniyo for Vietnamese. MovieBox is disabled because its current DASH stream is HEVC-only. AniZone and AniDB fetch through the system `curl` binary with a neutral `any-watch/1.0` user agent - never a browser-claiming UA or reqwest TLS stack - because their upstream network policies reject or challenge the reqwest transport. A provider is selectable only while its application health check is healthy.
- A provider can become unavailable because of rate limits, regional routing, or upstream changes. Treat that as an operational incident, not a reason to delete user data or bypass access controls.
- Authenticated `GET /api/providers/health` results are cached for five minutes, and concurrent stale requests share one refresh. Each provider check is bounded to 60 seconds. If the initial UI health request fails while entries are still `unknown`, they become retryable `unavailable` entries instead of remaining indefinitely in `Checking`.
- AniSkip certification must verify the exact catalog ID, provider episode
  number, and stream duration. Run
  `cargo test skip_times::tests::live_one_piece_skip_times_smoke -- --ignored --nocapture`
  plus the provider playback test. Missing upstream timing is not a failure and
  must not be replaced with an adjacent episode's timing.

## Cloudflare failover

Deploy the Worker through its checked-in script and Wrangler configuration. Do not reconstruct the deployment with ad hoc flags or paste only the JavaScript into the dashboard; `wrangler.toml` includes the `OUTAGE_STATE` binding and migration:

```sh
npm run cloudflare:test
deploy/cloudflare/deploy-worker.sh
cd deploy/cloudflare
npx --yes wrangler@latest deployments list
npx --yes wrangler@latest tail any-watch-failover
```

Worker deployment needs `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID`. The checked-in Wrangler configuration declares the
Durable Object binding and migration, but the `ani.dangphuc.me/*` Worker Route
is managed separately. Do not delete or rewrite migration `v1` during rollback.

Online verification must include `X-Any-Watch-Mode: app`:

```sh
curl -fsS -D - -o /dev/null https://ani.dangphuc.me/ \
  | grep -i '^x-any-watch-mode: app'
```

Production validation must confirm:

- `/` returns `X-Any-Watch-Mode: app` while the origin is healthy;
- authenticated `/api/providers/health` can exceed four seconds, up to the 70-second edge allowance, without selecting maintenance;
- during a controlled Caddy stop, `/` serves the static shell and ordinary API routes return JSON `503`;
- `/status.json` reports one stable `detectedAtIso` throughout the outage; and
- after recovery, `/status.json` returns `mode: online`, clears `detectedAtIso`, and `/` returns app mode again.

## Monitoring

- Probe `https://ani.dangphuc.me/api/health` at least once per minute.
- Review `docker logs --tail 100 homelab-any-watch-1` after deploys and during incidents.
- AniList `429` and provider `502` errors are upstream failures. Record their time and provider, then retry at a controlled rate.
- Keep Cloudflare DDNS timer enabled and check its latest service result after networking changes.

DDNS diagnostics:

```sh
systemctl status any-watch-cloudflare-ddns.timer --no-pager
systemctl status any-watch-cloudflare-ddns.service --no-pager
systemctl list-timers any-watch-cloudflare-ddns.timer --all
journalctl -u any-watch-cloudflare-ddns.service -n 100 --no-pager
sudo systemctl start any-watch-cloudflare-ddns.service
curl -fsS --max-time 10 https://api.ipify.org
```

The timer runs every ten minutes with up to one minute of randomized delay.
Because the DNS record is proxied, public `dig` output shows Cloudflare edge
addresses; inspect the stored origin value through the Cloudflare API/dashboard.

## Rollback

1. Stop only `any-watch`.
2. Restore the previous known-good application tree or image.
3. Rebuild and run `docker compose up -d any-watch` without `-v`.
4. Verify internal/public health and database integrity/counts.
5. Restore the data archive only if the application change actually damaged persistent state.

Code-only rollback from the retained source archive:

```sh
SOURCE_BACKUP=/srv/any-watch/backups/SELECTED_SOURCE_BACKUP.tar.gz
ROLLBACK=/srv/any-watch/deployer/rollback-$(date -u +%Y%m%dT%H%M%SZ)
COMPOSE="docker compose --project-name homelab \
  --env-file /srv/any-watch/config/any-watch.env \
  --file /srv/any-watch/app/deploy/homelab/compose.yml"
install -d -m 0750 "$ROLLBACK"
tar -xzf "$SOURCE_BACKUP" -C "$ROLLBACK"
$COMPOSE stop any-watch
rsync -a --delete --exclude target/ "$ROLLBACK/" /srv/any-watch/app/
$COMPOSE build any-watch
$COMPOSE up -d --force-recreate any-watch
```

For data restore, first stop `any-watch` and archive the failed state:

```sh
DATA_BACKUP=/srv/any-watch/backups/SELECTED_DATA_BACKUP.tar.gz
RESTORE=/srv/any-watch/deployer/data-restore-$(date -u +%Y%m%dT%H%M%SZ)
COMPOSE="docker compose --project-name homelab \
  --env-file /srv/any-watch/config/any-watch.env \
  --file /srv/any-watch/app/deploy/homelab/compose.yml"
$COMPOSE stop any-watch
tar -C /srv/any-watch -czf \
  "/srv/any-watch/backups/data-failed-$(date -u +%Y%m%dT%H%M%SZ).tar.gz" data
install -d -m 0750 "$RESTORE"
tar -xzf "$DATA_BACKUP" -C "$RESTORE"
rsync -a --delete "$RESTORE/data/" /srv/any-watch/data/
$COMPOSE up -d any-watch
python3 /srv/any-watch/app/deploy/homelab/data-guard.py snapshot \
  /srv/any-watch/data/web.db
```

For a Worker-only rollback, redeploy the previous known-good Worker commit with
its matching `wrangler.toml`, then repeat the production validation. If a safe
redeploy is unavailable, remove the Worker Route temporarily so proxied traffic
reaches the unchanged homelab origin directly. Do not delete the `OUTAGE_STATE`
namespace or migration during a routine script rollback.

## Host access

The application checkout is owned by `dangphuc`. Docker access is available through the `docker` group; privileged host cleanup requires sudo. Keep SSH private keys on operator machines and install only their public keys in `~/.ssh/authorized_keys`.
