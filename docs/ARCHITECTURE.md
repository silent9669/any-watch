# any-watch architecture

## Status

This is the target architecture. The legacy `ani-web` implementation remains
live until the side-by-side migration in [ANY_WATCH_MIGRATION.md](ANY_WATCH_MIGRATION.md)
is accepted.

## Runtime topology

```text
Authenticated browser or TV browser
  -> Cloudflare DNS/TLS edge
  -> Caddy
  -> any-watch application
       -> embedded Nuxt PWA assets
       -> Go HTTP API
       -> bounded background worker
       -> PostgreSQL
       -> Valkey
       -> metadata and approved provider integrations
```

Only Caddy publishes host ports. PostgreSQL and Valkey remain on the private
Compose network. The application never returns provider credentials, cookies,
required upstream headers, or raw signed media URLs to the browser. Provider
ports must work without importing browser cookies or completing manual upstream
authorization; application sessions remain account-only.

## Components

| Component | Responsibility |
| --- | --- |
| Nuxt PWA | SSR-ready Vue UI, installability, responsive and TV navigation, client state |
| Go API | Authentication, catalog, account library, provider orchestration, playback authorization |
| Go worker | Cache refresh, provider health checks, source certification, scheduled maintenance |
| PostgreSQL | Users, roles, sessions, library, watch progress, canonical titles, mappings, audit records |
| Valkey | Rate limits, short-lived sessions, cache coordination, bounded job state |
| Caddy | TLS, reverse proxy, security headers, compression, access logs |
| Cloudflare | DNS, TLS edge, static-cache policy, maintenance routing only |

## Domain model

`title` is the canonical discovery identity. `offering` is a source-specific
representation of a title. `episode` belongs to an offering and preserves its
opaque source ID. `playback` is an authorized, time-bound attempt for a specific
account, offering, episode, and provider capability.

This permits a title to show multiple viewing options while preserving the rule
that providers are never silently merged or substituted.

## Provider boundary

Providers implement a versioned contract:

1. Identity, region/language, capabilities, and unauthenticated web requirements.
2. Search, details, episodes, and source-specific opaque identifiers.
3. Playback preparation or platform handoff.
4. Subtitle, audio, quality, and media-type metadata when available.
5. Health probes, bounded timeouts, circuit breaking, and safe failure codes.

The legacy Rust adapters are migration candidates. Each port is independently
certified before activation. The complete disposition is in
[PROVIDER_INVENTORY.md](PROVIDER_INVENTORY.md).

## Account and browser contract

- The login screen is the only unauthenticated route.
- Administrators manage family accounts under Settings.
- Passwords use Argon2id. Session cookies are Secure, HttpOnly, SameSite=Lax,
  same-origin, revocable, and stored server-side.
- Unsafe requests require CSRF origin validation and an application request
  header.
- Progress persists on pause, seek, episode change, close, and a bounded cadence
  while playing.
- PWA caching is limited to shell/assets and safe catalog data. It never caches
  sessions, account responses, playback grants, or provider credentials.

## Capacity boundary

The initial VM has 2 vCPU, 3.8 GiB RAM, and limited local disk. It is suitable
for approximately 100 concurrent browsing users after load testing. It is not a
general media CDN: concurrent proxied video streams must be explicitly capped
and measured. Prefer platform-supported embeds or handoffs over relaying
third-party media through the VM.

## Legacy coexistence

The old Rust/React/Tauri service, SQLite databases, current Compose deployment,
and maintenance fallback remain operational references during migration. They
are not modified or removed by documentation work.
