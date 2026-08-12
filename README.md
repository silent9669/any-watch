# any-watch

Private, account-only viewing for family members across desktop, mobile, and TV
browsers. `any-watch` is the planned successor to `ani-web`; the current Rust,
React, and Tauri implementation remains the live system until a staged migration
is accepted.

**Current service:** [ani.dangphuc.me](https://ani.dangphuc.me)

## Product direction

- Accounts are created, disabled, and reset only by an administrator.
- Unauthenticated visitors see the login screen only. There is no guest mode.
- Watch history, progress, My List, language, and playback preferences belong to
  the signed-in account and work across browsers.
- The replacement is a Nuxt PWA with a Go API, PostgreSQL, Valkey, Caddy, and
  Cloudflare. It targets modern browsers and remote-controlled TV browsers.
- Discovery is title-first: a title can expose multiple independently identified
  viewing options without silently merging or switching providers.

## Current providers and migration

The current implementation contains MovieBox, KKPhim, OPhim, Niniyo, AllAnime,
and AnimeGG adapters, plus unregistered or stub integrations. They are migration
inventory, not a guarantee of availability. Each adapter must be ported behind a
versioned provider contract, retain opaque source and episode IDs, and pass
search, episode, playback, subtitle, and failure-path certification before it is
enabled in `any-watch`.

Provider protocol constants, credentials, cookies, raw media URLs, and required
upstream headers remain server-side. New source integrations require a documented
authorization or platform-supported integration method. See
[docs/PROVIDER_INVENTORY.md](docs/PROVIDER_INVENTORY.md).

## Migration documents

- [Architecture](docs/ARCHITECTURE.md)
- [Migration plan](docs/ANY_WATCH_MIGRATION.md)
- [Provider inventory](docs/PROVIDER_INVENTORY.md)
- [Design system](design.md)
- [Current-system inventory](context/02-current-system.md)

## Repository status

This repository currently contains the legacy application and its operational
assets. Documentation defines the replacement; no runtime migration has started.
Do not remove the legacy code, databases, or deployment configuration until the
side-by-side cutover and rollback gates in the migration plan pass.
