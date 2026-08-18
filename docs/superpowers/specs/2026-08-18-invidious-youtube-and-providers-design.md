# Invidious YouTube Experience, Provider Dashboards, and K20 Stremio Integration Design

## Overview
This specification details the overhaul of the `any-watch` media viewing application to deliver:
1. An authentic **YouTube watching experience** powered by Invidious (Trending, Popular, topic chips, YouTube-specific continue watching, vertical search results list with 16:9 landscape cards, and a dedicated watch room featuring a theater player with a "Related Videos / Up Next" queue).
2. **Provider-specific Dashboards** in the search view: when no query is typed, selecting any provider renders its exclusive continuing watching history and catalog categories.
3. **Provider Repairs**: Fix OPhim (update outdated stream domains), AllAnime (bootstrap crypto/decryption fallbacks), and AniZone.
4. **New Provider Integration**: Add K-20 / Stremio Addon streaming engine (`sc.k-20.xyz`) for direct HLS/HTTP movie and series streams with OpenSubtitles support.
5. **Cross-Browser Playback**: Ensure seamless video streaming across Chrome, Google/Android, Safari (macOS/iOS with native HLS), and Firefox via an opaque media proxy handling CORS and Byte-Range requests.

---

## 1. User Interface & Layout Architecture

### 1.1 Navigation & Global Home Feed
- **Top / Sidebar Navigation**: `Home`, `Anime Catalog`, `Search & Providers`, `YouTube`, `My List`, `History`, `Settings`.
- **Global Home Feed**:
  - **Universal Continue Watching Shelf**: Displays all active media across both Anime providers and YouTube with progress indicators, episode/duration tags, and provider badges (`Invidious`, `K20`, `KKPhim`, `Niniyo`, etc.).
  - **Featured Banner & Recommendations**: Curated mix of trending anime and top YouTube highlight picks.

### 1.2 YouTube Experience (`/youtube`)
- **Dashboard View (Non-Search)**:
  - **Filter Chips**: `All`, `Trending`, `Music`, `Gaming`, `News`, `Animations`.
  - **YouTube Continue Watching**: Account-specific history filtered to Invidious items with thumbnail, channel, and exact playback progress.
  - **Feed Grid**: Responsive 16:9 video cards displaying thumbnail, duration pill (`mm:ss`), video title, channel name, verified badge/avatar, view count, and time elapsed.
- **Search View**:
  - Vertical list of 16:9 horizontal video items with rich metadata (channel info, view count, published time, description snippet).
- **YouTube Watch Room (`/youtube?v={id}` or watch room)**:
  - **Main Player Stage (Left / Top)**:
    - 16:9 video player with ambient glow, quality selection, WebVTT subtitle switcher, speed control, and theater toggle.
    - Large video title, channel badge with avatar, Subscribe/Bookmark button.
    - Expandable description box (view counts, upload date, formatted description text).
  - **Related Videos Sidebar (Right on Desktop / Bottom on Mobile/TV)**:
    - "Up Next" queue of related Invidious videos. Clicking plays the video seamlessly and refreshes the related queue.

### 1.3 Provider Search Dashboards (`/search`)
- **Empty Query Mode (Provider Dashboard)**:
  - Selecting any provider pill (e.g., `K20`, `KKPhim`, `Niniyo`, `AniDB`, `AnimeGG`, `Invidious`) displays:
    - **Provider-specific Continue Watching**: Filtered to media from that exact source.
    - **Provider Catalog Sections**: Popular, newly updated, and categorized titles from that provider.
- **Active Query Mode**:
  - Real-time search result cards formatted for the respective media type (2:3 portrait anime cards or 16:9 video cards).

---

## 2. Backend & Core Provider Architecture

### 2.1 Invidious Enhancements (`src/providers/invidious.rs`, `server/src/main.rs`)
- Add endpoints:
  - `GET /api/youtube/trending`: Fetches trending videos from the configured Invidious instance.
  - `GET /api/youtube/popular`: Fetches popular videos.
  - `GET /api/youtube/related/:id`: Fetches related video recommendations for the watch room sidebar.
- Robust stream extraction supporting DASH manifests, HLS master playlists, and direct MP4 format streams with fallback handling.

### 2.2 K-20 Stremio Addon Provider (`src/providers/k20.rs`)
- Implements `AnimeProvider` trait for `sc.k-20.xyz` Stremio Addon protocol:
  - **Catalog**: Queries `/catalog/series/nguonc-series/` and `/catalog/movie/nguonc-movie/`.
  - **Search**: Queries `/catalog/{type}/{id}/search={query}.json`.
  - **Details & Episodes**: Resolves metadata from `/meta/{type}/{id}.json`.
  - **Stream**: Fetches stream links from `/stream/{type}/{id}.json` and unpacks direct HLS/MP4 streams.
  - **Subtitles**: OpenSubtitles WebVTT integration.

### 2.3 Provider Fixes
- **OPhim (`src/providers/ophim.rs`)**: Update API endpoint and media URL rewriting to use active OPhim streaming CDNs (e.g. `opstream1`, `opstream90` fallbacks, `s1.phim128.tv`).
- **AllAnime (`src/providers/allanime.rs`)**: Harden crypto table extraction and decrypt routines.
- **AniZone (`src/providers/anizone.rs`)**: Update endpoint fallbacks.

### 2.4 Cross-Browser Media Streaming & Opaque Proxy
- **Safari Compatibility**: Full support for native HLS (`application/vnd.apple.mpegurl`) with proper Content-Type headers and Byte-Range handling.
- **Chrome / Firefox / Android**: Hls.js and Dash.js playback backed by the Axum opaque proxy (`/proxy/media`) which handles CORS, upstream referrer spoofing, and range chunks.

---

## 3. Database & Storage
- SQLite schema in `data/any-watch.db` remains backwards-compatible.
- `watch_history` and `favorites` store `provider` (`Invidious`, `K20`, `AniDB`, etc.) and `anime_id`, allowing seamless filtering for per-provider continue-watching shelves.

---

## 4. Verification & Testing Strategy
1. **Rust Workspace Tests**: `cargo test --workspace`
2. **Live Provider Certification**: `cargo run --example provider_certification -- --all`
3. **Server Smoke Test**: `bash scripts/smoke-web-server.sh`
4. **E2E & Responsive Tests**: `pytest tests/e2e/test_app.py -q`
5. **Documentation Updates**: Synchronize `docs/ARCHITECTURE.md`, `docs/PROVIDER_INVENTORY.md`, and `README.md`.
