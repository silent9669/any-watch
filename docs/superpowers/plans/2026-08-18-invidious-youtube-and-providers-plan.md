# Invidious YouTube Experience, Provider Dashboards, and K20 Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `any-watch` to include a full YouTube-style watch experience via Invidious (feeds, 16:9 search cards, theater player with related queue), per-provider search dashboards (empty search query shows provider's continuing watching and catalog), provider repairs (OPhim, AllAnime, AniZone), and K-20 Stremio Addon stream integration.

**Architecture:** 
- Backend (Rust/Axum): Extend Invidious provider with trending, popular, and related video endpoints (`/api/youtube/trending`, `/api/youtube/popular`, `/api/youtube/related/:id`). Implement K-20 (`sc.k-20.xyz`) Stremio Addon provider for direct HLS/HTTP streams. Fix OPhim stream domains and AllAnime/AniZone fallbacks.
- Frontend (React/TypeScript): Build YouTube feed with topic chips and YouTube continue watching, YouTube search list (16:9 landscape cards), and YouTube theater watch room with Related Videos sidebar. Enhance Search view to render provider-specific dashboards when query is empty.
- Player: Harden cross-browser HLS/DASH/MP4 streaming across Safari (native HLS), Chrome, and Firefox via opaque proxy.

**Tech Stack:** Rust 2021 (Axum 0.7, Tokio, Rusqlite, Reqwest), React 18, TypeScript, Vite, HLS.js, Dash.js, Playwright E2E.

---

### Task 1: Backend Invidious Endpoints & Fallback Hardening
**Files:**
- Modify: `src/providers/invidious.rs`
- Modify: `server/src/main.rs`
- Modify: `src/providers/mod.rs`
- Test: `src/providers/invidious.rs` unit tests, `cargo test`

- [ ] **Step 1: Add trending, popular, and related methods to `InvidiousProvider`**
- [ ] **Step 2: Add API routes in `server/src/main.rs` (`/api/youtube/trending`, `/api/youtube/popular`, `/api/youtube/related/:id`)**
- [ ] **Step 3: Test Invidious endpoints with `cargo test`**
- [ ] **Step 4: Commit changes**

---

### Task 2: Backend Provider Repairs (OPhim, AllAnime, AniZone)
**Files:**
- Modify: `src/providers/ophim.rs`
- Modify: `src/providers/allanime.rs`
- Modify: `src/providers/anizone.rs`
- Test: `examples/provider_certification.rs`

- [ ] **Step 1: Fix OPhim streaming domain resolution (replace dead `vip.opstream90.com` with active opstream/hls domains and fallbacks)**
- [ ] **Step 2: Harden AllAnime crypto bootstrap and fallback stream extraction**
- [ ] **Step 3: Run `cargo run --example provider_certification -- --all` to verify provider playback**
- [ ] **Step 4: Commit changes**

---

### Task 3: Backend K-20 Stremio Addon Integration
**Files:**
- Create: `src/providers/k20.rs`
- Modify: `src/providers/mod.rs`
- Modify: `src/config.rs`
- Modify: `config.toml.example`
- Test: `src/providers/k20.rs` unit tests, `cargo test`

- [ ] **Step 1: Implement `K20Provider` querying `https://sc.k-20.xyz/manifest.json`, catalogs, search, metadata, and direct HLS stream extraction**
- [ ] **Step 2: Register `K20Provider` in `ProviderRegistry` and config**
- [ ] **Step 3: Run `cargo test` and verify K-20 stream resolution**
- [ ] **Step 4: Commit changes**

---

### Task 4: Frontend YouTube Watching Experience & Watch Room
**Files:**
- Modify: `web/src/types.ts`
- Modify: `web/src/api.ts`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Add YouTube types and API client methods (`getYouTubeTrending`, `getYouTubePopular`, `getYouTubeRelated`)**
- [ ] **Step 2: Build `YouTubePage` with filter chips (`All`, `Trending`, `Music`, `Gaming`, `News`, `Animations`), YouTube continue watching shelf, and 16:9 video cards**
- [ ] **Step 3: Build YouTube search list view (horizontal 16:9 desktop layout / stacked mobile layout with channel, view count, published time)**
- [ ] **Step 4: Build YouTube watch room with theater player, video details, channel bar, and Related Videos / Up Next sidebar queue**
- [ ] **Step 5: Verify build with `npm run build`**
- [ ] **Step 6: Commit changes**

---

### Task 5: Frontend Provider Search Dashboards & Universal Overview
**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`

- [ ] **Step 1: Update Home Dashboard to feature universal Continue Watching shelf across anime + YouTube**
- [ ] **Step 2: Update Search View so selecting any provider when query is empty shows that provider's dedicated dashboard (continue watching from that provider + catalog categories)**
- [ ] **Step 3: Verify build with `npm run build`**
- [ ] **Step 4: Commit changes**

---

### Task 6: Cross-Browser Playback & Proxy Hardening
**Files:**
- Modify: `server/src/main.rs`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: Ensure opaque proxy handles CORS, Safari native HLS (`application/vnd.apple.mpegurl`), and range requests**
- [ ] **Step 2: Harden HLS/DASH player error recovery and quality switcher in `VideoPlayer`**
- [ ] **Step 3: Run server smoke tests `bash scripts/smoke-web-server.sh`**
- [ ] **Step 4: Commit changes**

---

### Task 7: Full Verification & E2E Testing
**Files:**
- Modify: `tests/e2e/test_app.py`
- Test: `pytest tests/e2e/test_app.py -q`
- Test: `cargo test --workspace`

- [ ] **Step 1: Update and run Pytest E2E browser tests covering YouTube hub, search dashboards, and player**
- [ ] **Step 2: Run all workspace tests and formatting checks (`cargo fmt --check`, `cargo clippy`, `cargo test`)**
- [ ] **Step 3: Commit changes**

---

### Task 8: Documentation Updates
**Files:**
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/PROVIDER_INVENTORY.md`
- Modify: `docs/DEPLOYMENT_HANDBOOK.md`
- Modify: `README.md`

- [ ] **Step 1: Document Invidious YouTube experience, endpoints, K-20 provider, and provider dashboard features**
- [ ] **Step 2: Commit documentation changes**
