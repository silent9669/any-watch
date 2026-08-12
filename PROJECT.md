# any-watch project brief

## Purpose

`any-watch` is a private, account-only media-viewing application for family use.
It is a web-first PWA for desktop, mobile, and TV browsers. The project will
replace the legacy `ani-web` implementation through a reversible migration, not
through an in-place rewrite of the live service.

## Target architecture

| Concern | Target |
| --- | --- |
| Client | Nuxt 4, Vue 3, TypeScript, installable PWA |
| API and workers | Go |
| Durable data | PostgreSQL |
| Sessions, rate limits, ephemeral jobs | Valkey |
| Edge | Caddy behind Cloudflare |
| Deployment | Docker Compose on the existing VM |

## Product invariants

- The login page is the only unauthenticated surface.
- Administrators create and manage accounts in a protected Settings section.
- Account data is server-side and isolated by user.
- English and Vietnamese are first-class language preferences.
- Provider, title, and episode identities remain opaque and source-scoped.
- Providers never silently replace one another during search or playback.
- Secrets and media authorization stay on the server.

## Delivery status

| Milestone | Scope | Status |
| --- | --- | --- |
| 1 | Product, architecture, provider, and design documentation | In progress |
| 2 | Nuxt/Go/PostgreSQL/Valkey foundation | Planned |
| 3 | Identity and SQLite-to-PostgreSQL migration | Planned |
| 4 | Provider contract ports and certification | Planned |
| 5 | PWA, TV interaction, player, and account library | Planned |
| 6 | Side-by-side deployment, load tests, and cutover | Planned |

The existing React, Rust, Tauri, desktop packaging, and homelab assets are legacy
inventory. They remain intact until the migration acceptance gates pass.
