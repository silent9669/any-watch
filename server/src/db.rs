use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, types::Type, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct WebDatabase {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    pub id: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub enabled: bool,
    pub protected: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteRecord {
    pub anime_id: String,
    pub catalog_id: Option<i64>,
    pub provider: String,
    pub title: String,
    pub cover_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub anime_id: String,
    pub catalog_id: Option<i64>,
    pub provider: String,
    pub title: String,
    pub cover_url: String,
    pub episode_number: u32,
    pub episode_title: Option<String>,
    pub position_seconds: u64,
    pub total_seconds: u64,
    pub updated_at: String,
}

pub struct NewFavorite<'a> {
    pub anime_id: &'a str,
    pub catalog_id: Option<i64>,
    pub provider: &'a str,
    pub title: &'a str,
    pub cover_url: &'a str,
}

pub struct NewHistory<'a> {
    pub anime_id: &'a str,
    pub catalog_id: Option<i64>,
    pub provider: &'a str,
    pub title: &'a str,
    pub cover_url: &'a str,
    pub episode_number: u32,
    pub episode_title: Option<&'a str>,
    pub position_seconds: u64,
    pub total_seconds: u64,
}

impl WebDatabase {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open web database at {}", path.display()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE COLLATE NOCASE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('admin', 'user', 'guest')),
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                token_hash TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                expires_at TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
            CREATE TABLE IF NOT EXISTS user_favorites (
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                anime_id TEXT NOT NULL,
                catalog_id INTEGER,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT NOT NULL,
                added_at TEXT NOT NULL,
                PRIMARY KEY(user_id, anime_id)
            );
            CREATE TABLE IF NOT EXISTS user_history (
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                anime_id TEXT NOT NULL,
                catalog_id INTEGER,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT NOT NULL,
                episode_number INTEGER NOT NULL,
                episode_title TEXT,
                position_seconds INTEGER NOT NULL,
                total_seconds INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(user_id, anime_id)
            );
            CREATE INDEX IF NOT EXISTS idx_user_history_updated
                ON user_history(user_id, updated_at DESC);",
        )?;
        let users_sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'users'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(sql) = users_sql {
            if !sql.contains("'guest'") {
                conn.execute_batch(
                    "PRAGMA foreign_keys = OFF;
                     CREATE TABLE users_migrated (
                         id TEXT PRIMARY KEY,
                         username TEXT NOT NULL UNIQUE COLLATE NOCASE,
                         password_hash TEXT NOT NULL,
                         role TEXT NOT NULL CHECK(role IN ('admin', 'user', 'guest')),
                         enabled INTEGER NOT NULL DEFAULT 1,
                         created_at TEXT NOT NULL,
                         protected INTEGER NOT NULL DEFAULT 0
                     );
                     INSERT INTO users_migrated (id, username, password_hash, role, enabled, created_at, protected)
                     SELECT id, username, password_hash, role, enabled, created_at, COALESCE(protected, 0) FROM users;
                     DROP TABLE users;
                     ALTER TABLE users_migrated RENAME TO users;
                     PRAGMA foreign_keys = ON;",
                )?;
            }
        }
        let has_protected = conn
            .prepare("PRAGMA table_info(users)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|column| column == "protected");
        if !has_protected {
            conn.execute(
                "ALTER TABLE users ADD COLUMN protected INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    pub async fn bootstrap_admin(&self, username: &str, password: &str) -> Result<()> {
        validate_username(username)?;
        validate_password(password)?;
        let username = username.trim();
        let (protected_accounts, target_account) = {
            let conn = self.conn.lock().await;
            let mut protected_statement = conn.prepare(
                "SELECT id, username, password_hash FROM users WHERE protected = 1 ORDER BY created_at",
            )?;
            let protected_accounts = protected_statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let target_account = conn
                .query_row(
                    "SELECT id, password_hash FROM users WHERE username = ?1 COLLATE NOCASE",
                    [username],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            (protected_accounts, target_account)
        };

        anyhow::ensure!(
            protected_accounts.len() <= 1,
            "multiple protected administrator accounts exist; resolve them before changing the configured administrator"
        );

        if let Some((protected_id, protected_username, protected_hash)) =
            protected_accounts.into_iter().next()
        {
            if let Some((target_id, _)) = target_account.as_ref() {
                anyhow::ensure!(
                    target_id == &protected_id,
                    "the configured administrator username is already used by another account"
                );
            }

            let username_changed = !protected_username.eq_ignore_ascii_case(username);
            let password_changed = !verify_password_async(password, &protected_hash).await?;
            if !username_changed && !password_changed {
                let changed = self.conn.lock().await.execute(
                    "UPDATE users SET role = 'admin', enabled = 1, protected = 1 WHERE id = ?1",
                    [&protected_id],
                )?;
                anyhow::ensure!(changed == 1, "protected administrator was not found");
                return Ok(());
            }

            let password_hash = hash_password_async(password).await?;
            let mut conn = self.conn.lock().await;
            let transaction = conn.transaction()?;
            let changed = transaction.execute(
                "UPDATE users
                 SET username = ?1, password_hash = ?2, role = 'admin', enabled = 1, protected = 1
                 WHERE id = ?3",
                params![username, password_hash, protected_id],
            )?;
            anyhow::ensure!(changed == 1, "protected administrator was not found");
            transaction.execute("DELETE FROM sessions WHERE user_id = ?1", [&protected_id])?;
            transaction.commit()?;
            return Ok(());
        }

        if let Some((target_id, target_hash)) = target_account {
            let password_changed = !verify_password_async(password, &target_hash).await?;
            let password_hash = if password_changed {
                Some(hash_password_async(password).await?)
            } else {
                None
            };
            let mut conn = self.conn.lock().await;
            let transaction = conn.transaction()?;
            if let Some(password_hash) = password_hash {
                let changed = transaction.execute(
                    "UPDATE users
                     SET password_hash = ?1, role = 'admin', enabled = 1, protected = 1
                     WHERE id = ?2",
                    params![password_hash, target_id],
                )?;
                anyhow::ensure!(changed == 1, "configured administrator was not found");
                transaction.execute("DELETE FROM sessions WHERE user_id = ?1", [&target_id])?;
            } else {
                let changed = transaction.execute(
                    "UPDATE users SET role = 'admin', enabled = 1, protected = 1 WHERE id = ?1",
                    [&target_id],
                )?;
                anyhow::ensure!(changed == 1, "configured administrator was not found");
            }
            transaction.commit()?;
            return Ok(());
        }

        let hash = hash_password_async(password).await?;
        self.conn.lock().await.execute(
            "INSERT INTO users
             (id, username, password_hash, role, enabled, protected, created_at)
             VALUES (?1, ?2, ?3, 'admin', 1, 1, ?4)",
            params![
                Uuid::new_v4().to_string(),
                username,
                hash,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<SessionUser>> {
        let row = {
            let conn = self.conn.lock().await;
            conn.query_row(
                "SELECT id, username, role, password_hash, enabled
                 FROM users WHERE username = ?1 COLLATE NOCASE",
                [username.trim()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()?
        };
        let Some((id, username, role, hash, enabled)) = row else {
            // Perform comparable KDF work so unknown usernames are not a cheap oracle.
            let _ = hash_password_async(password).await;
            return Ok(None);
        };
        if !enabled || !verify_password_async(password, &hash).await? {
            return Ok(None);
        }
        Ok(Some(SessionUser { id, username, role }))
    }

    pub async fn create_session(&self, user_id: &str) -> Result<String> {
        let mut raw = [0_u8; 32];
        OsRng.fill_bytes(&mut raw);
        let token = URL_SAFE_NO_PAD.encode(raw);
        let now = Utc::now();
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM sessions WHERE expires_at <= ?1",
            [now.to_rfc3339()],
        )?;
        conn.execute(
            "INSERT INTO sessions (token_hash, user_id, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                token_hash(&token),
                user_id,
                (now + Duration::days(30)).to_rfc3339(),
                now.to_rfc3339()
            ],
        )?;
        Ok(token)
    }

    pub async fn session_user(&self, token: &str) -> Result<Option<SessionUser>> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();
        let user = conn
            .query_row(
                "SELECT u.id, u.username, u.role
                 FROM sessions s JOIN users u ON u.id = s.user_id
                 WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.enabled = 1",
                params![token_hash(token), now],
                |row| {
                    Ok(SessionUser {
                        id: row.get(0)?,
                        username: row.get(1)?,
                        role: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(user)
    }

    pub async fn revoke_session(&self, token: &str) -> Result<()> {
        self.conn.lock().await.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            [token_hash(token)],
        )?;
        Ok(())
    }

    pub async fn list_users(&self) -> Result<Vec<ManagedUser>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, role, enabled, protected, created_at
             FROM users ORDER BY protected DESC, username COLLATE NOCASE",
        )?;
        let users = stmt
            .query_map([], |row| {
                Ok(ManagedUser {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    role: row.get(2)?,
                    enabled: row.get(3)?,
                    protected: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(users)
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: &str,
    ) -> Result<ManagedUser> {
        validate_username(username)?;
        validate_password(password)?;
        anyhow::ensure!(
            matches!(role, "admin" | "user" | "guest"),
            "role must be admin, user, or guest"
        );
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let password_hash = hash_password_async(password).await?;
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![id, username.trim(), password_hash, role, created_at],
        )?;
        Ok(ManagedUser {
            id,
            username: username.trim().into(),
            role: role.into(),
            enabled: true,
            protected: false,
            created_at,
        })
    }

    pub async fn is_protected_user(&self, id: &str) -> Result<bool> {
        Ok(self.conn.lock().await.query_row(
            "SELECT protected FROM users WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?)
    }

    pub async fn update_user(
        &self,
        id: &str,
        username: &str,
        enabled: bool,
        role: &str,
        password: Option<&str>,
    ) -> Result<()> {
        validate_username(username)?;
        anyhow::ensure!(
            matches!(role, "admin" | "user" | "guest"),
            "role must be admin, user, or guest"
        );
        let password_hash = if let Some(password) = password.filter(|value| !value.is_empty()) {
            validate_password(password)?;
            Some(hash_password_async(password).await?)
        } else {
            None
        };
        let conn = self.conn.lock().await;
        let changed = if let Some(password_hash) = password_hash {
            conn.execute(
                "UPDATE users SET username = ?1, enabled = ?2, role = ?3, password_hash = ?4
                 WHERE id = ?5 AND protected = 0",
                params![username.trim(), enabled, role, password_hash, id],
            )?
        } else {
            conn.execute(
                "UPDATE users SET username = ?1, enabled = ?2, role = ?3
                 WHERE id = ?4 AND protected = 0",
                params![username.trim(), enabled, role, id],
            )?
        };
        anyhow::ensure!(changed == 1, "user was not found or is protected");
        if !enabled {
            conn.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        }
        Ok(())
    }

    pub async fn delete_user(&self, id: &str) -> Result<()> {
        let changed = self
            .conn
            .lock()
            .await
            .execute("DELETE FROM users WHERE id = ?1 AND protected = 0", [id])?;
        anyhow::ensure!(changed == 1, "user was not found or is protected");
        Ok(())
    }

    pub async fn favorites(&self, user_id: &str, limit: usize) -> Result<Vec<FavoriteRecord>> {
        let limit = i64::try_from(limit).context("favorite limit is too large")?;
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT anime_id, catalog_id, provider, title, cover_url FROM user_favorites
             WHERE user_id = ?1 ORDER BY added_at DESC LIMIT ?2",
        )?;
        let favorites = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(FavoriteRecord {
                    anime_id: row.get(0)?,
                    catalog_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    cover_url: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(favorites)
    }

    pub async fn save_favorite(&self, user_id: &str, value: &NewFavorite<'_>) -> Result<()> {
        self.conn.lock().await.execute(
            "INSERT OR REPLACE INTO user_favorites
             (user_id, anime_id, catalog_id, provider, title, cover_url, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                user_id,
                value.anime_id,
                value.catalog_id,
                value.provider,
                value.title,
                value.cover_url,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub async fn remove_favorite(&self, user_id: &str, anime_id: &str) -> Result<()> {
        self.conn.lock().await.execute(
            "DELETE FROM user_favorites WHERE user_id = ?1 AND anime_id = ?2",
            params![user_id, anime_id],
        )?;
        Ok(())
    }

    pub async fn history(&self, user_id: &str, limit: usize) -> Result<Vec<HistoryRecord>> {
        let limit = i64::try_from(limit).context("history limit is too large")?;
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT anime_id, catalog_id, provider, title, cover_url, episode_number,
                    episode_title, position_seconds, total_seconds, updated_at
             FROM user_history WHERE user_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let history = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(HistoryRecord {
                    anime_id: row.get(0)?,
                    catalog_id: row.get(1)?,
                    provider: row.get(2)?,
                    title: row.get(3)?,
                    cover_url: row.get(4)?,
                    episode_number: u32::try_from(row.get::<_, i64>(5)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(5, Type::Integer, Box::new(error))
                    })?,
                    episode_title: row.get(6)?,
                    position_seconds: u64::try_from(row.get::<_, i64>(7)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(7, Type::Integer, Box::new(error))
                    })?,
                    total_seconds: u64::try_from(row.get::<_, i64>(8)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(8, Type::Integer, Box::new(error))
                    })?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(history)
    }

    pub async fn save_history(&self, user_id: &str, value: &NewHistory<'_>) -> Result<()> {
        let episode_number = i64::from(value.episode_number);
        let position_seconds =
            i64::try_from(value.position_seconds).context("history position is too large")?;
        let total_seconds =
            i64::try_from(value.total_seconds).context("history duration is too large")?;
        self.conn.lock().await.execute(
            "INSERT OR REPLACE INTO user_history
             (user_id, anime_id, catalog_id, provider, title, cover_url, episode_number,
              episode_title, position_seconds, total_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                user_id,
                value.anime_id,
                value.catalog_id,
                value.provider,
                value.title,
                value.cover_url,
                episode_number,
                value.episode_title,
                position_seconds,
                total_seconds,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub async fn remove_history(&self, user_id: &str, anime_id: &str) -> Result<()> {
        self.conn.lock().await.execute(
            "DELETE FROM user_history WHERE user_id = ?1 AND anime_id = ?2",
            params![user_id, anime_id],
        )?;
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String> {
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string())
}

async fn hash_password_async(password: &str) -> Result<String> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .context("password hashing task stopped")?
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

async fn verify_password_async(password: &str, hash: &str) -> Result<bool> {
    let password = password.to_owned();
    let hash = hash.to_owned();
    tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .context("password verification task stopped")
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn validate_username(username: &str) -> Result<()> {
    let value = username.trim();
    anyhow::ensure!(
        (3..=40).contains(&value.len()),
        "username must contain 3 to 40 characters"
    );
    anyhow::ensure!(
        value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "username contains unsupported characters"
    );
    Ok(())
}

fn validate_password(password: &str) -> Result<()> {
    anyhow::ensure!(
        (10..=256).contains(&password.len()),
        "password must contain at least 10 characters"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn protected_admin_migrates_in_place_and_regular_accounts_are_manageable() {
        let path = std::env::temp_dir().join(format!("any-watch-web-{}.db", Uuid::new_v4()));
        let db = WebDatabase::open(&path).await.unwrap();
        db.bootstrap_admin("root", "Root-Password-2026")
            .await
            .unwrap();

        let viewer = db
            .create_user("viewer", "Viewer-Password-2026", "user")
            .await
            .unwrap();
        let users = db.list_users().await.unwrap();
        let root = users.iter().find(|user| user.username == "root").unwrap();
        assert!(root.protected);
        assert!(!viewer.protected);

        let root_update = db
            .update_user(
                &root.id,
                "changed-root",
                false,
                "user",
                Some("Changed-Root-Password"),
            )
            .await;
        assert!(root_update.is_err());

        db.update_user(
            &viewer.id,
            "viewer-renamed",
            true,
            "admin",
            Some("Viewer-Updated-Password"),
        )
        .await
        .unwrap();
        let authenticated = db
            .authenticate("viewer-renamed", "Viewer-Updated-Password")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(authenticated.role, "admin");
        assert!(db
            .authenticate("viewer", "Viewer-Password-2026")
            .await
            .unwrap()
            .is_none());

        db.delete_user(&viewer.id).await.unwrap();
        assert!(!db
            .list_users()
            .await
            .unwrap()
            .iter()
            .any(|user| user.id == viewer.id));
        assert!(db.delete_user(&root.id).await.is_err());

        db.save_favorite(
            &root.id,
            &NewFavorite {
                anime_id: "animegg:one-piece",
                catalog_id: Some(21),
                provider: "AnimeGG",
                title: "One Piece",
                cover_url: "https://example.invalid/one-piece.jpg",
            },
        )
        .await
        .unwrap();
        db.save_history(
            &root.id,
            &NewHistory {
                anime_id: "animegg:one-piece",
                catalog_id: Some(21),
                provider: "AnimeGG",
                title: "One Piece",
                cover_url: "https://example.invalid/one-piece.jpg",
                episode_number: 1,
                episode_title: Some("I'm Luffy!"),
                position_seconds: 42,
                total_seconds: 1_500,
            },
        )
        .await
        .unwrap();

        let root_session = db.create_session(&root.id).await.unwrap();
        db.bootstrap_admin("protected-admin", "Replacement-Password-2026")
            .await
            .unwrap();
        let users = db.list_users().await.unwrap();
        assert!(!users.iter().any(|user| user.username == "root"));
        let replacement = users
            .iter()
            .find(|user| user.username == "protected-admin")
            .unwrap();
        assert_eq!(replacement.id, root.id);
        assert!(replacement.protected);
        assert!(db
            .authenticate("root", "Root-Password-2026")
            .await
            .unwrap()
            .is_none());
        assert!(db
            .authenticate("protected-admin", "Replacement-Password-2026")
            .await
            .unwrap()
            .is_some());
        let favorites = db.favorites(&replacement.id, 10).await.unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].anime_id, "animegg:one-piece");
        let history = db.history(&replacement.id, 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].episode_number, 1);
        assert_eq!(history[0].position_seconds, 42);
        assert!(db.session_user(&root_session).await.unwrap().is_none());

        let replacement_session = db.create_session(&replacement.id).await.unwrap();
        db.bootstrap_admin("protected-admin", "Replacement-Password-2026")
            .await
            .unwrap();
        assert!(db
            .session_user(&replacement_session)
            .await
            .unwrap()
            .is_some());

        let guest = db
            .create_user("guest-account", "Guest-Password-2026", "guest")
            .await
            .unwrap();
        assert_eq!(guest.role, "guest");
        let guest_auth = db
            .authenticate("guest-account", "Guest-Password-2026")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(guest_auth.role, "guest");

        drop(db);
        let _ = tokio::fs::remove_file(path).await;
    }
}
