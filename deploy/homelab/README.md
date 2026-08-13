# any-watch homelab deployment

This deployment runs any-watch behind Caddy with automatic HTTPS. Only Caddy
binds host ports; the application is reachable only on the private Docker
network.

## First deployment

1. Copy this repository to `/srv/any-watch/app` on the VM.
2. Copy `.env.example` to `.env`, set a long unique admin password, and protect
   the file with mode `0600`.
3. Create `/srv/any-watch/data` and keep it on the persistent data disk.
4. From the repository root run the explicit commands below. The build context
   must be the repository root because that is where `Dockerfile` and
   `tokens.css` live.

   ```sh
   docker compose --env-file deploy/homelab/.env \
     -f deploy/homelab/compose.yml build any-watch
   docker compose --env-file deploy/homelab/.env \
     -f deploy/homelab/compose.yml up -d
   ```
5. Check `docker compose ps`, `docker compose logs --tail=100`, and
   `curl -fsS https://ani.dangphuc.me/api/health`.

## Update and rollback

Before each update, archive `/srv/any-watch/data`. Pull the intended Git commit,
run `docker compose build`, and recreate the services. To roll back, check out
the previous known-good commit and rebuild. Restore the data archive only when
the new version changed or damaged persistent state.

Never commit `.env`, database files, Caddy certificates, or backup archives.

## Dynamic DNS after Cloudflare cutover

When `dangphuc.me` is delegated to Cloudflare, Namecheap Dynamic DNS no longer
controls the authoritative record. Install the Cloudflare updater and its units:

```sh
sudo install -m 0755 deploy/homelab/cloudflare-ddns.sh /usr/local/sbin/cloudflare-ddns
sudo install -m 0644 deploy/homelab/any-watch-cloudflare-ddns.service /etc/systemd/system/
sudo install -m 0644 deploy/homelab/any-watch-cloudflare-ddns.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now any-watch-cloudflare-ddns.timer
sudo systemctl start any-watch-cloudflare-ddns.service
```

Create `/etc/any-watch-cloudflare-ddns.env` with mode `0600`; the required values
and least-privilege token scope are documented in `deploy/cloudflare/README.md`.
Confirm the new service succeeds before disabling `any-watch-ddns.timer`.

### User-data preservation contract

The Compose service bind-mounts `${ANY_WATCH_DATA_PATH:-/srv/any-watch/data}` at
`/data`. Accounts, password hashes, favorites, watch history, and sessions live
in that mounted directory rather than in the replaceable container image.

- Keep `ANY_WATCH_DATA_PATH` pointed at the same persistent host directory for
  every deploy and rollback.
- Use `docker compose up -d`; do not add `-v` to `docker compose down` and do
  not delete the data directory during an application update.
- Changing the configured administrator username or password migrates the one
  protected administrator row in place. Its stable user ID keeps favorites and
  watch history attached; existing sessions are revoked only when credentials
  change.
- Manual deployment builds the replacement image before the maintenance
  window. Stop the application only long enough to archive the data directory
  and recreate the container.

Before a manual update, record the database health and row counts:

```sh
python3 - <<'PY'
import sqlite3
with sqlite3.connect("file:/srv/any-watch/data/web.db?mode=ro", uri=True) as db:
    print("integrity", db.execute("PRAGMA integrity_check").fetchone()[0])
    for table in ("users", "user_favorites", "user_history"):
        print(table, db.execute("SELECT count(*) FROM " + table).fetchone()[0])
PY
```

Run the same commands after the container is healthy. The integrity result must
be `ok`, and the counts must not decrease unless an administrator deliberately
removed those records.

## Manual deployment only

GitHub Actions validates every push and pull request, but it does not connect
to the homelab and the VM does not poll GitHub. Review the successful CI run,
then deploy an exact 40-character commit SHA during a maintenance window.

