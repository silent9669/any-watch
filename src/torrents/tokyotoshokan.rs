use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct TokyoToshokanProvider {
    client: Client,
}

impl TokyoToshokanProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TorrentSearchProvider for TokyoToshokanProvider {
    fn name(&self) -> &'static str {
        "TokyoToshokan"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[TorrentCategory::All, TorrentCategory::Anime]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        _page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        if category == TorrentCategory::Movies || category == TorrentCategory::Tv {
            return Ok(Vec::new());
        }

        let mut url = Url::parse("https://www.tokyotosho.info/rss.php")?;
        url.query_pairs_mut()
            .append_pair("terms", query)
            .append_pair("type", "1"); // Anime

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("TokyoToshokan request failed")?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let xml = response
            .text()
            .await
            .context("TokyoToshokan invalid RSS text")?;

        let results = parse_tokyotosho_rss(&xml);
        Ok(results)
    }
}

fn parse_tokyotosho_rss(xml: &str) -> Vec<TorrentSearchResult> {
    let mut results = Vec::new();
    let item_re = Regex::new(r"(?s)<item>(.*?)</item>").unwrap();
    let title_re =
        Regex::new(r"<title><!\[CDATA\[(.*?)\]\]></title>|<title>(.*?)</title>").unwrap();
    let link_re = Regex::new(r"<link>(.*?)</link>").unwrap();
    let desc_re = Regex::new(r"(?s)<description>(.*?)</description>").unwrap();
    let date_re = Regex::new(r"<pubDate>(.*?)</pubDate>").unwrap();

    let size_re = Regex::new(r"Size:\s*([0-9.]+\s*[KMGT]?B)").unwrap();

    for cap in item_re.captures_iter(xml) {
        let item_xml = match cap.get(1) {
            Some(m) => m.as_str(),
            None => continue,
        };

        let title = if let Some(t_cap) = title_re.captures(item_xml) {
            t_cap
                .get(1)
                .or_else(|| t_cap.get(2))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default()
        } else {
            continue;
        };

        if title.is_empty() {
            continue;
        }

        let link = link_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();

        let magnet_url = if link.starts_with("magnet:") {
            link.clone()
        } else {
            format!(
                "magnet:?dn={}",
                url::form_urlencoded::byte_serialize(title.as_bytes()).collect::<String>()
            )
        };

        let desc = desc_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        let size_str = size_re
            .captures(desc)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("0 B");

        let size_bytes = parse_size_to_bytes(size_str);
        let quality = detect_quality(&title);
        let (has_engsub, has_vietsub) = detect_subtitles(&title);

        let upload_date = date_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let id = format!(
            "tt-{}",
            title
                .chars()
                .filter(|c| c.is_alphanumeric())
                .take(20)
                .collect::<String>()
        );

        results.push(TorrentSearchResult {
            id,
            title,
            magnet_url,
            torrent_url: if link.starts_with("http") {
                Some(link)
            } else {
                None
            },
            source: "TokyoToshokan".to_string(),
            category: "Anime".to_string(),
            size_bytes,
            formatted_size: format_bytes(size_bytes),
            seeds: 10,
            peers: 2,
            quality,
            upload_date,
            has_engsub: has_engsub || true,
            has_vietsub,
        });
    }

    results
}

fn parse_size_to_bytes(s: &str) -> u64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return 0;
    }
    let val: f64 = parts[0].parse().unwrap_or(0.0);
    let unit = parts.get(1).map(|u| u.to_uppercase()).unwrap_or_default();

    if unit.starts_with("G") {
        (val * 1024.0 * 1024.0 * 1024.0) as u64
    } else if unit.starts_with("M") {
        (val * 1024.0 * 1024.0) as u64
    } else if unit.starts_with("K") {
        (val * 1024.0) as u64
    } else if unit.starts_with("T") {
        (val * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64
    } else {
        val as u64
    }
}
