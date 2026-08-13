# any-watch architecture

## Runtime topology

```text
Authenticated desktop, mobile, or TV browser
  -> Cloudflare DNS/TLS edge
  -> Caddy
  -> Rust/Axum service
       -> React/Vite static assets
       -> same-origin /api routes
       -> provider orchestration and opaque media proxy
       -> SQLite account and library data
       -> AniList, AniSkip, and admitted providers
```

Only the reverse proxy publishes host ports. The application serves the browser
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
| Cloudflare | DNS, TLS edge, static-cache policy, and maintenance routing |

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

## Persistence compatibility

The service retains existing SQLite schemas, data directories, cookie behavior,
`ANY_WATCH_*` deployment variables, and legacy local-storage fallback keys. The
obsolete downloads table may remain in upgraded databases but is not used by the
browser interface.

## Capacity boundary

The initial VM has 2 vCPU, 3.8 GiB RAM, and limited local disk. It is suitable
for a private family service, not a general media CDN. Provider calls and proxied
streams require bounded timeouts, health checks, and concurrency controls.
