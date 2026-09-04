use super::{Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header::{self, HeaderMap};
use std::collections::HashMap;
use std::time::Duration;

const OPHIM_APIS: &[&str] = &[
    "https://phimapi.com/v1/api",
    "https://phimapi.com",
    "https://ophim17.cc/v1/api",
    "https://ophim19.cc/v1/api",
    "https://ophim1.com/v1/api",
];
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

pub struct OphimProvider {
    client: reqwest::Client,
}

impl Default for OphimProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OphimProvider {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, header::HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        ));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    async fn fetch_json(&self, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut last_error = None;
        for api_base in OPHIM_APIS {
            let url = format!(
                "{}/{}",
                api_base.trim_end_matches('/'),
                path.trim_start_matches('/')
            );
            match self.client.get(&url).query(query).send().await {
                Ok(response) if response.status().is_success() => {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        return Ok(json);
                    }
                }
                Ok(response) => {
                    last_error = Some(anyhow::anyhow!(
                        "OPhim mirror {} returned HTTP {}",
                        api_base,
                        response.status()
                    ));
                }
                Err(error) => {
                    last_error = Some(error.into());
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All OPhim API mirrors failed")))
    }

    fn absolute_image_url(cdn: &str, value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.starts_with("http") {
            Some(trimmed.to_string())
        } else {
            Some(format!(
                "{}/uploads/movies/{}",
                cdn.trim_end_matches('/'),
                trimmed.trim_start_matches('/')
            ))
        }
    }
}

#[async_trait]
impl AnimeProvider for OphimProvider {
    fn name(&self) -> &str {
        "OPhim"
    }

    fn language(&self) -> Language {
        Language::Vietnamese
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["🇻🇳".to_string()]
    }

    fn website_url(&self) -> Option<&'static str> {
        Some("https://ophim19.cc")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            subtitles: false,
            ..ProviderCapabilities::default()
        }
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let response = self
            .fetch_json("tim-kiem", &[("keyword", query), ("limit", "40")])
            .await
            .context("Failed to search OPhim")?;

        let mut results = Vec::new();

        if let Some(data) = response.get("data") {
            if let Some(items) = data.get("items").and_then(|i| i.as_array()) {
                let mut items = items.clone();
                // Sort items to prioritize anime (type: "hoathinh")
                items.sort_by(|a, b| {
                    let a_type = a["type"].as_str().unwrap_or("");
                    let b_type = b["type"].as_str().unwrap_or("");
                    let a_priority = if a_type == "hoathinh" { 0 } else { 1 };
                    let b_priority = if b_type == "hoathinh" { 0 } else { 1 };
                    a_priority.cmp(&b_priority)
                });

                for item in items {
                    let slug = item["slug"].as_str().unwrap_or_default().to_string();
                    let name = item["name"].as_str().unwrap_or_default().to_string();
                    let thumb = item["thumb_url"].as_str().unwrap_or_default().to_string();
                    let poster = item["poster_url"].as_str().unwrap_or_default().to_string();

                    // Use APP_DOMAIN_CDN_IMAGE if available, or fallback to known CDN
                    let cdn = response["data"]["APP_DOMAIN_CDN_IMAGE"]
                        .as_str()
                        .unwrap_or("https://img.ophim.live");

                    let image_url = if poster.starts_with("http") {
                        poster.clone()
                    } else if thumb.starts_with("http") {
                        thumb.clone()
                    } else {
                        format!("{}/uploads/movies/{}", cdn.trim_end_matches('/'), poster)
                    };

                    let episode_count = item["episode_total"]
                        .as_str()
                        .and_then(|e| e.parse::<u32>().ok());

                    if !slug.is_empty() && !name.is_empty() {
                        results.push(Anime {
                            id: slug,
                            provider: "OPhim".to_string(),
                            title: name,
                            cover_url: image_url,
                            banner_url: Some(if poster.starts_with("http") {
                                poster
                            } else {
                                thumb
                            }),
                            language: Language::Vietnamese,
                            total_episodes: episode_count,
                            synopsis: item["content"].as_str().map(|s| s.to_string()),
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    async fn catalog(&self) -> Result<Vec<Anime>> {
        let response = match self
            .fetch_json("danh-sach/hoat-hinh", &[("page", "1")])
            .await
        {
            Ok(res) => res,
            Err(_) => self
                .fetch_json("danh-sach/phim-moi-cap-nhat", &[("page", "1")])
                .await
                .context("Failed to get OPhim catalog")?,
        };

        let mut results = Vec::new();

        if let Some(data) = response.get("data") {
            if let Some(items) = data.get("items").and_then(|i| i.as_array()) {
                let cdn = data["APP_DOMAIN_CDN_IMAGE"]
                    .as_str()
                    .unwrap_or("https://img.ophim.live");

                for item in items {
                    let slug = item["slug"].as_str().unwrap_or_default().to_string();
                    let name = item["name"].as_str().unwrap_or_default().to_string();

                    let thumb = item["thumb_url"].as_str().unwrap_or_default();
                    let poster = item["poster_url"].as_str().unwrap_or_default();

                    let image_url = if poster.starts_with("http") {
                        poster.to_string()
                    } else if thumb.starts_with("http") {
                        thumb.to_string()
                    } else {
                        format!("{}/uploads/movies/{}", cdn.trim_end_matches('/'), poster)
                    };

                    let episode_count = item["episode_total"]
                        .as_str()
                        .and_then(|e| e.parse::<u32>().ok());

                    if !slug.is_empty() && !name.is_empty() {
                        results.push(Anime {
                            id: slug,
                            provider: "OPhim".to_string(),
                            title: name,
                            cover_url: image_url,
                            banner_url: Some(if poster.starts_with("http") {
                                poster.to_string()
                            } else {
                                thumb.to_string()
                            }),
                            language: Language::Vietnamese,
                            total_episodes: episode_count,
                            synopsis: item["content"].as_str().map(|s| s.to_string()),
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let response = self
            .fetch_json(&format!("phim/{anime_id}"), &[])
            .await
            .context("Failed to get OPhim details")?;

        let (item, cdn) = if let Some(data) = response.get("data") {
            let item = if data.get("item").is_some() {
                &data["item"]
            } else {
                data
            };
            let cdn = data["APP_DOMAIN_CDN_IMAGE"]
                .as_str()
                .unwrap_or("https://img.ophim.live");
            (item, cdn)
        } else if let Some(movie) = response.get("movie") {
            (movie, "https://phimimg.com")
        } else {
            return Ok(None);
        };

        let title = item["name"].as_str().unwrap_or_default().to_string();
        if title.is_empty() {
            return Ok(None);
        }

        let poster_url = item["poster_url"].as_str().unwrap_or_default();
        let thumb_url = item["thumb_url"].as_str().unwrap_or_default();
        let cover_url = Self::absolute_image_url(cdn, poster_url)
            .or_else(|| Self::absolute_image_url(cdn, thumb_url))
            .unwrap_or_default();
        let banner_url = Self::absolute_image_url(cdn, thumb_url)
            .or_else(|| Self::absolute_image_url(cdn, poster_url));
        let total_episodes = item["episode_total"]
            .as_str()
            .and_then(|e| e.parse::<u32>().ok());

        Ok(Some(Anime {
            id: anime_id.to_string(),
            provider: "OPhim".to_string(),
            title,
            cover_url,
            banner_url,
            language: Language::Vietnamese,
            total_episodes,
            synopsis: item["content"].as_str().map(|s| s.to_string()),
        }))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let response = self
            .fetch_json(&format!("phim/{anime_id}"), &[])
            .await
            .context("Failed to get OPhim episodes")?;

        let mut episodes = Vec::new();

        let episode_list = response
            .get("data")
            .and_then(|d| d.get("item"))
            .and_then(|i| i.get("episodes"))
            .or_else(|| response.get("data").and_then(|d| d.get("episodes")))
            .or_else(|| response.get("episodes"))
            .and_then(|e| e.as_array());

        if let Some(episode_list) = episode_list {
            for server in episode_list {
                if let Some(server_data) = server.get("server_data").and_then(|s| s.as_array()) {
                    for ep in server_data {
                        let name = ep["name"].as_str().unwrap_or("");
                        let ep_num = super::parse_episode_number(name);

                        if ep_num > 0 {
                            episodes.push(Episode {
                                id: format!("{}:{}", anime_id, name),
                                number: ep_num,
                                aniskip_episode_number: super::aniskip_episode_number(name),
                                title: Some(ep["filename"].as_str().unwrap_or("").to_string()),
                                thumbnail: None,
                            });
                        }
                    }
                }
            }
        }

        episodes.sort_by_key(|episode| (episode.number, episode.aniskip_episode_number.is_none()));
        episodes.dedup_by(|a, b| a.number == b.number);

        Ok(episodes)
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        let (anime_slug, episode_name) = episode_id
            .split_once(':')
            .context("Invalid episode_id format. Expected 'anime_slug:episode_name'")?;

        let response = self
            .fetch_json(&format!("phim/{anime_slug}"), &[])
            .await
            .context("Failed to get OPhim stream")?;

        let mut stream_url = String::new();
        let subtitles = Vec::new();

        let episode_list = response
            .get("data")
            .and_then(|d| d.get("item"))
            .and_then(|i| i.get("episodes"))
            .or_else(|| response.get("data").and_then(|d| d.get("episodes")))
            .or_else(|| response.get("episodes"))
            .and_then(|e| e.as_array());

        if let Some(episode_list) = episode_list {
            // Sort servers to prioritize Vietsub (usually "#Hà Nội")
            let mut sorted_servers = episode_list.clone();
            sorted_servers.sort_by(|a, b| {
                let a_name = a["server_name"].as_str().unwrap_or("").to_lowercase();
                let b_name = b["server_name"].as_str().unwrap_or("").to_lowercase();
                let a_priority = if a_name.contains("hà nội") || a_name.contains("vietsub") {
                    0
                } else {
                    1
                };
                let b_priority = if b_name.contains("hà nội") || b_name.contains("vietsub") {
                    0
                } else {
                    1
                };
                a_priority.cmp(&b_priority)
            });

            'outer: for server in sorted_servers {
                if let Some(server_data) = server.get("server_data").and_then(|s| s.as_array()) {
                    for ep in server_data {
                        let name = ep["name"].as_str().unwrap_or("");
                        if name == episode_name {
                            if let Some(link) = ep["link_m3u8"].as_str() {
                                if !link.is_empty() {
                                    stream_url = link.to_string();
                                }
                            }

                            if stream_url.is_empty() {
                                if let Some(link) = ep["link_embed"].as_str() {
                                    if link.contains("url=") {
                                        if let Some(extracted) = link.split("url=").nth(1) {
                                            stream_url = extracted.to_string();
                                        }
                                    }
                                }
                            }

                            if !stream_url.is_empty() {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        if stream_url.is_empty() {
            anyhow::bail!("No working stream URL found for this episode.");
        }

        let mut headers: HashMap<String, String> = HashMap::new();
        headers.insert("User-Agent".to_string(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string());
        headers.insert("Referer".to_string(), "https://ophim17.cc/".to_string());
        headers.insert("Origin".to_string(), "https://ophim17.cc".to_string());

        Ok(StreamInfo {
            video_url: stream_url,
            subtitles,
            qualities: vec!["auto".to_string()],
            headers,
            use_curl: false,
        })
    }
}
