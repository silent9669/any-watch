use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, types::Type, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct WatchHistory {
    pub anime_id: String,
    pub catalog_id: Option<i64>,
    pub provider: String,
    pub title: String,
    pub cover_url: String,
    pub episode_number: u32,
    pub episode_title: Option<String>,
    pub position_seconds: u64,
    pub total_seconds: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ImageCache {
    pub id: String,
    pub url: String,
    pub data: Vec<u8>,
    pub accessed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub checked_at: DateTime<Utc>,
    pub show_notification: bool,
}

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: String,
    pub provider: String,
    pub anime_id: String,
    pub anime_title: String,
    pub cover_url: String,
    pub episode_id: String,
    pub episode_number: u32,
    pub episode_title: Option<String>,
    pub file_path: String,
    pub file_name: String,
    pub bytes_downloaded: u64,
    pub media_kind: String,
    pub completed_at: DateTime<Utc>,
}

impl Database {
    pub async fn new() -> Result<Self> {
        let db_path = Self::default_db_path()?;
        Self::migrate_legacy_database(&db_path).await?;

        Self::new_at(db_path).await
    }

    pub async fn new_at(db_path: impl Into<PathBuf>) -> Result<Self> {
        let db_path = db_path.into();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.init_tables().await?;

        Ok(db)
    }

    async fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().await;

        // Watch history table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS watch_history (
                anime_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT NOT NULL,
                episode_number INTEGER NOT NULL,
                episode_title TEXT,
                position_seconds INTEGER NOT NULL,
                total_seconds INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;
        Self::ensure_column(&conn, "watch_history", "catalog_id", "INTEGER")?;

