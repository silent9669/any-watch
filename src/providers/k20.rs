use super::{
    Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo, Subtitle,
    SubtitleFormat,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header::{self, HeaderMap};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const K20_BASE_URL: &str = "https://sc.k-20.xyz";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Deserialize)]
struct K20CatalogResponse {
    #[serde(default)]
    metas: Vec<K20MetaSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20MetaSummary {
    id: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    media_type: String,
    name: String,
    poster: Option<String>,
    background: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20MetaDetailsResponse {
    meta: Option<K20MetaDetail>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20MetaDetail {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    media_type: String,
    name: String,
    poster: Option<String>,
    background: Option<String>,
    description: Option<String>,
    #[serde(default)]
    videos: Vec<K20VideoItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20VideoItem {
    id: String,
    title: Option<String>,
    #[allow(dead_code)]
    season: Option<u32>,
    episode: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20StreamResponse {
    #[serde(default)]
    streams: Vec<K20StreamItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20StreamItem {
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    title: Option<String>,
    url: Option<String>,
    #[serde(default)]
    subtitles: Vec<K20SubtitleItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct K20SubtitleItem {
    url: String,
    lang: Option<String>,
}

pub struct K20Provider {
    client: reqwest::Client,
}

impl Default for K20Provider {
    fn default() -> Self {
        Self::new()
    }
}

impl K20Provider {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://sc.k-20.xyz/"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("Failed to create K20 HTTP client");

        Self { client }
    }

    fn split_media_id(compound_id: &str) -> (&str, &str) {
        if let Some((media_type, rest)) = compound_id.split_once(':') {
            if media_type == "series" || media_type == "movie" || media_type == "tv" {
                return (media_type, rest);
            }
        }
        ("series", compound_id)
    }

    fn parse_episode_number(video: &K20VideoItem, index: usize) -> u32 {
        if let Some(title) = &video.title {
            let parsed = super::parse_episode_number(title);
            if parsed > 0 {
                return parsed;
            }
        }
        if let Some(ep) = video.episode {
            if ep > 0 {
                return ep;
            }
        }
        (index + 1) as u32
    }

    fn subtitle_format(value: &str) -> SubtitleFormat {
        let path = url::Url::parse(value)
            .ok()
            .map(|url| url.path().to_ascii_lowercase())
            .unwrap_or_else(|| {
                value
                    .split(['?', '#'])
                    .next()
                    .unwrap_or(value)
                    .to_ascii_lowercase()
            });
        if path.ends_with(".vtt") {
            SubtitleFormat::WebVtt
        } else if path.ends_with(".srt") {
            SubtitleFormat::Srt
        } else if path.ends_with(".ass") {
            SubtitleFormat::Ass
        } else {
            SubtitleFormat::Unknown
        }
    }
}

#[async_trait]
impl AnimeProvider for K20Provider {
    fn name(&self) -> &str {
        "K20"
    }

    fn language(&self) -> Language {
        Language::Vietnamese
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["🇻🇳".to_string(), "en".to_string()]
    }

    fn website_url(&self) -> Option<&'static str> {
        Some(K20_BASE_URL)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            search: true,
            details: true,
            episodes: true,
            playback: true,
            subtitles: true,
        }
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let catalogs: &[(&str, &str)] = &[
            ("series", "nguonc-series"),
            ("movie", "nguonc-movie"),
            ("series", "hh3d-series"),
            ("movie", "hh3d-movie"),
            ("series", "stp-series"),
            ("movie", "stp-movie"),
            ("series", "vsmov-series"),
            ("movie", "vsmov-movie"),
            ("series", "yan-series"),
            ("movie", "yan-movie"),
        ];

        let encoded_query: String =
            url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
        let mut futures = Vec::new();

        for &(media_type, catalog_id) in catalogs {
            let url = format!(
                "{}/catalog/{}/{}/search={}.json",
                K20_BASE_URL, media_type, catalog_id, encoded_query
            );
            let client = self.client.clone();
            let media_type = media_type.to_string();

            futures.push(tokio::spawn(async move {
                let response = client.get(&url).send().await.ok()?;
                if !response.status().is_success() {
                    return None;
                }
                let payload: K20CatalogResponse = response.json().await.ok()?;
                Some((media_type, payload.metas))
            }));
        }

        let mut all_results = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for handle in futures {
            if let Ok(Some((media_type, metas))) = handle.await {
                for meta in metas {
                    if !seen_ids.insert(meta.id.clone()) {
                        continue;
                    }

                    let id = format!("{}:{}", media_type, meta.id);
                    all_results.push(Anime {
                        id,
                        provider: "K20".to_string(),
                        title: meta.name,
                        cover_url: meta.poster.unwrap_or_default(),
                        banner_url: meta.background,
                        language: Language::Vietnamese,
                        total_episodes: None,
                        synopsis: meta.description,
                    });
                }
            }
        }

        Ok(all_results)
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let (media_type, target_id) = Self::split_media_id(anime_id);
        let detail_url = format!("{}/meta/{}/{}.json", K20_BASE_URL, media_type, target_id);

        let response = self
            .client
            .get(&detail_url)
            .send()
            .await
            .context("Failed to query K20 meta endpoint")?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let payload: K20MetaDetailsResponse = response
            .json()
            .await
            .context("Failed to parse K20 meta details response")?;

        let Some(meta) = payload.meta else {
            return Ok(None);
        };

        let total_episodes = if !meta.videos.is_empty() {
            Some(meta.videos.len() as u32)
        } else {
            Some(1)
        };

        Ok(Some(Anime {
            id: anime_id.to_string(),
            provider: "K20".to_string(),
            title: meta.name,
            cover_url: meta.poster.unwrap_or_default(),
            banner_url: meta.background,
            language: Language::Vietnamese,
            total_episodes,
            synopsis: meta.description,
        }))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let (media_type, target_id) = Self::split_media_id(anime_id);
        let detail_url = format!("{}/meta/{}/{}.json", K20_BASE_URL, media_type, target_id);

        let response = self
            .client
            .get(&detail_url)
            .send()
            .await
            .context("Failed to query K20 meta for episodes")?
            .error_for_status()
            .context("K20 meta endpoint returned an error")?;

        let payload: K20MetaDetailsResponse = response
            .json()
            .await
            .context("Failed to parse K20 meta payload")?;

        let Some(meta) = payload.meta else {
            return Ok(Vec::new());
        };

        if meta.videos.is_empty() {
            return Ok(vec![Episode {
                id: format!("{}:{}", media_type, target_id),
                number: 1,
                aniskip_episode_number: Some(1),
                title: Some("Full".to_string()),
                thumbnail: meta.poster,
            }]);
        }

        let mut episodes = Vec::new();
        for (idx, video) in meta.videos.iter().enumerate() {
            let ep_num = Self::parse_episode_number(video, idx);
            episodes.push(Episode {
                id: format!("{}:{}", media_type, video.id),
                number: ep_num,
                aniskip_episode_number: Some(ep_num),
                title: video.title.clone(),
                thumbnail: None,
            });
        }

        episodes.sort_by_key(|ep| ep.number);
        episodes.dedup_by(|a, b| a.number == b.number && a.id == b.id);

        Ok(episodes)
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        let (media_type, stream_target_id) = Self::split_media_id(episode_id);
        let stream_url = format!(
            "{}/stream/{}/{}.json",
            K20_BASE_URL, media_type, stream_target_id
        );

        let response = self
            .client
            .get(&stream_url)
            .send()
            .await
            .context("Failed to query K20 stream endpoint")?
            .error_for_status()
            .context("K20 stream endpoint returned an error")?;

        let payload: K20StreamResponse = response
            .json()
            .await
            .context("Failed to parse K20 stream response")?;

        let best_stream = payload
            .streams
            .into_iter()
            .find(|s| {
                s.url
                    .as_ref()
                    .map(|u| !u.trim().is_empty())
                    .unwrap_or(false)
            })
            .context("No playable stream URL found in K20 stream response")?;

        let video_url = best_stream.url.unwrap();

        let subtitles = best_stream
            .subtitles
            .into_iter()
            .map(|sub| Subtitle {
                language: sub.lang.unwrap_or_else(|| "Default".to_string()),
                format: Self::subtitle_format(&sub.url),
                url: sub.url,
            })
            .collect();

        let mut headers = HashMap::new();
        headers.insert(
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
        );
        headers.insert("Referer".to_string(), "https://sc.k-20.xyz/".to_string());
        headers.insert("Origin".to_string(), "https://sc.k-20.xyz".to_string());

        Ok(StreamInfo {
            video_url,
            subtitles,
            qualities: vec!["Auto".to_string()],
            headers,
            use_curl: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_media_id() {
        assert_eq!(
            K20Provider::split_media_id("series:nguonc:naruto:tap-1"),
            ("series", "nguonc:naruto:tap-1")
        );
        assert_eq!(
            K20Provider::split_media_id("movie:nguonc:one-piece-movie"),
            ("movie", "nguonc:one-piece-movie")
        );
        assert_eq!(
            K20Provider::split_media_id("nguonc:naruto"),
            ("series", "nguonc:naruto")
        );
    }

    #[test]
    fn test_subtitle_format_uses_url_path() {
        assert_eq!(
            K20Provider::subtitle_format("https://cdn.example/subtitle.vtt?token=secret"),
            SubtitleFormat::WebVtt,
        );
        assert_eq!(
            K20Provider::subtitle_format("https://cdn.example/subtitle.srt?token=secret"),
            SubtitleFormat::Srt,
        );
        assert_eq!(
            K20Provider::subtitle_format("https://cdn.example/subtitle.ass#track"),
            SubtitleFormat::Ass,
        );
    }

    #[test]
    fn test_parse_episode_number() {
        let video = K20VideoItem {
            id: "nguonc:naruto:tap-5".to_string(),
            title: Some("Tập 5".to_string()),
            season: Some(1),
            episode: Some(5),
        };
        assert_eq!(K20Provider::parse_episode_number(&video, 0), 5);

        let later_season_video = K20VideoItem {
            id: "nguonc:show:2:1".to_string(),
            title: Some("13".to_string()),
            season: Some(2),
            episode: Some(1),
        };
        assert_eq!(
            K20Provider::parse_episode_number(&later_season_video, 12),
            13
        );

        let video2 = K20VideoItem {
            id: "nguonc:naruto:tap-full".to_string(),
            title: Some("Full".to_string()),
            season: None,
            episode: None,
        };
        assert_eq!(K20Provider::parse_episode_number(&video2, 0), 1);
    }
}
