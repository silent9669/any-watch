# any-watch Cloudflare automatic maintenance fallback

`failover-worker.js` runs as a Worker Route on `ani.dangphuc.me/*`. The DNS
record remains an orange-clouded `A` record pointing at the homelab. Calling
`fetch(request)` from a Worker Route reaches that configured origin.

For each request, the Worker:

1. gives the homelab origin four seconds to respond;
2. returns every non-5xx origin response unchanged, including authentication
   failures and provider errors;
3. returns a stable JSON `503` for API calls when the origin is unavailable;
4. serves navigation and static assets from the independent GitHub Pages
   artifact at `https://silent9669.github.io/any-watch/`;
5. adds `X-Ani-Desk-Mode: app` or `maintenance` for verification.

Cookies and authorization headers are sent only to the normal ani-desk origin.
Fallback requests are reconstructed with an `Accept` header and do not disclose
account credentials to GitHub Pages.

## Cloudflare configuration

1. Manage the `dangphuc.me` zone with Cloudflare nameservers.
2. Keep `ani` as an orange-clouded `A` record pointing to the homelab address.
3. Create a Worker from `failover-worker.js`.
4. Add the Worker Route `ani.dangphuc.me/*` for the `dangphuc.me` zone.

Moving the authoritative nameservers disables Namecheap Dynamic DNS. Install
`deploy/homelab/cloudflare-ddns.sh` as `/usr/local/sbin/cloudflare-ddns`, install
the matching service and timer units, and store these values with mode `0600`
in `/etc/ani-desk-cloudflare-ddns.env`:

```sh
CLOUDFLARE_API_TOKEN=RESTRICTED_DNS_EDIT_TOKEN
CLOUDFLARE_ZONE_ID=ZONE_ID
CLOUDFLARE_DNS_RECORD_ID=ANI_A_RECORD_ID
CLOUDFLARE_DNS_NAME=ani.dangphuc.me
```

The token needs only Zone DNS Edit permission for `dangphuc.me`. Run the new
service successfully before disabling `ani-desk-ddns.timer`. Keep the old unit
installed but disabled so rolling the nameservers back to Namecheap is quick.

Do not configure `ani.dangphuc.me` as a Worker Custom Domain: this deployment
has a real external origin and therefore uses a Worker Route.

## Acceptance and rollback

With the origin online:

```bash
curl -fsS -D - -o /dev/null https://ani.dangphuc.me/ \
  | grep -i '^x-ani-desk-mode: app'
```

During a controlled origin stop, `/` must return the maintenance page with
`X-Ani-Desk-Mode: maintenance`, while `/api/health` returns JSON `503` with the
same header. Restart the origin and confirm the header returns to `app`.

Worker-only rollback requires removing the Worker Route; the proxied DNS record
then continues directly to the same homelab origin through Cloudflare. A full
DNS rollback requires restoring Namecheap BasicDNS nameservers, disabling the
Cloudflare DDNS timer, and re-enabling the Namecheap DDNS timer.
