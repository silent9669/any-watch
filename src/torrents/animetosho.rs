use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct AnimeToshoProvider {
    client: Client,
}

impl AnimeToshoProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct ToshoItem {
    id: Option<u64>,
    title: Option<String>,
    magnet_url: Option<String>,
    torrent_url: Option<String>,
    total_size: Option<u64>,
    seeders: Option<u32>,
    leechers: Option<u32>,
    timestamp: Option<i64>,
    #[serde(default)]
    has_subtitles: Option<bool>,
}

#[async_trait]
impl TorrentSearchProvider for AnimeToshoProvider {
    fn name(&self) -> &'static str {
        "AnimeTosho"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[TorrentCategory::All, TorrentCategory::Anime]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
    ) -> Result<Vec<TorrentSearchResult>> {
        if category == TorrentCategory::Movies || category == TorrentCategory::Tv {
            return Ok(Vec::new());
        }

        let mut url = Url::parse("https://animetosho.org/api/v1/search")?;
        url.query_pairs_mut().append_pair("q", query);

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("AnimeTosho request failed")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let items: Vec<ToshoItem> = response.json().await.unwrap_or_default();
        let mut results = Vec::new();

        for item in items {
            let title = match item.title {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };

            let magnet_url = match item.magnet_url {
                Some(m) if !m.is_empty() => m,
                _ => continue,
            };

            let size_bytes = item.total_size.unwrap_or(0);
            let seeds = item.seeders.unwrap_or(0);
            let peers = item.leechers.unwrap_or(0);
            let id = format!("animetosho-{}", item.id.unwrap_or(0));
            let quality = detect_quality(&title);
            let (mut has_engsub, has_vietsub) = detect_subtitles(&title);
            if item.has_subtitles.unwrap_or(false) {
                has_engsub = true;
            }

            let upload_date = item.timestamp.map(|ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_default()
            });

            results.push(TorrentSearchResult {
                id,
                title,
                magnet_url,
                torrent_url: item.torrent_url,
                source: "AnimeTosho".to_string(),
                category: "Anime".to_string(),
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
