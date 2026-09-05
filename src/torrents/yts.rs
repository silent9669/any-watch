use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::{format_bytes, TorrentCategory, TorrentSearchProvider, TorrentSearchResult};

pub struct YtsProvider {
    client: Client,
}

impl YtsProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct YtsResponse {
    data: Option<YtsData>,
}

#[derive(Debug, Deserialize)]
struct YtsData {
    movies: Option<Vec<YtsMovie>>,
}

#[derive(Debug, Deserialize)]
struct YtsMovie {
    id: Option<u64>,
    title: Option<String>,
    year: Option<u32>,
    torrents: Option<Vec<YtsTorrent>>,
}

#[derive(Debug, Deserialize)]
struct YtsTorrent {
    url: Option<String>,
    hash: Option<String>,
    quality: Option<String>,
    #[serde(rename = "type")]
    torrent_type: Option<String>,
    seeds: Option<u32>,
    peers: Option<u32>,
    size_bytes: Option<u64>,
    date_uploaded: Option<String>,
}

#[async_trait]
impl TorrentSearchProvider for YtsProvider {
    fn name(&self) -> &'static str {
        "YTS"
    }

    fn supported_categories(&self) -> &'static [TorrentCategory] {
        &[TorrentCategory::All, TorrentCategory::Movies]
    }

    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        if category == TorrentCategory::Anime
            || category == TorrentCategory::Tv
            || category == TorrentCategory::Documentaries
        {
            return Ok(Vec::new());
        }

        const MIRRORS: &[&str] = &[
            "https://yts.do/api/v2/list_movies.json",
            "https://yts.bz/api/v2/list_movies.json",
            "https://yts.mx/api/v2/list_movies.json",
        ];

        let mut body: Option<YtsResponse> = None;
        for base_mirror in MIRRORS {
            let Ok(mut url) = Url::parse(base_mirror) else {
                continue;
            };
            url.query_pairs_mut()
                .append_pair("query_term", query)
                .append_pair("sort_by", "seeds")
                .append_pair("order_by", "desc")
                .append_pair("limit", "50")
                .append_pair("page", &page.max(1).to_string());

            if let Ok(resp) = self
                .client
                .get(url)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
                )
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(parsed) = resp.json::<YtsResponse>().await {
                        body = Some(parsed);
                        break;
                    }
                }
            }
        }

        let body = match body {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };

        let mut results = Vec::new();
        let movies = match body.data.and_then(|d| d.movies) {
            Some(m) => m,
            None => return Ok(results),
        };

        for movie in movies {
            let movie_title = match movie.title {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };
            let year_str = movie.year.map(|y| format!(" ({})", y)).unwrap_or_default();
            let torrents = match movie.torrents {
                Some(t) => t,
                None => continue,
            };

            for torrent in torrents {
                let hash = match torrent.hash {
                    Some(h) if !h.is_empty() => h,
                    _ => continue,
                };
                let quality = torrent.quality.unwrap_or_else(|| "1080p".to_string());
                let t_type = torrent.torrent_type.unwrap_or_default();
                let full_title =
                    format!("{}{} [{} {}] [YTS]", movie_title, year_str, quality, t_type);

                let magnet_url = format!(
                    "magnet:?xt=urn:btih:{}&dn={}&tr=udp://open.demonii.com:1337/announce&tr=udp://tracker.openbittorrent.com:80&tr=udp://tracker.coppersurfer.tk:6969&tr=udp://glotorrents.pw:6969/announce&tr=udp://tracker.opentrackr.org:1337/announce",
                    hash,
                    url::form_urlencoded::byte_serialize(full_title.as_bytes()).collect::<String>()
                );

                let size_bytes = torrent.size_bytes.unwrap_or(0);
                let seeds = torrent.seeds.unwrap_or(0);
                let peers = torrent.peers.unwrap_or(0);
                let id = format!("yts-{}-{}", movie.id.unwrap_or(0), hash);

                results.push(TorrentSearchResult {
                    id,
                    title: full_title,
                    magnet_url,
                    torrent_url: torrent.url,
                    source: "YTS".to_string(),
                    category: "Movies".to_string(),
                    size_bytes,
                    formatted_size: format_bytes(size_bytes),
                    seeds,
                    peers,
                    quality: Some(quality),
                    upload_date: torrent.date_uploaded,
                    has_engsub: false,
                    has_vietsub: false,
                });
            }
        }

        Ok(results)
    }
}
