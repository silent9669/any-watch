# any-watch

Private, account-only anime viewing for family members in desktop, mobile, and
TV browsers. The application is a React/Vite client backed by a Rust/Axum API
and SQLite persistence.

**Current service:** [ani.dangphuc.me](https://ani.dangphuc.me)

## Features

- Administrator-managed family accounts with no guest mode.
- Account-scoped watch history, progress, and My List across browsers.
- Title-first discovery with explicit provider availability and no silent source switching.
- Browser-native HLS, DASH, subtitle, quality, fullscreen, and picture-in-picture playback.
- AniSkip opening and ending markers with a persistent auto-skip preference.
- Browser downloads through short-lived authenticated tickets.
- Responsive layouts and keyboard/remote navigation for phones, computers, and TVs.

## Providers

AniZone, AniDB, KKPhim, OPhim, and Niniyo are the playback-tested defaults.
AniZone's English ASS subtitles are converted to browser-native WebVTT by the
opaque media proxy; AniDB contributes the Japanese-audio HLS source used by
ani-cli v5. MovieBox, AllAnime,
AnimeGG, AnimeVietSub, and HiAnime remain disabled unless they pass the current
admission and playback certification requirements. Provider cookies, required
headers, and upstream media URLs stay server-side behind opaque playback paths.

See [docs/PROVIDER_INVENTORY.md](docs/PROVIDER_INVENTORY.md) for the complete
status and admission policy.

## Development

```bash
npm install
npm run build
cargo test --workspace
```

Run the frontend and API separately during development:

```bash
npm run dev
ANY_WATCH_ADMIN_PASSWORD='replace-with-a-long-password' npm run serve:web
```

The `ANY_WATCH_*` environment variable names and existing SQLite paths are kept
for deployment and data compatibility.

## Deployment

The production image builds the web assets and the Axum service together:

```bash
docker build --tag any-watch .
```

Homelab and Cloudflare instructions remain under `deploy/`. Keep the existing
data mount and environment variables when upgrading so accounts, sessions,
history, and My List remain available.

Provider-health GETs are cached and concurrent refreshes are coalesced for five
minutes; checks have a 60-second backend budget. Cloudflare allows that endpoint
70 seconds without selecting whole-site maintenance. The independent GitHub
Pages maintenance shell is static; the Worker owns dynamic `/status.json` and
the globally stable first-observed outage timestamp.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Deployment handbook](docs/DEPLOYMENT_HANDBOOK.md)
- [Provider inventory](docs/PROVIDER_INVENTORY.md)
- [Design system](design.md)
