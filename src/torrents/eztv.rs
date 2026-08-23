use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct EztvProvider {
    client: Client,
}

impl EztvProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct EztvResponse {
    torrents: Option<Vec<EztvTorrent>>,
}

#[derive(Debug, Deserialize)]
struct EztvTorrent {
    id: Option<u64>,
    hash: Option<String>,
    filename: Option<String>,
    title: Option<String>,
    torrent_url: Option<String>,
    magnet_url: Option<String>,
    size_bytes: Option<u64>,
    seeds: Option<u32>,
    peers: Option<u32>,
    date_released_unix: Option<i64>,
}

#[async_trait]
impl TorrentSearchProvider for EztvProvider {
    fn name(&self) -> &'static str {
        "EZTV"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[
            TorrentCategory::All,
            TorrentCategory::Tv,
            TorrentCategory::Anime,
            TorrentCategory::Movies,
        ]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        if category == TorrentCategory::Movies {
            // EZTV is TV / series focused
            return Ok(Vec::new());
        }

        let endpoints = [
            "https://eztvx.to/api/get-torrents",
            "https://eztv.re/api/get-torrents",
        ];

        let mut body: Option<EztvResponse> = None;

        for base_url in endpoints {
            let mut url = match Url::parse(base_url) {
                Ok(u) => u,
                Err(_) => continue,
            };
            url.query_pairs_mut()
                .append_pair("limit", "50")
                .append_pair("page", &page.max(1).to_string());

            let response = self
                .client
                .get(url)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
                )
                .send()
                .await;

            if let Ok(resp) = response {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<EztvResponse>().await {
                        body = Some(json);
                        break;
                    }
                }
            }
        }

        let mut results = Vec::new();
        let torrents = match body.and_then(|b| b.torrents) {
            Some(t) => t,
            None => return Ok(results),
        };

        let query_words: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        for item in torrents {
            let title = item
                .title
                .or(item.filename)
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }

            let lower_title = title.to_lowercase();
            if !query_words.is_empty() && !query_words.iter().all(|word| lower_title.contains(word))
            {
                continue;
            }

            let magnet_url = match item.magnet_url {
                Some(m) if !m.is_empty() => m,
                _ => {
                    if let Some(hash) = item.hash {
                        format!(
                            "magnet:?xt=urn:btih:{}&dn={}",
                            hash,
                            url::form_urlencoded::byte_serialize(title.as_bytes())
                                .collect::<String>()
                        )
                    } else {
                        continue;
                    }
                }
            };

            let size_bytes = item.size_bytes.unwrap_or(0);
            let seeds = item.seeds.unwrap_or(0);
            let peers = item.peers.unwrap_or(0);
            let quality = detect_quality(&title);
            let (has_engsub, has_vietsub) = detect_subtitles(&title);

            let upload_date = item.date_released_unix.and_then(|ts| {
                chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.format("%Y-%m-%d").to_string())
            });

            let id = format!(
                "eztv-{}",
                item.id.map(|i| i.to_string()).unwrap_or_else(|| title
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .take(16)
                    .collect())
            );

            results.push(TorrentSearchResult {
                id,
                title,
                magnet_url,
                torrent_url: item.torrent_url,
                source: "EZTV".to_string(),
                category: "TV".to_string(),
                size_bytes,
                formatted_size: format_bytes(size_bytes),
                seeds,
                peers,
                quality,
                upload_date,
                has_engsub: has_engsub || true, // TV releases have English audio/subs by default
                has_vietsub,
            });
        }

        Ok(results)
    }
}
