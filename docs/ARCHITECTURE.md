# any-watch architecture

## Runtime topology

```text
Authenticated desktop, mobile, or TV browser
  -> orange-clouded Cloudflare A record and Worker Route
  -> any-watch failover Worker
       -> OUTAGE_STATE Durable Object
       -> dynamic /status.json
       -> GitHub Pages immutable maintenance shell when origin is unavailable
       -> controlled maintenance 503 if both origins are unavailable
       -> configured external origin -> Caddy TLS
            -> Rust/Axum service
                 -> React/Vite static assets
                 -> same-origin /api routes
                 -> provider orchestration and opaque media proxy
                 -> SQLite account and library data
                 -> AniList, AniSkip, admitted anime providers, and optional Invidious
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
| Cloudflare Worker | Origin routing, four-second ordinary timeout, 70-second provider-health allowance, dynamic status, maintenance selection, and controlled double-outage responses |
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

Invidious is a separate, opt-in authentic YouTube experience rather than an anime language
provider. The server calls its documented v1 API for search, details, captions, DASH manifests,
trending feeds (`/api/v1/trending`), popular videos (`/api/v1/popular`), and recommended/related
videos (`/api/v1/videos/{id}`), then applies the same opaque media-session boundary used by
anime providers. The browser never receives the configured instance URL.

The frontend delivers an authentic YouTube watching experience:
- **Topic Filter Bar**: Quick navigation chips (`All`, `Trending`, `Music`, `Gaming`, `News`, `Animations`).
- **Theater Watch Room**: Inline 16:9 player with ambient glow, channel identity, save/share actions, an expandable description box, and an "Up Next / Related Videos" sidebar (`/api/youtube/related/:id`).
- **16:9 Vertical Feed Cards & Horizontal Search Rows**: Vertical cards in feed grids and landscape rows with channel avatars, views, publication dates, and descriptions in search.
- **Dedicated Continue Watching**: 16:9 cards with duration pills, progress bars, and one-click instant resume.

### Provider Search Dashboards and Universal Home Overview

- **Universal Home Dashboard**: Combines continue watching items across all active sources (Anime providers + YouTube) with explicit provider badges and progress tracking.
- **Provider-Specific Search Dashboards**: In the Search view, when the search query is empty (`query.trim().length < 2`), selecting any provider renders that provider's custom dashboard featuring:
  - Hero header with provider icon, status, language scope, and official website link.
  - Dedicated Continue Watching shelf isolated exclusively to history from that provider.
  - Curated Quick Search and Discovery topic tags that populate search with one click.
- When searching (`query.trim().length >= 2`), live search results are displayed with provider-specific result panes.

### K-20 Stremio Addon Integration

K-20 (`https://sc.k-20.xyz`) connects as a native provider following the Stremio Addon standard:
- Multi-catalog search across NguonC, STP, HH3D, VSMOV, and YanHH3D.
- Full metadata and alphanumeric episode parsing.
- Direct HLS/HTTP streaming with subtitle extraction and WebVTT/SRT format conversion.
- Playback through the shared cross-browser pipeline: HLS.js where MediaSource is available and native HLS on Safari/WebKit.

Playback preparation stores upstream URLs, cookies, and required headers in a
short-lived server session. HLS playlists and DASH manifests are rewritten to
opaque same-origin resource IDs. Browser JSON, markup, and logs must not expose
raw signed media URLs or reversible encodings of private upstream material.

The curl-backed proxy appends an HTTP metadata trailer after the response body
and splits only on the final trailer marker. The delimiter newline is metadata,
not media; retaining it corrupts encrypted HLS segments. Curl output is read
incrementally with a 64 MiB resource ceiling, while recognized HLS and DASH
manifests retain the stricter 4 MiB limit used by the reqwest path. A reachable
manifest is insufficient certification: HLS keys/segments and DASH codecs must
also work in the supported browser matrix. MovieBox remains disabled while its
upstream advertises only `hev1`/HEVC video.

HLS and DASH remain adaptive by default. The browser starts with a moderate
bandwidth estimate, prefetches the first HLS fragment, permits full-resolution
ABR instead of capping to CSS player dimensions, and uses a short DASH startup
buffer with fast quality up-switching. Manual quality selection disables ABR
until the user returns to Auto.

AniSkip is keyed by a canonical AniList title (resolved to MyAnimeList) plus a
provider-certified positive integer episode number. AniDB, AnimeGG, KKPhim,
OPhim, and integer-numbered Niniyo episodes expose that mapping by default;
AniZone exposes it when explicitly enabled. Decimal specials remain unmapped.
Direct search keeps a unique exact catalog/query match
attached to the chosen provider without switching providers. Skip intro seeks
only opening ranges whose reported episode length matches the active video
within 20 seconds or three percent. Missing AniSkip submissions are a normal
no-marker state.

Catalog availability sends the canonical title, native title, and synonyms to
provider matching. Direct provider search attaches a catalog ID only after one
unique exact alias match; result order is never sufficient. Titles without a
MyAnimeList mapping and AniSkip `found: false` responses are normal empty states,
while transport/protocol failures remain retryable errors.

Authenticated `GET` and `POST /api/providers/health` refreshes share a
five-minute aggregate cache and coalescing/version protection. Provider checks
run with a maximum concurrency of four and a 60-second per-check backend timeout.
Cloudflare allows this endpoint 70 seconds and does not reinterpret
provider-health errors as a whole-site outage. Compose healthchecks
`/api/health`, and homelab Caddy waits for the application service to become
`healthy` before starting.

## Persistence compatibility

The service retains existing SQLite schemas, data directories, cookie behavior,
`ANY_WATCH_*` deployment variables, and legacy local-storage fallback keys. The
obsolete downloads table may remain in upgraded databases but is not used by the
browser interface.

## Capacity boundary

The initial VM has 2 vCPU, 3.8 GiB RAM, and limited local disk. It is suitable
for a private family service, not a general media CDN. Provider calls and proxied
streams require bounded timeouts, health checks, and concurrency controls.
