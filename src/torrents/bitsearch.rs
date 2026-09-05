use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct BitsearchProvider {
    client: Client,
}

impl BitsearchProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct BitsearchResponse {
    results: Option<Vec<BitsearchItem>>,
}

#[derive(Debug, Deserialize)]
struct BitsearchItem {
    id: Option<String>,
    title: Option<String>,
    infohash: Option<String>,
    size: Option<u64>,
    seeders: Option<u32>,
    leechers: Option<u32>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
}

#[async_trait]
impl TorrentSearchProvider for BitsearchProvider {
    fn name(&self) -> &'static str {
        "Bitsearch"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[
            TorrentCategory::All,
            TorrentCategory::Movies,
            TorrentCategory::Tv,
            TorrentCategory::Anime,
            TorrentCategory::Documentaries,
        ]
    }

    async fn search(
        &self,
        query: &str,
        _category: TorrentCategory,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        let mut url = Url::parse("https://bitsearch.to/api/v1/search")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("page", &page.max(1).to_string());

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            )
            .send()
            .await
            .context("Bitsearch request failed")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let body: BitsearchResponse = response
            .json()
            .await
            .context("Bitsearch returned invalid JSON")?;

        let mut results = Vec::new();
        let items = match body.results {
            Some(i) => i,
            None => return Ok(results),
        };

        for item in items {
            let Some(title) = item.title else { continue };
            let Some(infohash) = item.infohash else {
                continue;
            };
            let size = item.size.unwrap_or(0);
            let seeds = item.seeders.unwrap_or(0);
            let peers = item.leechers.unwrap_or(0);

            let magnet_url = format!(
                "magnet:?xt=urn:btih:{}&dn={}",
                infohash,
                url::form_urlencoded::byte_serialize(title.as_bytes()).collect::<String>()
            );

            let quality = detect_quality(&title);
            let (has_engsub, has_vietsub) = detect_subtitles(&title);
            let formatted_size = format_bytes(size);

            results.push(TorrentSearchResult {
                id: item.id.unwrap_or_else(|| infohash.clone()),
                title,
                magnet_url,
                torrent_url: None,
                source: "Bitsearch".to_string(),
                category: "all".to_string(),
                size_bytes: size,
                formatted_size,
                seeds,
                peers,
                quality,
                upload_date: item.updated_at,
                has_engsub,
                has_vietsub,
            });
        }

        Ok(results)
    }
}