```sh
cd /srv/any-watch/app
git fetch origin master
git checkout --detach REVIEWED_40_CHARACTER_SHA

docker compose --env-file /srv/any-watch/config/any-watch.env \
  -f deploy/homelab/compose.yml build any-watch

python3 deploy/homelab/data-guard.py snapshot \
  /srv/any-watch/data/web.db
docker compose --env-file /srv/any-watch/config/any-watch.env \
  -f deploy/homelab/compose.yml stop any-watch
install -d -m 0750 /srv/any-watch/backups
backup_path="/srv/any-watch/backups/manual-$(date -u +%Y%m%dT%H%M%SZ).tar.gz"
tar -C /srv/any-watch -czf "$backup_path" data
ls -lh "$backup_path"
docker compose --env-file /srv/any-watch/config/any-watch.env \
  -f deploy/homelab/compose.yml up -d any-watch caddy
curl -fsS https://ani.dangphuc.me/api/health
python3 deploy/homelab/data-guard.py snapshot \
  /srv/any-watch/data/web.db
```

The second snapshot must report `integrity: ok`, and protected row counts must
not decrease. Also verify the public Cloudflare path: `/` must report
`X-Any-Watch-Mode: app`, authenticated `/api/providers/health` must return the
complete provider snapshot, and `/status.json` must report `mode: online` with
`detectedAtIso: null`. There is deliberately no application deployment cron
entry or systemd timer.

## Dockerfile not found

`open Dockerfile: no such file or directory` means Docker received the wrong
build context or an incorrect `-f` path. Do not run
`docker build deploy/homelab` or use `deploy/homelab/Dockerfile` because that
file does not exist. From the repository root, either run the Compose command
above or build the application image directly with:

```sh
docker build --file Dockerfile --tag any-watch-homelab:test .
```

## Catalog connectivity

The runtime image prefers IPv4-mapped addresses. This is intentional for the
homelab VM, which has working IPv4 internet access but no routed IPv6. Without
that preference, AniList may resolve to IPv6 first and catalog requests can fail
immediately even though the same build works on Railway.

## Browser login and stale UI

The hosted shell sends `Cache-Control: private, no-cache`, so Safari revalidates
the current build instead of keeping an old login or layout bundle. If one
browser still rejects a known-good account:

1. use one exact origin (for example, do not alternate between an IP address,
   hostname, and public domain because their cookies are separate);
2. reload without content blockers and reveal the password field to verify what
   Safari Passwords actually filled;
3. check recent application logs for `INVALID_CREDENTIALS` or rate limiting;
4. verify the API directly without placing the password in shell history:

   ```sh
   read -r ANI_USER
   read -rs ANI_PASSWORD
   curl -i -c /tmp/any-watch-cookie.txt \
     -H 'Content-Type: application/json' \
     -H 'X-Any-Watch-Request: 1' \
     --data "$(jq -nc --arg username "$ANI_USER" --arg password "$ANI_PASSWORD" '{username:$username,password:$password}')" \
     https://YOUR_DOMAIN/api/login
   unset ANI_USER ANI_PASSWORD
   ```

Do not reset or replace `web.db` to repair a browser-only login problem. The
configured protected administrator is migrated in place, and the persistent
data directory must remain mounted during every redeploy.

## Provider monitoring

Provider health is a snapshot, not a permanent disable switch. Authenticated
`GET /api/providers/health` is cached for five minutes, concurrent stale
requests share one refresh, and each provider check is bounded to 60 seconds.
Failed initial `unknown` checks become retryable `unavailable` entries, and an
explicit provider-health retry can restore a healthy source. OPhim uses
`https://ophim1.com/v1/api`, allows a 20-second upstream response window, and
validates HTTP status before parsing results. MovieBox signs the request
separately for each of its API mirrors and fails over between `api4`, `api5`,
and `api6` when a regional endpoint is unreachable.

Run full provider certification only before a release or during a focused
incident; it performs real upstream searches and media byte-range checks:

```sh
cargo run --example provider_certification -- --require-english
```

For continuous monitoring, probe `/api/health` every minute and run provider
certification at most once or twice daily to avoid noisy or rate-limited traffic.
Cloudflare allows aggregate provider health 70 seconds and does not treat an
endpoint failure as evidence that the whole site requires maintenance.
