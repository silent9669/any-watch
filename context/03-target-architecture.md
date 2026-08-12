# Target any-watch architecture

## Decision

Replace the legacy React/Vite, Rust/Axum, SQLite, and Tauri stack with a
web-first modular monolith:

- Nuxt 4 and Vue 3 for the responsive PWA and TV browser interface.
- Go for HTTP APIs, provider orchestration, scheduled work, and media grants.
- PostgreSQL for durable account, catalog mapping, library, and audit data.
- Valkey for rate limits, short-lived sessions, cache coordination, and bounded
  background job state.
- Docker Compose and Caddy on the existing VM, with Cloudflare handling DNS/TLS
  edge configuration.

The Nuxt output is embedded or served alongside the Go application so production
does not require a separately managed frontend process.

## Boundaries

```text
browser/PWA/TV -> Caddy -> Go API -> PostgreSQL
                              -> Valkey
                              -> provider adapters and metadata services
```

- Providers run behind a versioned adapter contract and cannot return secrets to
  the browser.
- Durable data lives only in PostgreSQL; rebuildable caches have explicit TTLs.
- Valkey holds only expiring or reconstructable data.
- Media preparation is bounded separately from ordinary API work.
- Caddy is the only public listener; PostgreSQL and Valkey are private.

## Scale path

Start with one application replica. Move to additional replicas only after
PostgreSQL, Valkey, readiness checks, connection limits, and media-plane
boundaries are measured. The initial 100-user target applies to browsing and
account activity, not unlimited proxied video streams.

## Coexistence rule

The legacy provider core is valuable migration inventory, not a reason to keep
the old stack. Port it adapter-by-adapter with certification and retain the old
service/data snapshot until the new system passes cutover acceptance.
