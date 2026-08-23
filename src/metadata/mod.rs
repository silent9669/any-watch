use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const ANILIST_API: &str = "https://graphql.anilist.co";
const CACHE_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AniListMetadata {
    pub anilist_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub rating: Option<i64>,
    pub cover_url: Option<String>,
    pub banner_url: Option<String>,
    pub genres: Vec<String>,
    pub episode_count: Option<i64>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<u32>,
    pub description: Option<String>,
    pub rating: Option<f32>,
    pub cover_url: Option<String>,
    pub banner_url: Option<String>,
    pub genres: Vec<String>,
    pub media_type: String,
}

#[derive(Debug, Clone)]
pub struct EnrichedAnime {
    pub base: crate::providers::Anime,
    pub metadata: Option<AniListMetadata>,
}

#[derive(Clone)]
pub struct AniListClient {
    client: reqwest::Client,
}

impl AniListClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub async fn search_anime(&self, query: &str) -> Result<Vec<AniListMetadata>> {
        let search_query = r#"
            query ($search: String) {
                Page(page: 1, perPage: 10) {
                    media(search: $search, type: ANIME) {
                        id
                        title {
                            romaji
                            english
                            native
                        }
                        description
                        averageScore
                        coverImage {
                            large
                            medium
                        }
                        bannerImage
                        genres
                        episodes
                    }
                }
            }
        "#;

        let variables = serde_json::json!({
            "search": query
        });