        // Image cache table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS image_cache (
                id TEXT PRIMARY KEY,
                url TEXT UNIQUE NOT NULL,
                data BLOB NOT NULL,
                accessed_at TEXT NOT NULL
            )",
            [],
        )?;
        // Metadata cache table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS metadata_cache (
                anilist_id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                rating INTEGER,
                cover_url TEXT,
                banner_url TEXT,
                genres TEXT,
                episode_count INTEGER,
                cached_at TEXT NOT NULL
            )",
            [],
        )?;
        Self::ensure_column(&conn, "metadata_cache", "banner_url", "TEXT")?;

        // Favorites table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS favorites (
                anime_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT NOT NULL,
                added_at TEXT NOT NULL
            )",
            [],
        )?;
        Self::ensure_column(&conn, "favorites", "catalog_id", "INTEGER")?;

        // Update info table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS update_info (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                latest_version TEXT NOT NULL,
                checked_at TEXT NOT NULL,
                show_notification INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Completed local downloads. Active progress remains owned by the
        // running downloader and is reconciled with this durable library when
        // the transfer finishes.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                anime_id TEXT NOT NULL,
                anime_title TEXT NOT NULL,
                cover_url TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                episode_number INTEGER NOT NULL,
                episode_title TEXT,
                file_path TEXT NOT NULL UNIQUE,
                file_name TEXT NOT NULL,
                bytes_downloaded INTEGER NOT NULL,
                media_kind TEXT NOT NULL,
                completed_at TEXT NOT NULL
            )",
            [],
        )?;

        // Indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_watch_history_updated
             ON watch_history(updated_at DESC)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_image_cache_accessed
             ON image_cache(accessed_at)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_downloads_completed
             ON downloads(completed_at DESC)",
            [],
        )?;

        Ok(())
    }

    pub async fn save_watch_history(&self, history: &WatchHistory) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO watch_history
             (anime_id, catalog_id, provider, title, cover_url, episode_number, episode_title,
              position_seconds, total_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                history.anime_id,
                history.catalog_id,
                history.provider,
                history.title,
                history.cover_url,
                history.episode_number,
                history.episode_title,
                history.position_seconds,
                history.total_seconds,
                history.updated_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub async fn get_watch_history(&self, anime_id: &str) -> Result<Option<WatchHistory>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT anime_id, catalog_id, provider, title, cover_url, episode_number, episode_title,
                    position_seconds, total_seconds, updated_at
             FROM watch_history WHERE anime_id = ?1",
        )?;

        let history = stmt
            .query_row([anime_id], |row| {
                Ok(WatchHistory {
                    anime_id: row.get(0)?,
                    catalog_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    cover_url: row.get(4)?,
                    episode_number: row.get(5)?,
                    episode_title: row.get(6)?,
                    position_seconds: row.get(7)?,
                    total_seconds: row.get(8)?,
                    updated_at: row
                        .get::<_, String>(9)?
                        .parse()
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .ok();

        Ok(history)
    }

    pub async fn get_continue_watching(&self, limit: usize) -> Result<Vec<WatchHistory>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT anime_id, catalog_id, provider, title, cover_url, episode_number, episode_title,
                    position_seconds, total_seconds, updated_at
             FROM watch_history
             ORDER BY updated_at DESC
             LIMIT ?1",
        )?;

        let histories: Vec<WatchHistory> = stmt
            .query_map([limit], |row| {
                Ok(WatchHistory {
                    anime_id: row.get(0)?,
                    catalog_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    cover_url: row.get(4)?,
                    episode_number: row.get(5)?,
                    episode_title: row.get(6)?,
                    position_seconds: row.get(7)?,
                    total_seconds: row.get(8)?,
                    updated_at: row
                        .get::<_, String>(9)?
                        .parse()
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<_, _>>()?;

        Ok(histories)
    }

    pub async fn remove_from_continue_watching(&self, anime_id: &str) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "DELETE FROM watch_history WHERE anime_id = ?1",
            params![anime_id],
        )?;

        Ok(())
    }

    pub async fn update_history_catalog_id(&self, anime_id: &str, catalog_id: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE watch_history SET catalog_id = ?1 WHERE anime_id = ?2 AND catalog_id IS NULL",
            params![catalog_id, anime_id],
        )?;
        Ok(())
    }

    pub async fn cache_image(&self, id: &str, url: &str, data: &[u8]) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO image_cache (id, url, data, accessed_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, url, data, Utc::now().to_rfc3339(),],
        )?;

        Ok(())
    }

    pub async fn get_cached_image(&self, id: &str) -> Result<Option<ImageCache>> {
        let conn = self.conn.lock().await;

        let mut stmt =
            conn.prepare("SELECT id, url, data, accessed_at FROM image_cache WHERE id = ?1")?;

        let cache = stmt
            .query_row([id], |row| {
                Ok(ImageCache {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    data: row.get(2)?,
                    accessed_at: row
                        .get::<_, String>(3)?
                        .parse()
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .ok();

        // Update access time
        if cache.is_some() {
            conn.execute(
                "UPDATE image_cache SET accessed_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), id],
            )?;
        }

        Ok(cache)
    }

    pub async fn cleanup_old_images(&self, max_size_mb: usize) -> Result<()> {
        let conn = self.conn.lock().await;

        // Calculate current cache size
        let size_mb: f64 = conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(data)), 0) / (1024.0 * 1024.0) FROM image_cache",
            [],
            |row| row.get(0),
        )?;

        if size_mb > max_size_mb as f64 {
            // Delete oldest entries until under limit
            let to_delete = ((size_mb - max_size_mb as f64) / 0.5) as i64;

            conn.execute(
                "DELETE FROM image_cache WHERE id IN (
                    SELECT id FROM image_cache ORDER BY accessed_at ASC LIMIT ?1
                )",
                [to_delete],
            )?;
        }

        Ok(())
    }

    pub async fn cache_metadata(&self, metadata: &crate::metadata::AniListMetadata) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO metadata_cache
             (anilist_id, title, description, rating, cover_url, banner_url, genres, episode_count, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                metadata.anilist_id,
                metadata.title,
                metadata.description.as_deref(),
                metadata.rating,
                metadata.cover_url.as_deref(),
                metadata.banner_url.as_deref(),
                serde_json::to_string(&metadata.genres)?,
                metadata.episode_count,
                metadata.cached_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub async fn get_cached_metadata(
        &self,
        anilist_id: i64,
    ) -> Result<Option<crate::metadata::AniListMetadata>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT anilist_id, title, description, rating, cover_url, banner_url, genres, episode_count, cached_at
             FROM metadata_cache WHERE anilist_id = ?1"
        )?;

        let metadata = stmt
            .query_row([anilist_id], |row| {
                let genres_str: String = row.get(6)?;
                let genres: Vec<String> = serde_json::from_str(&genres_str).unwrap_or_default();

                Ok(crate::metadata::AniListMetadata {
                    anilist_id: row.get(0)?,
                    title: row.get(1)?,
                    description: blank_to_none(row.get(2)?),
                    rating: row.get(3)?,
                    cover_url: blank_to_none(row.get(4)?),
                    banner_url: blank_to_none(row.get(5)?),
                    genres,
                    episode_count: row.get(7)?,
                    cached_at: row
                        .get::<_, String>(8)?
                        .parse()
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .ok();

        Ok(metadata)
    }

    pub async fn save_favorite(
        &self,
        anime_id: &str,
        catalog_id: Option<i64>,
        provider: &str,
        title: &str,
        cover_url: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO favorites
             (anime_id, catalog_id, provider, title, cover_url, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                anime_id,
                catalog_id,
                provider,
                title,
                cover_url,
                Utc::now().to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub async fn remove_favorite(&self, anime_id: &str) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "DELETE FROM favorites WHERE anime_id = ?1",
            params![anime_id],
        )?;

        Ok(())
    }

    pub async fn is_favorite(&self, anime_id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM favorites WHERE anime_id = ?1",
            params![anime_id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    pub async fn get_favorites(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, Option<i64>, String, String, String)>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT anime_id, catalog_id, provider, title, cover_url
             FROM favorites
             ORDER BY added_at DESC
             LIMIT ?1",
        )?;

        let favorites: Vec<(String, Option<i64>, String, String, String)> = stmt
            .query_map([limit], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<_, _>>()?;

        Ok(favorites)
    }

    pub async fn update_favorite_catalog_id(&self, anime_id: &str, catalog_id: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE favorites SET catalog_id = ?1 WHERE anime_id = ?2 AND catalog_id IS NULL",
            params![catalog_id, anime_id],
        )?;
        Ok(())
    }

    pub async fn save_update_info(&self, version: &str, show_notification: bool) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO update_info (id, latest_version, checked_at, show_notification)
             VALUES (1, ?1, ?2, ?3)",
            params![version, Utc::now().to_rfc3339(), show_notification as i32],
        )?;
        Ok(())
    }

    pub async fn get_update_info(&self) -> Result<Option<UpdateInfo>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT latest_version, checked_at, show_notification FROM update_info WHERE id = 1",
        )?;

        let info = stmt
            .query_row([], |row| {
                Ok(UpdateInfo {
                    latest_version: row.get(0)?,
                    checked_at: row
                        .get::<_, String>(1)?
                        .parse()
                        .unwrap_or_else(|_| Utc::now()),
                    show_notification: row.get::<_, i32>(2)? != 0,
                })
            })
            .ok();

        Ok(info)
    }

    pub async fn clear_update_notification(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE update_info SET show_notification = 0 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    pub async fn save_download(&self, download: &DownloadRecord) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO downloads
             (id, provider, anime_id, anime_title, cover_url, episode_id,
              episode_number, episode_title, file_path, file_name,
              bytes_downloaded, media_kind, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                download.id,
                download.provider,
                download.anime_id,
                download.anime_title,
                download.cover_url,
                download.episode_id,
                download.episode_number,
                download.episode_title,
                download.file_path,
                download.file_name,
                download.bytes_downloaded,
                download.media_kind,
                download.completed_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub async fn get_downloads(&self, limit: usize) -> Result<Vec<DownloadRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, provider, anime_id, anime_title, cover_url, episode_id,
                    episode_number, episode_title, file_path, file_name,
                    bytes_downloaded, media_kind, completed_at
             FROM downloads
             ORDER BY completed_at DESC
             LIMIT ?1",
        )?;
        let downloads = stmt
            .query_map([limit], Self::map_download_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(downloads)
    }

    pub async fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, provider, anime_id, anime_title, cover_url, episode_id,
                    episode_number, episode_title, file_path, file_name,
                    bytes_downloaded, media_kind, completed_at
             FROM downloads WHERE id = ?1",
        )?;
        Ok(stmt.query_row([id], Self::map_download_row).optional()?)
    }

    pub async fn remove_download(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn map_download_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadRecord> {
        let completed_at = row.get::<_, String>(12)?.parse().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(12, Type::Text, Box::new(error))
        })?;
        Ok(DownloadRecord {
            id: row.get(0)?,
            provider: row.get(1)?,
            anime_id: row.get(2)?,
            anime_title: row.get(3)?,
            cover_url: row.get(4)?,
            episode_id: row.get(5)?,
            episode_number: row.get(6)?,
            episode_title: row.get(7)?,
            file_path: row.get(8)?,
            file_name: row.get(9)?,
            bytes_downloaded: row.get(10)?,
            media_kind: row.get(11)?,
            completed_at,
        })
    }

    fn ensure_column(
        conn: &Connection,
        table: &str,
        column: &str,
        column_type: &str,
    ) -> Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;

        if !columns.iter().any(|existing| existing == column) {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}"),
                [],
            )?;
        }

        Ok(())
    }

    fn default_db_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "silent9669", "any-watch")
            .context("Failed to determine data directory")?;
        Ok(proj_dirs.data_dir().join("history.db"))
    }

    fn legacy_db_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "ani-tui", "ani-tui")
            .context("Failed to determine legacy data directory")?;
        Ok(proj_dirs.data_dir().join("history.db"))
    }

    async fn migrate_legacy_database(db_path: &std::path::Path) -> Result<()> {
        if db_path.exists() {
            return Ok(());
        }

        let legacy_path = Self::legacy_db_path()?;
        if !legacy_path.exists() {
            return Ok(());
        }

        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::copy(&legacy_path, db_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to migrate database from {} to {}",
                    legacy_path.display(),
                    db_path.display()
                )
            })?;

        Ok(())
    }
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_id_columns_migrate_without_replacing_existing_rows() {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute(
            "CREATE TABLE watch_history (anime_id TEXT PRIMARY KEY, title TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE favorites (anime_id TEXT PRIMARY KEY, title TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO watch_history (anime_id, title) VALUES ('legacy', 'Legacy Anime')",
            [],
        )
        .unwrap();

        Database::ensure_column(&conn, "watch_history", "catalog_id", "INTEGER").unwrap();
        Database::ensure_column(&conn, "favorites", "catalog_id", "INTEGER").unwrap();

        let row: (String, Option<i64>) = conn
            .query_row(
                "SELECT title, catalog_id FROM watch_history WHERE anime_id = 'legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("Legacy Anime".into(), None));
    }
}
