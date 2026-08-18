use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct ThePirateBayProvider {
    client: Client,
}

impl ThePirateBayProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct ApibayItem {
    id: Option<String>,
    name: Option<String>,
    info_hash: Option<String>,
    size: Option<String>,
    seeders: Option<String>,
    leechers: Option<String>,
    added: Option<String>,
    category: Option<String>,
}

#[async_trait]
impl TorrentSearchProvider for ThePirateBayProvider {
    fn name(&self) -> &'static str {
        "ThePirateBay"
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
    ) -> Result<Vec<TorrentSearchResult>> {
        let mut url = Url::parse("https://apibay.org/q.php")?;
        url.query_pairs_mut().append_pair("q", query);

        match category {
            TorrentCategory::Movies => {
                url.query_pairs_mut().append_pair("cat", "201,207"); // Movies, HD Movies
            }
            TorrentCategory::Tv => {
                url.query_pairs_mut().append_pair("cat", "205,208"); // TV shows, HD TV
            }
            _ => {
                url.query_pairs_mut().append_pair("cat", "200"); // Video category
            }
        }

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
            )
            .send()
            .await
            .context("ThePirateBay request failed")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let items: Vec<ApibayItem> = response.json().await.unwrap_or_default();
        let mut results = Vec::new();

        for item in items {
            let name = match item.name {
                Some(n) if !n.is_empty() && n != "No results returned" => n,
                _ => continue,
            };

            let hash = match item.info_hash {
                Some(h) if !h.is_empty() && h != "0000000000000000000000000000000000000000" => h,
                _ => continue,
            };

            let size_bytes: u64 = item.size.and_then(|s| s.parse().ok()).unwrap_or(0);
            let seeds: u32 = item.seeders.and_then(|s| s.parse().ok()).unwrap_or(0);
            let peers: u32 = item.leechers.and_then(|s| s.parse().ok()).unwrap_or(0);

            let magnet_url = format!(
                "magnet:?xt=urn:btih:{}&dn={}&tr=udp://tracker.opentrackr.org:1337/announce&tr=udp://open.tracker.cl:1337/announce&tr=udp://tracker.openbittorrent.com:6969/announce&tr=udp://opentracker.i2p.rocks:6969/announce",
                hash,
                url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>()
            );

            let quality = detect_quality(&name);
            let (has_engsub, has_vietsub) = detect_subtitles(&name);

            let category_str = match item.category.as_deref() {
                Some("201") | Some("207") => "Movies",
                Some("205") | Some("208") => "TV",
                _ => "Video",
            }
            .to_string();

            let upload_date = item
                .added
                .and_then(|ts| ts.parse::<i64>().ok())
                .and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                });

            let id = format!("tpb-{}", item.id.unwrap_or_else(|| hash.clone()));

            results.push(TorrentSearchResult {
                id,
                title: name,
                magnet_url,
                torrent_url: None,
                source: "ThePirateBay".to_string(),
                category: category_str,
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
