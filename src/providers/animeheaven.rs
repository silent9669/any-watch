use super::{
    probe_stream, Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{self, HeaderMap};
use std::collections::HashMap;
use std::time::Duration;

const ANIMEHEAVEN_BASE: &str = "https://animeheaven.me";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

pub struct AnimeHeavenProvider {
    client: reqwest::Client,
}

impl Default for AnimeHeavenProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimeHeavenProvider {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );
        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://animeheaven.me/"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();

        Self { client }
    }
}

#[async_trait]
impl AnimeProvider for AnimeHeavenProvider {
    fn name(&self) -> &str {
        "AnimeHeaven"
    }

    fn language(&self) -> Language {
        Language::English
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["en".to_string()]
    }

    fn website_url(&self) -> Option<&'static str> {
        Some("https://animeheaven.me")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            search: true,
            details: true,
            episodes: true,
            playback: true,
            subtitles: false,
        }
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let search_url = format!(
            "{}/fastsearch.php?xhr=1&s={}",
            ANIMEHEAVEN_BASE,
            url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
        );
        let html = self.client.get(&search_url).send().await?.text().await?;

        let search_regex = Regex::new(
            r#"<a[^>]*href=['"]/anime\.php\?([a-z0-9]+)['"][^>]*>[\s\S]*?<img[^>]*src=['"]([^'"]*)['"][^>]*alt=['"]([^'"]*)['"]"#,
        )?;

        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for cap in search_regex.captures_iter(&html) {
            let id = cap[1].to_string();
            let raw_img = &cap[2];
            let raw_title = &cap[3];

            if id.is_empty() || seen.contains(&id) {
                continue;
            }

            let title = raw_title
                .replace("&quot;", "\"")
                .replace("&#039;", "'")
                .replace("&amp;", "&")
                .trim()
                .to_string();

            let cover_url = if raw_img.starts_with("http") {
                raw_img.to_string()
            } else if raw_img.starts_with('/') {
                format!("{ANIMEHEAVEN_BASE}{raw_img}")
            } else {
                format!("{ANIMEHEAVEN_BASE}/{raw_img}")
            };

            seen.insert(id.clone());
            results.push(Anime {
                id,
                provider: "AnimeHeaven".to_string(),
                title,
                cover_url,
                banner_url: None,
                language: Language::English,
                total_episodes: None,
                synopsis: None,
            });
        }

        Ok(results)
    }

    async fn catalog(&self) -> Result<Vec<Anime>> {
        self.search("One Piece").await
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let url = format!("{}/anime.php?{}", ANIMEHEAVEN_BASE, anime_id);
        let html = self.client.get(&url).send().await?.text().await?;

        let title_regex =
            Regex::new(r#"<title>([^<]+?)(?:\s*Anime\s*\|\s*AnimeHeaven\.Me)?</title>"#)?;
        let title = title_regex
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| {
                m.as_str()
                    .replace("Anime | AnimeHeaven.Me", "")
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| anime_id.to_string());

        let img_regex = Regex::new(r#"class=['"]coverimg['"][^>]*src=['"]([^'"]+)['"]"#)?;
        let cover_url = img_regex
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| {
                let s = m.as_str();
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("{ANIMEHEAVEN_BASE}/{}", s.trim_start_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{ANIMEHEAVEN_BASE}/images/logo.png"));

        Ok(Some(Anime {
            id: anime_id.to_string(),
            provider: "AnimeHeaven".to_string(),
            title,
            cover_url,
            banner_url: None,
            language: Language::English,
            total_episodes: None,
            synopsis: None,
        }))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let url = format!("{}/anime.php?{}", ANIMEHEAVEN_BASE, anime_id);
        let html = self.client.get(&url).send().await?.text().await?;

        let ep_regex = Regex::new(
            r#"gateh\(['"]([a-f0-9]{32})['"]\)[\s\S]*?class=['"]\s*watch2\s*bc\s*['"]>(\d+)</div>"#,
        )?;

        let mut episodes = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for cap in ep_regex.captures_iter(&html) {
            let key = cap[1].to_string();
            let num: u32 = cap[2].parse().unwrap_or(0);

            if num > 0 && !seen.contains(&key) {
                seen.insert(key.clone());
                episodes.push(Episode {
                    id: key,
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
        let gate_url = format!("{}/gate.php", ANIMEHEAVEN_BASE);
        let resp = self
            .client
            .get(&gate_url)
            .header(header::COOKIE, format!("key={episode_id}"))
            .header(header::REFERER, format!("{}/", ANIMEHEAVEN_BASE))
            .send()
            .await?
            .text()
            .await?;

        let src_regex =
            Regex::new(r#"<source\s+src=['"](https?://[^'"]+)['"]\s+type=['"]video/mp4['"]"#)?;

        let mut chosen_url = None;
        for cap in src_regex.captures_iter(&resp) {
            let candidate = cap[1].to_string();
            if !candidate.contains("&error") {
                chosen_url = Some(candidate);
                break;
            }
            if chosen_url.is_none() {
                chosen_url = Some(candidate);
            }
        }

        let video_url = chosen_url.context("No playable MP4 source found on AnimeHeaven gate")?;

        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), format!("{}/", ANIMEHEAVEN_BASE));
        headers.insert("User-Agent".to_string(), USER_AGENT.to_string());

        Ok(StreamInfo {
            video_url,
            subtitles: Vec::new(),
            qualities: vec!["1080p".to_string(), "auto".to_string()],
            headers,
            use_curl: false,
        })
    }

    async fn health_check(&self) -> Result<()> {
        let results = self.search("One Piece").await?;
        anyhow::ensure!(!results.is_empty(), "AnimeHeaven search returned 0 results");
        let episodes = self.get_episodes(&results[0].id).await?;
        anyhow::ensure!(!episodes.is_empty(), "AnimeHeaven episodes returned 0");
        let stream = self.get_stream_url(&episodes[0].id).await?;
        probe_stream(&stream).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AnimeHeavenProvider;
    use regex::Regex;

    #[test]
    fn parses_animeheaven_search_fixture() {
        let sample = r#"
            <a class='ac' href='/anime.php?ckqsc'><div class='fastitem bc1 ac'><div class='fastimg'><img class='coverimg' src='/image.php?giul6' alt='One Piece Heroines'></div><div class='fastname'>One Piece Heroines</div></div></a>
            <a class='ac' href='/anime.php?1ht8d'><div class='fastitem bc1 ac'><div class='fastimg'><img class='coverimg' src='/image.php?zbt7y' alt='One Piece'></div><div class='fastname'>One Piece</div></div></a>
        "#;
        let _provider = AnimeHeavenProvider::new();
        let results = Regex::new(
            r#"<a[^>]*href=['"]/anime\.php\?([a-z0-9]+)['"][^>]*>[\s\S]*?<img[^>]*src=['"]([^'"]*)['"][^>]*alt=['"]([^'"]*)['"]"#,
        )
        .unwrap()
        .captures_iter(sample)
        .map(|cap| cap[1].to_string())
        .collect::<Vec<_>>();
        assert_eq!(results, vec!["ckqsc", "1ht8d"]);
    }
}