        let response = self
            .client
            .post(ANILIST_API)
            .json(&serde_json::json!({
                "query": search_query,
                "variables": variables
            }))
            .send()
            .await
            .context("Failed to query AniList")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("AniList API error: {} - {}", status, text);
        }

        let json: serde_json::Value = response.json().await?;
        let mut results = Vec::new();

        if let Some(media_list) = json["data"]["Page"]["media"].as_array() {
            for media in media_list {
                let anilist_id = media["id"].as_i64().unwrap_or_default();

                let title = media["title"]["english"]
                    .as_str()
                    .or_else(|| media["title"]["romaji"].as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let description = media["description"]
                    .as_str()
                    .map(|s| s.replace("<br>", "\n").replace("<br/>", "\n"));

                let rating = media["averageScore"].as_i64();

                let cover_url = media["coverImage"]["large"]
                    .as_str()
                    .or_else(|| media["coverImage"]["medium"].as_str())
                    .map(|s| s.to_string());

                let banner_url = media["bannerImage"].as_str().map(|s| s.to_string());

                let genres: Vec<String> = media["genres"]
                    .as_array()
                    .map(|g| {
                        g.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let episode_count = media["episodes"].as_i64();

                results.push(AniListMetadata {
                    anilist_id,
                    title,
                    description,
                    rating,
                    cover_url,
                    banner_url,
                    genres,
                    episode_count,
                    cached_at: Utc::now(),
                });
            }
        }

        Ok(results)
    }

    pub async fn get_by_id(&self, anilist_id: i64) -> Result<Option<AniListMetadata>> {
        let query = r#"
            query ($id: Int) {
                Media(id: $id, type: ANIME) {
                    id
                    title {
                        romaji
                        english
                        native
                    }
                    description
                    averageScore
                    coverImage {
                        large
                        medium
                    }
                    bannerImage
                    genres
                    episodes
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": anilist_id
        });

        let response = self
            .client
            .post(ANILIST_API)
            .json(&serde_json::json!({
                "query": query,
                "variables": variables
            }))
            .send()
            .await
            .context("Failed to query AniList")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("AniList API error: {} - {}", status, text);
        }

        let json: serde_json::Value = response.json().await?;

        if let Some(media) = json["data"]["Media"].as_object() {
            let anilist_id = media["id"].as_i64().unwrap_or_default();

            let title = media["title"]["english"]
                .as_str()
                .or_else(|| media["title"]["romaji"].as_str())
                .unwrap_or("Unknown")
                .to_string();

            let description = media["description"]
                .as_str()
                .map(|s| s.replace("<br>", "\n").replace("<br/>", "\n"));

            let rating = media["averageScore"].as_i64();

            let cover_url = media["coverImage"]["large"]
                .as_str()
                .or_else(|| media["coverImage"]["medium"].as_str())
                .map(|s| s.to_string());

            let banner_url = media["bannerImage"].as_str().map(|s| s.to_string());

            let genres: Vec<String> = media["genres"]
                .as_array()
                .map(|g| {
                    g.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let episode_count = media["episodes"].as_i64();

            Ok(Some(AniListMetadata {
                anilist_id,
                title,
                description,
                rating,
                cover_url,
                banner_url,
                genres,
                episode_count,
                cached_at: Utc::now(),
            }))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone)]
pub struct MovieMetadataClient {
    client: reqwest::Client,
}

impl MovieMetadataClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn search_cinema(&self, query: &str) -> Result<Vec<MediaMetadata>> {
        let encoded = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let mut results = Vec::new();

        // Try Cinemeta Movies
        let movie_url =
            format!("https://v3-cinemeta.strem.io/catalog/movie/top/search={encoded}.json");
        if let Ok(resp) = self
            .client
            .get(&movie_url)
            .header("User-Agent", "any-watch/1.0")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(metas) = val["metas"].as_array() {
                        for m in metas {
                            let name = m["name"].as_str().unwrap_or_default().trim().to_string();
                            if name.is_empty() {
                                continue;
                            }
                            let year = m["year"]
                                .as_str()
                                .and_then(|y| y.chars().take(4).collect::<String>().parse().ok())
                                .or_else(|| m["year"].as_u64().map(|y| y as u32));
                            let cover_url = m["poster"].as_str().map(|s| s.to_string());
                            let banner_url = m["background"].as_str().map(|s| s.to_string());
                            let description = m["description"].as_str().map(|s| s.to_string());
                            let rating = m["imdbRating"]
                                .as_str()
                                .and_then(|r| r.parse::<f32>().ok())
                                .or_else(|| m["imdbRating"].as_f64().map(|r| r as f32));
                            let genres = m["genres"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|g| g.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();

                            results.push(MediaMetadata {
                                title: name,
                                original_title: None,
                                year,
                                description,
                                rating,
                                cover_url,
                                banner_url,
                                genres,
                                media_type: "movie".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Try Cinemeta Series
        let series_url =
            format!("https://v3-cinemeta.strem.io/catalog/series/top/search={encoded}.json");
        if let Ok(resp) = self
            .client
            .get(&series_url)
            .header("User-Agent", "any-watch/1.0")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(metas) = val["metas"].as_array() {
                        for m in metas {
                            let name = m["name"].as_str().unwrap_or_default().trim().to_string();
                            if name.is_empty() {
                                continue;
                            }
                            let year = m["year"]
                                .as_str()
                                .and_then(|y| y.chars().take(4).collect::<String>().parse().ok())
                                .or_else(|| m["year"].as_u64().map(|y| y as u32));
                            let cover_url = m["poster"].as_str().map(|s| s.to_string());
                            let banner_url = m["background"].as_str().map(|s| s.to_string());
                            let description = m["description"].as_str().map(|s| s.to_string());
                            let rating = m["imdbRating"]
                                .as_str()
                                .and_then(|r| r.parse::<f32>().ok())
                                .or_else(|| m["imdbRating"].as_f64().map(|r| r as f32));
                            let genres = m["genres"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|g| g.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();

                            results.push(MediaMetadata {
                                title: name,
                                original_title: None,
                                year,
                                description,
                                rating,
                                cover_url,
                                banner_url,
                                genres,
                                media_type: "tv".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // TVMaze fallback for TV series
        if results.is_empty() {
            let tvmaze_url = format!("https://api.tvmaze.com/search/shows?q={encoded}");
            if let Ok(resp) = self
                .client
                .get(&tvmaze_url)
                .header("User-Agent", "any-watch/1.0")
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(shows) = resp.json::<Vec<serde_json::Value>>().await {
                        for item in shows {
                            let show = &item["show"];
                            let name = show["name"].as_str().unwrap_or_default().trim().to_string();
                            if name.is_empty() {
                                continue;
                            }
                            let year = show["premiered"]
                                .as_str()
                                .and_then(|d| d.chars().take(4).collect::<String>().parse().ok());
                            let cover_url = show["image"]["original"]
                                .as_str()
                                .or_else(|| show["image"]["medium"].as_str())
                                .map(|s| s.to_string());
                            let description = show["summary"].as_str().map(|s| {
                                s.replace("<p>", "")
                                    .replace("</p>", "")
                                    .replace("<b>", "")
                                    .replace("</b>", "")
                                    .replace("<i>", "")
                                    .replace("</i>", "")
                            });
                            let rating = show["rating"]["average"].as_f64().map(|r| r as f32);
                            let genres = show["genres"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|g| g.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();

                            results.push(MediaMetadata {
                                title: name,
                                original_title: None,
                                year,
                                description,
                                rating,
                                cover_url,
                                banner_url: None,
                                genres,
                                media_type: "tv".to_string(),
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

impl Default for MovieMetadataClient {
    fn default() -> Self {
        Self::new()
    }
}

type MediaCacheMap = HashMap<String, (Instant, Option<MediaMetadata>)>;

#[derive(Clone)]
pub struct MetadataCache {
    db: Arc<crate::db::Database>,
    anilist: AniListClient,
    cinema: MovieMetadataClient,
    memory_cache: Arc<RwLock<MediaCacheMap>>,
}

impl MetadataCache {
    pub fn new(db: Arc<crate::db::Database>) -> Self {
        Self {
            db,
            anilist: AniListClient::new(),
            cinema: MovieMetadataClient::new(),
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_metadata(&self, anilist_id: i64) -> Result<Option<AniListMetadata>> {
        // Try cache first
        if let Some(cached) = self.db.get_cached_metadata(anilist_id).await? {
            if Utc::now()
                .signed_duration_since(cached.cached_at)
                .num_days()
                < CACHE_TTL_DAYS
            {
                return Ok(Some(cached));
            }
        }

        // Fetch from API
        match self.anilist.get_by_id(anilist_id).await {
            Ok(Some(metadata)) => {
                let _ = self.db.cache_metadata(&metadata).await;
                Ok(Some(metadata))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::warn!("Failed to fetch metadata from AniList: {}", e);
                Ok(self.db.get_cached_metadata(anilist_id).await?)
            }
        }
    }

    pub async fn search_and_cache(&self, query: &str) -> Result<Vec<AniListMetadata>> {
        let results = self.anilist.search_anime(query).await?;
        for metadata in &results {
            let _ = self.db.cache_metadata(metadata).await;
        }
        Ok(results)
    }

    pub async fn resolve_media_metadata(
        &self,
        query: &str,
        category: Option<&str>,
    ) -> Result<Option<MediaMetadata>> {
        let key = format!(
            "{}:{}",
            category.unwrap_or("all"),
            query.trim().to_lowercase()
        );

        {
            let cache = self.memory_cache.read().await;
            if let Some((instant, meta)) = cache.get(&key) {
                if instant.elapsed() < Duration::from_secs(3600 * 24) {
                    return Ok(meta.clone());
                }
            }
        }

        let is_anime = matches!(category, Some("anime") | Some("Anime"));
        let mut resolved: Option<MediaMetadata> = None;

        if is_anime || category.is_none() || category == Some("all") {
            if let Ok(anime_list) = self.anilist.search_anime(query).await {
                if let Some(first) = anime_list.into_iter().next() {
                    resolved = Some(MediaMetadata {
                        title: first.title,
                        original_title: None,
                        year: None,
                        description: first.description,
                        rating: first.rating.map(|r| r as f32 / 10.0),
                        cover_url: first.cover_url,
                        banner_url: first.banner_url,
                        genres: first.genres,
                        media_type: "anime".to_string(),
                    });
                }
            }
        }

        if resolved.is_none() {
            if let Ok(cinema_list) = self.cinema.search_cinema(query).await {
                if let Some(first) = cinema_list.into_iter().next() {
                    resolved = Some(first);
                }
            }
        }

        {
            let mut cache = self.memory_cache.write().await;
            cache.insert(key, (Instant::now(), resolved.clone()));
        }

        Ok(resolved)
    }

    pub async fn enrich_anime(&self, base: crate::providers::Anime) -> EnrichedAnime {
        match self.search_and_cache(&base.title).await {
            Ok(results) => {
                let metadata = results.into_iter().next();
                EnrichedAnime { base, metadata }
            }
            Err(e) => {
                tracing::warn!("Failed to enrich anime '{}': {}", base.title, e);
                EnrichedAnime {
                    base,
                    metadata: None,
                }
            }
        }
    }

    pub async fn enrich_anime_list(
        &self,
        anime_list: Vec<crate::providers::Anime>,
    ) -> Vec<EnrichedAnime> {
        let mut enriched = Vec::new();
        for anime in anime_list {
            enriched.push(self.enrich_anime(anime).await);
        }
        enriched
    }
}

impl Default for AniListClient {
    fn default() -> Self {
        Self::new()
    }
}
