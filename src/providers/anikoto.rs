use super::{
    probe_stream, Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo,
    Subtitle,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{self, HeaderMap};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

const ANIKOTO_BASE: &str = "https://anikototv.to";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub struct AnikotoProvider {
    client: reqwest::Client,
}

impl Default for AnikotoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AnikotoProvider {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );
        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://anikototv.to/"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();

        Self { client }
    }

    async fn extract_embed_sources(&self, embed_url: &str) -> Result<(String, Vec<Subtitle>)> {
        let page_html = self
            .client
            .get(embed_url)
            .header("Referer", "https://hianimes.re/")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await?
            .text()
            .await?;

        let id_regex = Regex::new(r#"data-id="([^"]*)""#)?;
        let file_id = id_regex
            .captures(&page_html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .context("Failed to find data-id in embed page")?;

        let origin = if let Ok(parsed) = reqwest::Url::parse(embed_url) {
            format!(
                "{}://{}",
                parsed.scheme(),
                parsed.host_str().unwrap_or_default()
            )
        } else {
            "https://megaplay.buzz".to_string()
        };

        let sources_url = format!(
            "{}/stream/getSources?id={file_id}&id={file_id}",
            origin.trim_end_matches('/')
        );
        let json_resp: Value = self
            .client
            .get(&sources_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", format!("{}/", origin.trim_end_matches('/')))
            .send()
            .await?
            .json()
            .await?;

        let hls_file = json_resp["sources"]["file"]
            .as_str()
            .or_else(|| json_resp["sources"][0]["file"].as_str())
            .context("Stream file URL not found in getSources response")?
            .to_string();

        let mut subtitles = Vec::new();
        if let Some(tracks) = json_resp["tracks"].as_array() {
            for track in tracks {
                if let (Some(file), Some(label)) = (track["file"].as_str(), track["label"].as_str())
                {
                    subtitles.push(Subtitle {
                        language: label.to_string(),
                        url: file.to_string(),
                        format: super::SubtitleFormat::WebVtt,
                    });
                }
            }
        }

        Ok((hls_file, subtitles))
    }
    fn parse_items_from_html(&self, html: &str) -> Vec<Anime> {
        let slug_regex =
            Regex::new(r#"href="https://anikototv\.to/watch/([^"/]+)(?:/ep-\d+)?"#).ok();
        let img_regex = Regex::new(r#"<img\s+src="([^"]+)""#).ok();
        let title_regex = Regex::new(r#"class="name d-title"[^>]*>([\s\S]*?)</a>"#).ok();
        let ep_regex = Regex::new(r#"class="ep-status (?:total|sub)"><span>\s*(\d+)</span>"#).ok();

        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for chunk in html.split("<div class=\"item") {
            let Some(ref slug_re) = slug_regex else {
                continue;
            };
            let Some(slug_cap) = slug_re.captures(chunk) else {
                continue;
            };
            let slug = slug_cap[1].to_string();
            if slug.is_empty() || seen.contains(&slug) {
                continue;
            }

            let mut title = title_regex
                .as_ref()
                .and_then(|re| re.captures(chunk))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| slug.replace('-', " "));

            title = title
                .replace("&quot;", "\"")
                .replace("&#039;", "'")
                .replace("&amp;", "&");
            if let Some(pos) = title.find('<') {
                title = title[..pos].trim().to_string();
            }

            let cover_url = img_regex
                .as_ref()
                .and_then(|re| re.captures(chunk))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "https://anikototv.to/images/logo.png".to_string());

            let total_episodes = ep_regex
                .as_ref()
                .and_then(|re| re.captures(chunk))
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().trim().parse::<u32>().ok());

            seen.insert(slug.clone());
            results.push(Anime {
                id: slug,
                provider: "Anikoto".to_string(),
                title,
                cover_url,
                banner_url: None,
                language: Language::English,
                total_episodes,
                synopsis: None,
            });
        }

        results
    }
}

#[async_trait]
impl AnimeProvider for AnikotoProvider {
    fn name(&self) -> &str {
        "Anikoto"
    }

    fn language(&self) -> Language {
        Language::English
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["en".to_string(), "ja".to_string()]
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
        let search_url = format!("{}/filter?keyword={}", ANIKOTO_BASE, url_encode(query));
        let html = self.client.get(&search_url).send().await?.text().await?;

        let items = self.parse_items_from_html(&html);
        if !items.is_empty() {
            return Ok(items);
        }

        // Fallback card scraper if layout is different
        let fallback_regex = Regex::new(
            r#"<a\s+href="https://anikototv\.to/watch/([^"/]+)(?:/ep-\d+)?"[^>]*>([\s\S]*?)</a>"#,
        )?;
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for cap in fallback_regex.captures_iter(&html) {
            let slug = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if !slug.is_empty() && !seen.contains(&slug) {
                seen.insert(slug.clone());
                results.push(Anime {
                    id: slug.clone(),
                    provider: "Anikoto".to_string(),
                    title: slug.replace('-', " "),
                    cover_url: "https://anikototv.to/images/logo.png".to_string(),
                    banner_url: None,
                    language: Language::English,
                    total_episodes: None,
                    synopsis: None,
                });
            }
        }

        Ok(results)
    }

    async fn catalog(&self) -> Result<Vec<Anime>> {
        let filter_url = format!("{}/filter?sort=trending", ANIKOTO_BASE);
        if let Ok(resp) = self.client.get(&filter_url).send().await {
            if resp.status().is_success() {
                if let Ok(html) = resp.text().await {
                    let items = self.parse_items_from_html(&html);
                    if !items.is_empty() {
                        return Ok(items);
                    }
                }
            }
        }

        let home_url = format!("{}/home", ANIKOTO_BASE);
        if let Ok(resp) = self.client.get(&home_url).send().await {
            if resp.status().is_success() {
                if let Ok(html) = resp.text().await {
                    let items = self.parse_items_from_html(&html);
                    if !items.is_empty() {
                        return Ok(items);
                    }
                }
            }
        }

        self.search("One Piece").await
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let watch_url = format!("{}/watch/{}", ANIKOTO_BASE, anime_id);
        let html = self.client.get(&watch_url).send().await?.text().await?;

        let title_regex = Regex::new(r#"<h2[^>]*class="[^"]*title[^"]*"[^>]*>([\s\S]*?)</h2>"#)?;
        let title = title_regex
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| {
                m.as_str()
                    .replace("&quot;", "\"")
                    .replace("&#039;", "'")
                    .replace("&amp;", "&")
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| anime_id.replace('-', " "));

        let poster_regex = Regex::new(r#"class="poster"[^>]*src="([^"]+)""#)?;
        let cover_url = poster_regex
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        let desc_regex = Regex::new(r#"class="description"[^>]*>([\s\S]*?)</div>"#)?;
        let synopsis = desc_regex
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().replace("<br>", "\n").trim().to_string());

        Ok(Some(Anime {
            id: anime_id.to_string(),
            provider: "Anikoto".to_string(),
            title,
            cover_url,
            banner_url: None,
            language: Language::English,
            total_episodes: None,
            synopsis,
        }))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let watch_url = format!("{}/watch/{}", ANIKOTO_BASE, anime_id);
        let watch_html = self.client.get(&watch_url).send().await?.text().await?;

        let id_regex = Regex::new(r#"data-id="(\d+)""#)?;
        let show_id = id_regex
            .captures(&watch_html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .context("Could not find show ID for anime")?;

        let list_url = format!("{}/ajax/episode/list/{}", ANIKOTO_BASE, show_id);
        let resp: Value = self
            .client
            .get(&list_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", &watch_url)
            .send()
            .await?
            .json()
            .await?;

        let html = resp["result"].as_str().unwrap_or_default();
        let a_regex = Regex::new(r#"<a\s+[^>]*data-num="(\d+)"[^>]*data-ids="([^"]*)"[^>]*>"#)?;

        let mut episodes = Vec::new();
        for cap in a_regex.captures_iter(html) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            let server_ids = cap[2].to_string();
            if num > 0 {
                episodes.push(Episode {
                    id: format!("{}:{}:{}", anime_id, num, server_ids),
                    number: num,
                    aniskip_episode_number: Some(num),
                    title: Some(format!("Episode {num}")),
                    thumbnail: None,
                });
            }
        }

        episodes.sort_by_key(|e| e.number);
        episodes.dedup_by(|a, b| a.number == b.number);
        Ok(episodes)
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        let parts: Vec<&str> = episode_id.split(':').collect();
        anyhow::ensure!(parts.len() >= 3, "Invalid Anikoto episode id format");
        let server_ids = parts[2];

        let servers_url = format!(
            "{}/ajax/server/list?servers={}",
            ANIKOTO_BASE,
            url_encode(server_ids)
        );
        let server_resp: Value = self
            .client
            .get(&servers_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", format!("{}/", ANIKOTO_BASE))
            .send()
            .await?
            .json()
            .await?;

        let html = server_resp["result"].as_str().unwrap_or_default();
        let link_regex = Regex::new(r#"data-link-id="([^"]*)""#)?;

        let mut stream_candidates = Vec::new();
        for cap in link_regex.captures_iter(html) {
            stream_candidates.push(cap[1].to_string());
        }

        anyhow::ensure!(
            !stream_candidates.is_empty(),
            "No server links found for this episode"
        );

        let mut last_error = None;
        for link_id in stream_candidates {
            let get_server_url =
                format!("{}/ajax/server?get={}", ANIKOTO_BASE, url_encode(&link_id));
            let resolved: Value = match self
                .client
                .get(&get_server_url)
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", format!("{}/", ANIKOTO_BASE))
                .send()
                .await
            {
                Ok(r) => match r.json().await {
                    Ok(j) => j,
                    Err(e) => {
                        last_error = Some(e.into());
                        continue;
                    }
                },
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };

            if let Some(embed_url) = resolved["result"]["url"].as_str() {
                if let Ok((video_url, subtitles)) = self.extract_embed_sources(embed_url).await {
                    let mut headers = HashMap::new();
                    headers.insert("Referer".to_string(), "https://megaplay.buzz/".to_string());
                    headers.insert("User-Agent".to_string(), USER_AGENT.to_string());

                    return Ok(StreamInfo {
                        video_url,
                        subtitles,
                        qualities: vec!["auto".to_string()],
                        headers,
                        use_curl: false,
                    });
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("Failed to resolve stream for Anikoto episode")))
    }

    async fn health_check(&self) -> Result<()> {
        let results = self.search("Frieren").await?;
        anyhow::ensure!(!results.is_empty(), "Anikoto search returned 0 results");
        let episodes = self.get_episodes(&results[0].id).await?;
        anyhow::ensure!(!episodes.is_empty(), "Anikoto episodes returned empty");
        let stream = self.get_stream_url(&episodes[0].id).await?;
        probe_stream(&stream).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AnikotoProvider;

    #[test]
    fn parses_anikoto_items_with_posters_and_episodes() {
        let sample_html = r#"
            <div class="item">
              <div class="ani poster" data-tip="1701">
                <a href="https://anikototv.to/watch/kabaneri-of-the-iron-fortress-8t72y/ep-12">
                  <img src="https://cdn.anipixcdn.co/thumbnail/test.jpg" alt="Kabaneri of the Iron Fortress" />
                  <div class="meta"><div class="inner"><div class="left">
                    <span class="ep-status total"><span>12</span></span>
                  </div></div></div>
                </a>
              </div>
              <div class="info">
                <a class="name d-title" href="https://anikototv.to/watch/kabaneri-of-the-iron-fortress-8t72y/ep-12">
                  Kabaneri of the Iron Fortress
                </a>
              </div>
            </div>
        "#;
        let provider = AnikotoProvider::new();
        let items = provider.parse_items_from_html(sample_html);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "kabaneri-of-the-iron-fortress-8t72y");
        assert_eq!(items[0].title, "Kabaneri of the Iron Fortress");
        assert_eq!(
            items[0].cover_url,
            "https://cdn.anipixcdn.co/thumbnail/test.jpg"
        );
        assert_eq!(items[0].total_episodes, Some(12));
    }
}
