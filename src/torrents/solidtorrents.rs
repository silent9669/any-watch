use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct SolidTorrentsProvider {
    client: Client,
}

impl SolidTorrentsProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct SolidResponse {
    results: Option<Vec<SolidItem>>,
}

#[derive(Debug, Deserialize)]
struct SolidItem {
    id: Option<String>,
    title: Option<String>,
    magnet: Option<String>,
    size: Option<u64>,
    swarm: Option<SolidSwarm>,
    category: Option<String>,
    imported: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SolidSwarm {
    seeders: Option<u32>,
    leechers: Option<u32>,
}

#[async_trait]
impl TorrentSearchProvider for SolidTorrentsProvider {
    fn name(&self) -> &'static str {
        "SolidTorrents"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[
            TorrentCategory::All,
            TorrentCategory::Movies,
            TorrentCategory::Tv,
            TorrentCategory::Anime,
        ]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        _page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        let mut url = Url::parse("https://solidtorrents.to/api/v1/search")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("sort", "seeders");

        match category {
            TorrentCategory::Movies => {
                url.query_pairs_mut().append_pair("category", "Video");
            }
            TorrentCategory::Tv => {
                url.query_pairs_mut().append_pair("category", "Video");
            }
            TorrentCategory::Anime => {
                url.query_pairs_mut().append_pair("category", "Anime");
            }
            _ => {
                url.query_pairs_mut().append_pair("category", "all");
            }
        }

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("SolidTorrents request failed")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let body: SolidResponse = response
            .json()
            .await
            .context("SolidTorrents returned invalid JSON")?;

        let mut results = Vec::new();
        let items = match body.results {
            Some(i) => i,
            None => return Ok(results),
        };

        for item in items {
            let title = match item.title {
                Some(t) if !t.is_empty() => t.trim().to_string(),
                _ => continue,
            };

            let magnet_url = match item.magnet {
                Some(m) if !m.is_empty() => m,
                _ => continue,
            };

            let size_bytes = item.size.unwrap_or(0);
            let seeds = item.swarm.as_ref().and_then(|s| s.seeders).unwrap_or(0);
            let peers = item.swarm.as_ref().and_then(|s| s.leechers).unwrap_or(0);
            let quality = detect_quality(&title);
            let (has_engsub, has_vietsub) = detect_subtitles(&title);

            let cat_str = item.category.unwrap_or_else(|| "Video".to_string());
            let upload_date = item.imported.map(|d| d.chars().take(10).collect());

            let id = format!(
                "solid-{}",
                item.id.unwrap_or_else(|| title
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .take(16)
                    .collect())
            );

            results.push(TorrentSearchResult {
                id,
                title,
                magnet_url,
                torrent_url: None,
                source: "SolidTorrents".to_string(),
                category: cat_str,
                size_bytes,
                formatted_size: format_bytes(size_bytes),
                seeds,
                peers,
                quality,
                upload_date,
                has_engsub,
                has_vietsub,
            });
        }

        Ok(results)
    }
}
