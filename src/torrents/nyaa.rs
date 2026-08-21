use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use url::Url;

use super::{
    detect_quality, detect_subtitles, format_bytes, TorrentCategory, TorrentSearchProvider,
    TorrentSearchResult,
};

pub struct NyaaProvider {
    client: Client,
}

impl NyaaProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TorrentSearchProvider for NyaaProvider {
    fn name(&self) -> &'static str {
        "Nyaa"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[TorrentCategory::All, TorrentCategory::Anime]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        if category == TorrentCategory::Movies || category == TorrentCategory::Tv {
            return Ok(Vec::new());
        }

        let mut url = Url::parse("https://nyaa.si/?page=rss")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("c", "1_2") // Anime - English-translated
            .append_pair("f", "0")
            .append_pair("s", "seeders")
            .append_pair("o", "desc")
            .append_pair("p", &page.max(1).to_string());

        let response = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await
            .context("Nyaa request failed")?;

        anyhow::ensure!(
            response.status().is_success(),
            "Nyaa returned HTTP {}",
            response.status()
        );

        let xml = response
            .text()
            .await
            .context("Nyaa returned invalid RSS text")?;
        let results = parse_nyaa_rss(&xml);
        Ok(results)
    }
}

fn parse_nyaa_rss(xml: &str) -> Vec<TorrentSearchResult> {
    let mut results = Vec::new();

    let item_re = Regex::new(r"(?s)<item>(.*?)</item>").unwrap();
    let title_re =
        Regex::new(r"<title><!\[CDATA\[(.*?)\]\]></title>|<title>(.*?)</title>").unwrap();
    let link_re = Regex::new(r"<link>(.*?)</link>").unwrap();
    let size_re = Regex::new(r"<nyaa:size>(.*?)</nyaa:size>").unwrap();
    let seeders_re = Regex::new(r"<nyaa:seeders>(.*?)</nyaa:seeders>").unwrap();
    let leechers_re = Regex::new(r"<nyaa:leechers>(.*?)</nyaa:leechers>").unwrap();
    let info_hash_re = Regex::new(r"<nyaa:infoHash>(.*?)</nyaa:infoHash>").unwrap();
    let pub_date_re = Regex::new(r"<pubDate>(.*?)</pubDate>").unwrap();

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

        let info_hash = info_hash_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());
        let torrent_link = link_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let magnet_url = if let Some(ref hash) = info_hash {
            format!(
                "magnet:?xt=urn:btih:{}&dn={}",
                hash,
                urlencoding::encode(&title)
            )
        } else if let Some(ref link) = torrent_link {
            link.clone()
        } else {
            continue;
        };

        let size_str = size_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim())
            .unwrap_or("0 B");
        let size_bytes = parse_size_to_bytes(size_str);

        let seeds: u32 = seeders_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().trim().parse().ok())
            .unwrap_or(0);
        let peers: u32 = leechers_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().trim().parse().ok())
            .unwrap_or(0);
        let upload_date = pub_date_re
            .captures(item_xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let quality = detect_quality(&title);
        let (_has_engsub, has_vietsub) = detect_subtitles(&title);

        let id = format!(
            "nyaa-{}",
            info_hash.unwrap_or_else(|| title
                .chars()
                .filter(|c| c.is_alphanumeric())
                .take(16)
                .collect())
        );

        results.push(TorrentSearchResult {
            id,
            title,
            magnet_url,
            torrent_url: torrent_link,
            source: "Nyaa".to_string(),
            category: "Anime".to_string(),
            size_bytes,
            formatted_size: format_bytes(size_bytes),
            seeds,
            peers,
            quality,
            upload_date,
            has_engsub: true, // English-translated category
            has_vietsub,
        });
    }

    results
}

fn parse_size_to_bytes(s: &str) -> u64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 2 {
        return 0;
    }
    let val: f64 = parts[0].parse().unwrap_or(0.0);
    let unit = parts[1].to_uppercase();

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

mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}
