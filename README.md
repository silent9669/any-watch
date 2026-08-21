# any-watch

Private, account-only anime viewing for family members in desktop, mobile, and
TV browsers. The application is a React/Vite client backed by a Rust/Axum API
and SQLite persistence.

**Current service:** [ani.dangphuc.me](https://ani.dangphuc.me)

## Features

- Administrator-managed family accounts with no guest mode.
- Account-scoped watch history, progress, and My List across browsers.
- Title-first discovery with explicit provider availability and no silent source switching.
- Authentic YouTube viewing experience via Invidious: Trending & Popular feeds, topic chips (`All`, `Trending`, `Music`, `Gaming`, `News`, `Animations`), Theater Watch Room with channel bar and Up Next / Related video queue, and 16:9 continue watching cards.
- Provider Search Dashboards with provider-backed film, series, and anime suggestions, explicit status, quick search topics, and general-catalog fallback when a provider feed is unavailable.
- Universal Home Dashboard aggregating watch history across all anime providers and YouTube.
- Multi-page torrent search aggregation (Nyaa.si, AnimeTosho, YTS.mx, The Pirate Bay) with per-user queued `aria2c` tasks, safe archive inspection, main-video selection, bundled and embedded subtitle extraction, browser-compatible fast-start MP4 preparation, inline preview, and direct download.
- Browser-native HLS, DASH, subtitle, quality, fullscreen, and picture-in-picture playback across Chrome, Firefox, and Safari.
- AniSkip opening and ending markers with a persistent Skip intro preference.
- Browser downloads through short-lived authenticated tickets.
- Responsive layouts and keyboard/remote navigation for phones, computers, and TVs.

## Providers

AniDB, AnimeGG, KKPhim, OPhim, Niniyo, and K20 (Stremio Addon) are
enabled by default. Invidious is enabled only when `ANY_WATCH_INVIDIOUS_URL` is
configured. AniZone and MovieBox are disabled by default; MovieBox's current
HEVC-only DASH output does not decode reliably in the supported browser matrix.
When AniZone is explicitly enabled, its English ASS subtitles are converted to
browser-native WebVTT by the opaque media proxy. AniDB contributes the
Japanese-audio HLS source used by ani-cli v5. K20 connects to NguonC, STP, HH3D,
VSMOV, and YanHH3D catalogs through the Stremio Addon protocol and resolves
direct HTTP/HLS streams. AllAnime, AnimeVietSub, and HiAnime remain disabled
unless they pass the current admission and playback certification requirements.
Provider cookies, required headers, and upstream media URLs stay server-side
behind opaque playback paths.

Direct provider results retain a unique exact AniList title, native-title, or
synonym match so AniSkip uses the correct MyAnimeList title and
provider-certified episode number. Skip
intro automatically seeks only verified opening ranges; an episode without an
AniSkip submission remains playable and displays no marker.

See [docs/PROVIDER_INVENTORY.md](docs/PROVIDER_INVENTORY.md) for the complete
status and admission policy.

An optional Invidious connection provides an authentic YouTube viewing experience. Set
`ANY_WATCH_INVIDIOUS_URL` to a private or trusted Invidious instance; YouTube
search and feeds remain outside the English/Vietnamese anime source selector, while
playback, subtitles, progress, and My List use the existing authenticated
any-watch boundary. The integration is disabled when no instance is configured.

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
for deployment and data compatibility. Hosted mode uses certified provider
defaults unless `ANY_WATCH_PROVIDER_OVERRIDES` is set. To load a TOML file such
as `/data/config.toml`, set `ANY_WATCH_CONFIG_PATH` explicitly; hosted mode does
not read a workstation user config implicitly.

## Local Testing Guide

Run tests locally before pushing to ensure all checks pass:

### 1. Code Quality & Unit Tests

```bash
# Type check and build web frontend
npm run build

# Rust formatting and linting
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Rust backend tests
cargo test --workspace
```

### 2. Web Server Smoke Test

Validates backend authentication, session handling, My List, and API endpoints:

```bash
cargo build -p any-watch-server
bash scripts/smoke-web-server.sh
```

### 3. Browser End-to-End (E2E) Tests

Run responsive layout, navigation, YouTube feed/watch room, and playback tests in Chromium:

```bash
# Install Python dependencies and Playwright browser (one-time setup)
pip install -r tests/e2e/requirements.txt
playwright install chromium

# Run application E2E tests
pytest tests/e2e/test_app.py --browser chromium -q

# Run maintenance page tests
pytest tests/e2e/test_maintenance.py -q
```

### 4. Maintenance & Cloudflare Worker Validation

```bash
npm run maintenance:validate
npm run cloudflare:test
```

### 5. Production Docker Image & Container Smoke Test

Builds the multi-stage image and verifies containerized startup, healthcheck, and provider endpoints:

```bash
npm run providers:test:docker
```

## Deployment

The production image builds the web assets and the Axum service together:

```bash
docker build --tag any-watch .
```

Homelab and Cloudflare instructions remain under `deploy/`. Keep the existing
data mount and environment variables when upgrading so accounts, sessions,
history, and My List remain available.

Authenticated `GET` and `POST /api/providers/health` refreshes share a
five-minute aggregate cache and coalescing/version protection. At most sixteen
provider checks run concurrently, and each check has a 60-second backend timeout.
Cloudflare allows that endpoint 70 seconds without selecting whole-site
maintenance. The independent GitHub Pages maintenance shell is static; the
Worker owns dynamic `/status.json` and the globally stable first-observed outage
timestamp.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Deployment handbook](docs/DEPLOYMENT_HANDBOOK.md)
- [Provider inventory](docs/PROVIDER_INVENTORY.md)
- [Design system](design.md)
