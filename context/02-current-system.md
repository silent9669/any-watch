# Current system

This is a snapshot of the current checkout, not a promise that every provider is healthy forever.

## Runtime topology

```text
Browser
  -> HTTPS :443
  -> Caddy container
  -> legacy ani-web server :3000 on private Compose network
       -> React/Vite static files
       -> /api Axum routes
       -> web.db (users, sessions, favorites, history)
       -> catalog.db (metadata cache)
       -> AniList and playback-provider HTTP endpoints
```

Only Caddy publishes host ports. The application container is addressable only on the Compose `web` network. Persistent state is bind-mounted at `/data` from `${ANI_DESK_DATA_PATH}`.

## Implementation map

| Concern | Current implementation | Primary files |
| --- | --- | --- |
| Web UI | React 18, TypeScript, Vite, Framer Motion | `web/src/App.tsx`, `web/src/styles.css` |
| Client transport | Tauri IPC locally, same-origin `/api` when hosted | `web/src/api.ts` |
| Hosted service | Rust, Axum 0.7, Tokio | `server/src/main.rs` |
| Identity/data | Argon2 passwords, opaque hashed sessions, SQLite | `server/src/db.rs` |
| Provider core | Rust trait adapters and registry | `src/providers/` |
| Discovery | AniList catalog client and metadata cache | `src/catalog.rs`, `src/metadata/` |
| Playback | Server-mediated HLS/DASH/resource proxy with signed media sessions | `server/src/main.rs` |
| Container image | Multi-stage Node + Rust build, non-login runtime user | `Dockerfile`, `scripts/docker-entrypoint.sh` |
| Edge/TLS | Caddy automatic HTTPS and security headers | `deploy/homelab/Caddyfile` |
| Deployment | Docker Compose; current runbook documents manual deployment | `deploy/homelab/` |

## Hosted data

`web.db` contains users, password hashes, sessions, favorites, and history. Sessions last 30 days, are stored as token hashes, and are revoked when the protected administrator password changes. `catalog.db` is a core metadata/cache database. Both live under `ANI_DESK_DATA_DIR` and must be backed up together.

The current database connection enables foreign keys but does not explicitly configure WAL mode or a busy timeout. This is acceptable for low traffic but should be addressed before adding background writers or greater concurrency.

## Current security behavior

- The administrator identity is bootstrapped from `ANI_DESK_ADMIN_USERNAME` and `ANI_DESK_ADMIN_PASSWORD`.
- Passwords use Argon2.
- The session cookie is `HttpOnly`, `SameSite=Lax`, and `Secure` in the homelab Compose configuration.
- State-changing browser requests require `X-Ani-Desk-Request: 1`.
- Login attempts are limited in memory to eight attempts per IP-and-username key in 15 minutes.
- Admin routes require an authenticated `admin` role and protect the configured administrator from deletion or demotion.
- Browser media uses short-lived server-side sessions/signatures so upstream headers and raw stream details stay server-side.
- The server and Caddy set complementary security headers.

## Current operational assets

- `deploy/homelab/bootstrap-host.sh` installs Docker on Debian, configures unattended upgrades and UFW, and restricts SSH to `192.168.1.0/24`.
- `deploy/homelab/compose.yml` runs the service and Caddy.
- Optional Namecheap DDNS scripts and timer refresh the public address.

## Known context gaps

- The health endpoint only proves the process responds; it does not prove SQLite is writable, AniList is reachable, or any provider can play.
- Provider health is volatile and must not block sign-in or access to saved library data.
- The deployment scripts assume a specific Linux user, LAN subnet, domain, paths, repository, and `main` branch. Parameterize or consciously retain these values for the target homelab.
- The checked-in runbook uses stopped-data backups. Any future online SQLite
  backup must use SQLite's backup mechanism rather than copying live database
  files.
- In-memory login limits and media sessions disappear after a restart and do not work across multiple application replicas.
