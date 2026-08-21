# any-watch Cloudflare automatic maintenance fallback

`failover-worker.js` runs as a Worker Route on `ani.dangphuc.me/*`. The DNS
record remains an orange-clouded `A` record pointing at the homelab. Calling
`fetch(request)` from a Worker Route reaches that configured origin.

For each request, the Worker:

1. gives navigation, assets, and `/api/health` four seconds for response headers,
   authenticated API routes 65 seconds, and `/api/providers/health` 70 seconds;
   after headers arrive, streaming response bodies are not aborted by that timer;
2. returns typed JSON API errors unchanged while treating non-JSON proxy 5xx
   responses as origin outages;
3. returns a stable JSON `503` for API calls when the origin is unavailable;
4. serves navigation and static assets from the independent GitHub Pages
   artifact at `https://silent9669.github.io/any-watch/`;
5. returns a controlled plain-text `503` when that maintenance origin also fails,
   or returns a non-OK response for navigation;
6. adds `X-Any-Watch-Mode: app` or `maintenance` for verification.

The double-outage response uses `Cache-Control: no-store`, `Retry-After: 60`,
`Content-Type: text/plain; charset=utf-8`, and
`X-Any-Watch-Mode: maintenance`. Its body is
`Maintenance page is temporarily unavailable.`

The hostname-scoped `OUTAGE_STATE` Durable Object globally records the first
failure observed by any Worker location. Worker-served `/status.json` probes
`/api/health`, returns that stable ISO timestamp to all visitors during the
incident, and clears it after recovery. `maintenance/status.json` is only a
timestamp-free default for direct GitHub Pages previews.

Cookies and authorization headers are sent only to the normal any-watch origin.
Fallback requests are reconstructed with an `Accept` header and do not disclose
account credentials to GitHub Pages.

## Cloudflare configuration

1. Manage the `dangphuc.me` zone with Cloudflare nameservers.
2. Keep `ani` as an orange-clouded `A` record pointing to the homelab address.
3. Deploy `any-watch-failover` with `./deploy-worker.sh`; its checked-in
   `wrangler.toml` supplies the module entry and `OUTAGE_STATE` binding/migration.
4. Add the Worker Route `ani.dangphuc.me/*` for the `dangphuc.me` zone.

Moving the authoritative nameservers disables Namecheap Dynamic DNS. Install
`deploy/homelab/cloudflare-ddns.sh` as `/usr/local/sbin/cloudflare-ddns`, install
the matching service and timer units, and store these values with mode `0600`
in `/etc/any-watch-cloudflare-ddns.env`:

```sh
CLOUDFLARE_API_TOKEN=RESTRICTED_DNS_EDIT_TOKEN
CLOUDFLARE_ZONE_ID=ZONE_ID
CLOUDFLARE_DNS_RECORD_ID=ANI_A_RECORD_ID
CLOUDFLARE_DNS_NAME=ani.dangphuc.me
```

The token needs only Zone DNS Edit permission for `dangphuc.me`. Run the new
service successfully before disabling `any-watch-ddns.timer`. Keep the old unit
installed but disabled so rolling the nameservers back to Namecheap is quick.

### Deploying the Worker

The Worker Route remains bound to `any-watch-failover`. Deploy the script and
its checked-in Wrangler configuration together:

```bash
./deploy-worker.sh
```

`deploy-worker.sh` runs `wrangler deploy` from this directory. `wrangler.toml`
provides the name, module entry, compatibility date, `workers_dev` setting, and
Durable Object migration.
It reads a workers-capable API token from, in order: `$CLOUDFLARE_API_TOKEN`,
`/etc/any-watch-worker.env` on the machine, or the homelab VM at
`192.168.1.181` (root-owned `/etc/any-watch-worker.env`, read via the
docker-group trick). The token on the VM needs Account `Workers Scripts: Edit`
plus Zone `Workers Routes: Edit` on the account hosting the Worker — the
DNS-only DDNS token above cannot deploy Workers.

Do not configure `ani.dangphuc.me` as a Worker Custom Domain: this deployment
has a real external origin and therefore uses a Worker Route.

## Acceptance and rollback

With the origin online:

```bash
curl -fsS -D - -o /dev/null https://ani.dangphuc.me/ \
  | grep -i '^x-any-watch-mode: app'
```

Also validate authenticated provider health through the public route. During a
controlled origin stop, `/` must return the maintenance page with
`X-Any-Watch-Mode: maintenance`, ordinary API routes must return JSON `503`, and
repeated `/status.json` requests must preserve one `detectedAtIso`. Test the
double-failure path separately: when both the application and GitHub Pages
maintenance origins are unavailable, navigation must return the controlled
plain-text `503` with `X-Any-Watch-Mode: maintenance`, `Retry-After: 60`, and
`Cache-Control: no-store`. Restart the origin and confirm `/status.json` returns
`mode: online` with a null `detectedAtIso`, then confirm `/` returns app mode.

Normal Worker rollback redeploys the previous known-good Worker commit with its
matching `wrangler.toml`. Removing the Worker Route is the emergency bypass; the
proxied DNS record then reaches the unchanged homelab origin directly. Do not
delete `OUTAGE_STATE` during a routine rollback. A full DNS rollback requires
restoring Namecheap BasicDNS nameservers, disabling the Cloudflare DDNS timer,
and re-enabling the Namecheap DDNS timer.
