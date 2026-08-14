# any-watch architecture

## Runtime topology

```text
Authenticated desktop, mobile, or TV browser
  -> orange-clouded Cloudflare A record and Worker Route
  -> any-watch failover Worker
       -> OUTAGE_STATE Durable Object
       -> dynamic /status.json
       -> GitHub Pages immutable maintenance shell when origin is unavailable
       -> configured external origin -> Caddy TLS
            -> Rust/Axum service
                 -> React/Vite static assets
                 -> same-origin /api routes
                 -> provider orchestration and opaque media proxy
                 -> SQLite account and library data
                 -> AniList, AniSkip, and admitted providers
```

Only the reverse proxy publishes host ports, and host firewall/router policy is
the external exposure boundary. The application serves the browser
client and JSON API from one origin. There is no native runtime, privileged IPC,
desktop updater, MPV process, or application-managed local filesystem library.

## Components

| Component | Responsibility |
| --- | --- |
| React/Vite client | Responsive discovery, account library, settings, playback, and TV navigation |
| Axum service | Authentication, account APIs, provider orchestration, downloads, and media proxying |
| Rust core | Provider adapters, catalog matching, skip timing, configuration, and shared SQLite models |
| SQLite | Users, sessions, My List, watch progress, and existing compatible data |
| Caddy | TLS, reverse proxy, security headers, compression, and access logs |
| Cloudflare Worker | Origin routing, four-second ordinary timeout, 70-second provider-health allowance, dynamic status, and maintenance selection |
| Durable Object `OUTAGE_STATE` | Globally persistent first Worker-observed outage timestamp, cleared on recovery |
| GitHub Pages | Immutable, account-free maintenance shell; it does not own live status |

## Browser contract

- The login screen is the only unauthenticated application state.
- Administrators create, disable, reset, and remove family accounts.
- Session cookies are Secure, HttpOnly, SameSite=Lax, same-origin, revocable,
  and stored server-side.
- Unsafe requests require origin validation and `X-Any-Watch-Request`; the
  historical header name remains for deployed-client compatibility.
- Progress persists on pause, seek, episode change, close, and a bounded cadence.
- Browser downloads use short-lived authenticated tickets and the browser's
  download manager rather than a server-indexed local device library.

## Provider and media boundary

Provider results remain distinct and retain opaque source identifiers. A
canonical catalog match never permits episode IDs or playback to switch sources
silently. Only admitted providers are enabled by default.

Playback preparation stores upstream URLs, cookies, and required headers in a
short-lived server session. HLS playlists and DASH manifests are rewritten to
opaque same-origin resource IDs. Browser JSON, markup, and logs must not expose
raw signed media URLs or reversible encodings of private upstream material.

The curl-backed proxy appends an HTTP metadata trailer after the response body
and splits only on the final trailer marker. The delimiter newline is metadata,
not media; retaining it corrupts encrypted HLS segments. A reachable manifest
is insufficient certification: HLS keys/segments and DASH codecs must also work
in the supported browser matrix. MovieBox remains disabled while its upstream
advertises only `hev1`/HEVC video.

HLS and DASH remain adaptive by default. The browser starts with a moderate
bandwidth estimate, prefetches the first HLS fragment, permits full-resolution
ABR instead of capping to CSS player dimensions, and uses a short DASH startup
buffer with fast quality up-switching. Manual quality selection disables ABR
until the user returns to Auto.

AniSkip is keyed by a canonical AniList title (resolved to MyAnimeList) plus a
provider-certified positive integer episode number. AniZone, AniDB, AnimeGG,
KKPhim, OPhim, and integer-numbered Niniyo episodes expose that mapping; decimal
specials remain unmapped. Direct search keeps a unique exact catalog/query match
attached to the chosen provider without switching providers. Skip intro seeks
only opening ranges whose reported episode length matches the active video
within 20 seconds or three percent. Missing AniSkip submissions are a normal
no-marker state.

Catalog availability sends the canonical title, native title, and synonyms to
provider matching. Direct provider search attaches a catalog ID only after one
unique exact alias match; result order is never sufficient. Titles without a
MyAnimeList mapping and AniSkip `found: false` responses are normal empty states,
while transport/protocol failures remain retryable errors.

`GET /api/providers/health` caches aggregate results for five minutes and
coalesces concurrent stale refreshes. Provider checks run concurrently with a
60-second per-check backend timeout. Cloudflare allows this endpoint 70 seconds
and does not reinterpret provider-health errors as a whole-site outage.
Compose defines no Docker healthcheck, so operational health means a successful
HTTP probe rather than a Docker `healthy` state.

## Persistence compatibility

The service retains existing SQLite schemas, data directories, cookie behavior,
`ANY_WATCH_*` deployment variables, and legacy local-storage fallback keys. The
obsolete downloads table may remain in upgraded databases but is not used by the
browser interface.

## Capacity boundary

The initial VM has 2 vCPU, 3.8 GiB RAM, and limited local disk. It is suitable
for a private family service, not a general media CDN. Provider calls and proxied
streams require bounded timeouts, health checks, and concurrency controls.
