# Design Specification: Torrent Search, Video Extraction, and Download Subsystem

## 1. Overview & Objectives

This specification defines the architecture, data models, APIs, and UI components for the **Download Section** in Any-Watch. The feature enables users to:
1. Search across multiple public torrent indexers (Anime: Nyaa, AnimeTosho; Cinema Movies & Shows: YTS.mx and ThePirateBay).
2. Download torrent contents securely on the server using the sandboxed `aria2c` BitTorrent client bundled in the production image.
3. Automatically extract and remux the target media into standard web/device-compatible **MP4 format (`faststart`)** without unnecessary quality loss.
4. Auto-detect and extract embedded subtitles (English, Vietnamese), or fetch external **EngSub & VietSub** tracks (via SubDL / OpenSubtitles).
5. Provide a responsive, real-time "Download" UI in the web application for queue management, progress tracking, in-app preview, and 1-click browser download to user devices.
6. Enforce safe resource usage (sandboxed temporary storage, disk quotas, connection limits, and automatic cleanup).

---

## 2. Architecture & Subsystems

### 2.1 Torrent Search Indexers (`any-watch-core::torrents`)
A modular `TorrentSearchProvider` trait with asynchronous search implementations:
- **`NyaaProvider`**: Queries `nyaa.si` search feeds / RSS with categorized search for anime (English translated & raw/multi-subs).
- **`AnimeToshoProvider`**: Queries `animetosho.org` JSON API with rich metadata, quality tags, and direct subtitle/attachment links.
- **`YtsProvider`**: Queries `yts.mx` JSON API (`/api/v2/list_movies.json`) for cinema releases with 720p/1080p/4K qualities, seeds, and IMDb IDs.
- **`ThePirateBayProvider`**: Queries reliable TPB API proxies (e.g. `apibay.org`) with query sanitization and category filtering.
- **`TorrentSearchHub`**: Concurrent orchestrator with a 15-second timeout per provider, page-aware queries, deduplication, seed-based sorting, source aliases, and a bounded aggregate result set.

### 2.2 Subtitle Extraction & Lookup (`any-watch-core::subtitles`)
- **Embedded Subtitles**: Inspects container streams (MKV/MP4) using container parsers / `ffprobe` to extract `.srt` and `.vtt` tracks, tagging language codes (`en`, `vi`, `ja`).
- **External Subtitles**: Integrates SubDL / OpenSubtitles API searching by title, release group, or IMDb ID for English and Vietnamese subtitles.

### 2.3 BitTorrent Engine & Video Extraction (`server::torrent_engine`)
- **Engine**: The Rust service manages a cancellable `aria2c` process for each active task, supporting validated magnet URIs and HTTP(S) `.torrent` files.
- **Queueing**: A global three-task semaphore keeps accepted jobs queued without rejecting normal bursts.
- **Sandboxed Storage**: Task directories live in `/data/downloads_tmp/{task_id}/` by default and can be moved with `ANY_WATCH_TORRENT_DOWNLOAD_DIR`.
- **Storage Safety**:
  - Maximum concurrent active downloads (default: 3).
  - Enforced disk space quota (aborts if free space < 2GB).
  - Per-user ownership and persisted task metadata survive restarts; interrupted work is marked failed honestly.
  - Automatic cleanup purges completed or failed task directories after 24 hours.
- **Video Remuxing**:
  - Stream-copy container remux (`ffmpeg`/container remuxer) to generate `.mp4` with `+faststart` metadata for instant seeking and broad device compatibility.
  - Converts unsupported audio codecs (e.g. DTS/AC3) to standard stereo/5.1 AAC if required for web playback.

### 2.4 Server API Endpoints (`server/src/main.rs`)
- `GET /api/torrents/search`: Query multi-source indexers (`query`, `category`, `source`).
- `POST /api/torrents/download`: Initiate a download & extraction task (`magnet_url`, `title`, `sub_pref`).
- `GET /api/torrents/tasks`: List all active, ready, and failed tasks with progress, speed, and ETA.
- `GET /api/torrents/tasks`: Pollable task state for progress, speed, and ETA.
- `GET /api/torrents/tasks/:id/file`: Download the prepared `.mp4`.
- `GET /api/torrents/tasks/:id/stream`: Range-enabled inline MP4 playback.
- `GET /api/torrents/tasks/:id/subtitles/:lang`: Stream extracted or fetched `.vtt` / `.srt` subtitle.
- `DELETE /api/torrents/tasks/:id`: Cancel task or remove completed file and clean up disk.

### 2.5 Frontend UI (`web/src/App.tsx`)
- **Navigation**: "Download" tab added to primary navigation bar.
- **Search View**:
  - Category selector (All, Cinema / Movies, TV Series, Anime).
  - Source selector (All, Nyaa, YTS, ThePirateBay, AnimeTosho).
  - Result cards with Title, Seed/Leech badges, Quality badge (1080p, 4K, 720p), Size, and Subtitle indicators (`[EN]`, `[VI]`).
  - Action buttons: "Extract & Download", "Copy Magnet".
- **Download Manager Panel**:
  - Task cards showing status (`Queued`, `Downloading [xx%]`, `Remuxing to MP4`, `Ready`, `Error`).
  - Speed, downloaded bytes, ETA, and progress bar.
  - Action buttons: "Download to PC", "Download Subtitles", "Preview / Play", "Delete / Cleanup".

---

## 3. Data Models

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSearchResult {
    pub id: String,
    pub title: String,
    pub magnet_url: String,
    pub torrent_url: Option<String>,
    pub source: String,
    pub category: String,
    pub size_bytes: u64,
    pub seeds: u32,
    pub peers: u32,
    pub quality: Option<String>,
    pub upload_date: Option<String>,
    pub has_engsub: bool,
    pub has_vietsub: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TorrentTaskStatus {
    Queued,
    Downloading { progress: f32, speed_bytes_per_sec: u64, eta_seconds: u64 },
    Remuxing { progress: f32 },
    Ready { file_name: String, file_size: u64, has_mp4: bool, subtitles: Vec<String> },
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentTask {
    pub id: String,
    pub title: String,
    pub magnet_url: String,
    pub status: TorrentTaskStatus,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}
```

---

## 4. Verification & Testing Plan

1. **Unit Tests**:
   - Parse search responses from Nyaa, AnimeTosho, YTS, TPB.
   - Validate magnet link generation and sanitization.
   - Verify task state transitions and cleanup rules.
   - Subtitle parser & mapper tests.
2. **Integration & API Tests**:
   - Mocked indexer query tests.
   - Test endpoints `/api/torrents/search`, `/api/torrents/download`, `/api/torrents/tasks`.
3. **Frontend Tests & Build**:
   - `npm run check:web-server` / TypeScript typecheck.
   - `npm run build` production asset compilation.
4. **CI & Docker Verification**:
   - `cargo check --workspace`
   - `cargo test --workspace`
   - Docker build validation.
