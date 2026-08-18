use super::{Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;
use url::Url;

const BASE_URL: &str = "https://anidb.app";
const USER_AGENT_VALUE: &str = "any-watch/1.0";
const OUTPUT_MARKER: &str = "\n__ANY_WATCH__%{http_code}__%{content_type}";

pub struct AniDbProvider;

impl Default for AniDbProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AniDbProvider {
    pub fn new() -> Self {
        Self
    }

    async fn curl_fetch(
        &self,
        url: &Url,
        referer: Option<&str>,
        range: Option<&str>,
    ) -> Result<(u16, String, String)> {
        let mut args: Vec<String> = vec![
            "-sS".into(),
            "-L".into(),
            "--max-redirs".into(),
            "8".into(),
            "--max-time".into(),
            "30".into(),
            "-A".into(),
            USER_AGENT_VALUE.into(),
        ];
        if let Some(referer) = referer {
            args.push("-e".into());
            args.push(referer.to_string());
        }
        if let Some(range) = range {
            args.push("-H".into());
            args.push(format!("Range: {range}"));
        }
        args.push("-w".into());
        args.push(OUTPUT_MARKER.into());
        args.push(url.as_str().into());
        let output = Command::new("curl")
            .args(&args)
            .output()
            .await
            .with_context(|| "PROVIDER_UNAVAILABLE: AniDB requires the curl binary")?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let (body, meta) = stdout
            .rsplit_once("__ANY_WATCH__")
            .unwrap_or((stdout.as_str(), "000__"));
        let mut fields = meta.split("__");
        let status = fields
            .next()
            .unwrap_or("000")
            .trim()
            .parse::<u16>()
            .unwrap_or(0);
        let content_type = fields.next().unwrap_or_default().to_string();
        anyhow::ensure!(
            status != 0,
            "PROVIDER_UNAVAILABLE: AniDB request failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok((status, content_type, body.to_string()))
    }

    async fn text(&self, url: Url, operation: &str) -> Result<String> {
        let (status, _, body) = self.curl_fetch(&url, None, None).await?;
        anyhow::ensure!(
            status == 200,
            "PROVIDER_UNAVAILABLE: {operation} returned HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        );
        anyhow::ensure!(
            !body.contains("Just a moment") && !body.contains("cf-chl-"),
            "PROVIDER_CAPTCHA: AniDB returned a browser challenge"
        );
        Ok(body)
    }

    async fn json(&self, url: Url, operation: &str) -> Result<Value> {
        let body = self.text(url, operation).await?;
        serde_json::from_str(&body).with_context(|| format!("{operation} returned invalid JSON"))
    }

    fn parse_search(html: &str) -> Result<Vec<Anime>> {
        let pattern = Regex::new(
            r#"(?s)<a\s+href="https://anidb\.app/anime/(?P<id>[a-z0-9-]+-[0-9]+)"[^>]*title="(?P<title>[^"]+)"[^>]*>.*?<img\s+src="(?P<cover>[^"]+)""#,
        )?;
        Ok(pattern
            .captures_iter(html)
            .map(|capture| Anime {
                id: capture["id"].to_string(),
                provider: "AniDB".into(),
                title: decode_html(&capture["title"]),
                cover_url: capture["cover"].to_string(),
                banner_url: None,
                language: Language::English,
                total_episodes: None,
                synopsis: None,
            })
            .collect())
    }

    fn parse_details(html: &str, anime_id: &str) -> Result<Anime> {
        let meta = |property: &str| -> Option<String> {
            Regex::new(&format!(
                r#"<meta\s+(?:property|name)="{}"\s+content="(?P<value>[^"]*)""#,
                regex::escape(property)
            ))
            .ok()?
            .captures(html)
            .map(|capture| decode_html(&capture["value"]))
        };
        let title = meta("og:title")
            .map(|title| title.trim_end_matches(" — AniDB").to_string())
            .context("AniDB details returned no title")?;
        Ok(Anime {
            id: anime_id.to_string(),
            provider: "AniDB".into(),
            title,
            cover_url: meta("og:image").unwrap_or_default(),
            banner_url: None,
            language: Language::English,
            total_episodes: None,
            synopsis: meta("og:description"),
        })
    }

    fn numeric_anime_id(anime_id: &str) -> Result<&str> {
        let id = anime_id
            .rsplit_once('-')
            .map(|(_, id)| id)
            .context("AniDB anime ID is invalid")?;
        anyhow::ensure!(
            !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()),
            "AniDB anime ID is invalid"
        );
        Ok(id)
    }

    fn parse_episodes(value: &Value) -> Vec<Episode> {
        let values = value["episodes"]
            .as_array()
            .or_else(|| value.as_array())
            .into_iter()
            .flatten();
        let mut episodes = values
            .filter_map(|episode| {
                let number = episode["number"]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())?;
                if number == 0 || episode["number2"].as_f64().is_some() {
                    return None;
                }
                Some(Episode {
                    id: episode["id"].as_u64()?.to_string(),
                    number,
                    aniskip_episode_number: Some(number),
                    title: None,
                    thumbnail: None,
                })
            })
            .collect::<Vec<_>>();
        episodes.sort_by_key(|episode| episode.number);
        episodes.dedup_by_key(|episode| episode.number);
        episodes
    }

    fn parse_embed_url(value: &Value) -> Result<String> {
        value["languages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|language| language["code"].as_str() == Some("jpn"))
            .and_then(|language| language["embed_url"].as_str())
            .map(str::to_string)
            .context("STREAM_NOT_FOUND: AniDB returned no Japanese-audio source")
    }

    fn parse_stream(html: &str, embed_url: &str) -> Result<StreamInfo> {
        let video_url = Regex::new(r#"file:\s*'(?P<url>https?://[^']+\.m3u8[^']*)'"#)?
            .captures(html)
            .map(|capture| capture["url"].to_string())
            .context("STREAM_NOT_FOUND: AniDB embed returned no HLS stream")?;
        let mut headers = HashMap::new();
        headers.insert("referer".to_string(), embed_url.to_string());
        headers.insert("user-agent".to_string(), USER_AGENT_VALUE.to_string());
        Ok(StreamInfo {
            video_url,
            subtitles: Vec::new(),
            qualities: vec!["Auto".into()],
            headers,
            use_curl: true,
        })
    }

    fn anime_url(anime_id: &str) -> Result<Url> {
        Self::numeric_anime_id(anime_id)?;
        Url::parse(&format!("{BASE_URL}/anime/{anime_id}")).map_err(Into::into)
    }

    async fn probe(&self, stream: &StreamInfo) -> Result<()> {
        let mut next_url = Url::parse(&stream.video_url)?;
        let referer = stream.headers.get("referer").map(String::as_str);
        for depth in 0..3 {
            let (status, content_type, body) = self
                .curl_fetch(&next_url, referer, Some("bytes=0-65535"))
                .await?;
            anyhow::ensure!(
                status == 200 || status == 206,
                "STREAM_FORBIDDEN: media request returned HTTP {status}"
            );
            anyhow::ensure!(
                !body.is_empty(),
                "STREAM_UNAVAILABLE: media response was empty"
            );
            anyhow::ensure!(
                !body.trim_start().starts_with('<'),
                "STREAM_UNAVAILABLE: media request returned HTML"
            );
            let is_hls = content_type.contains("mpegurl")
                || next_url.path().to_ascii_lowercase().contains(".m3u8")
                || body.starts_with("#EXTM3U");
            if !is_hls {
                return Ok(());
            }
            anyhow::ensure!(
                body.starts_with("#EXTM3U"),
                "STREAM_UNAVAILABLE: HLS response was not a playlist"
            );
            anyhow::ensure!(
                depth < 2,
                "STREAM_UNAVAILABLE: HLS playlist nesting exceeded the safety limit"
            );
            next_url = body
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .and_then(|line| next_url.join(line).ok())
                .context("STREAM_UNAVAILABLE: HLS playlist contained no media resource")?;
        }
        anyhow::bail!("STREAM_UNAVAILABLE: media probe did not resolve")
    }
}

#[async_trait]
impl AnimeProvider for AniDbProvider {
    fn name(&self) -> &str {
        "AniDB"
    }

    fn language(&self) -> Language {
        Language::English
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["en".into()]
    }

    fn website_url(&self) -> Option<&'static str> {
        Some(BASE_URL)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            subtitles: false,
            ..ProviderCapabilities::default()
        }
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let mut url = Url::parse(&format!("{BASE_URL}/browse"))?;
        url.query_pairs_mut().append_pair("q", query);
        Self::parse_search(&self.text(url, "AniDB search").await?)
    }

    async fn catalog(&self) -> Result<Vec<Anime>> {
        let url = Url::parse(&format!("{BASE_URL}/browse"))?;
        Self::parse_search(&self.text(url, "AniDB catalog").await?)
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let html = self
            .text(Self::anime_url(anime_id)?, "AniDB details")
            .await?;
        Ok(Some(Self::parse_details(&html, anime_id)?))
    }

    async fn health_check(&self) -> Result<()> {
        let variants = vec!["One Piece".to_string(), "Đảo Hải Tặc".to_string()];
        let anime = super::best_title_match(self.search("One Piece").await?, &variants)
            .context("AniDB health check found no matching title")?;
        let episodes = self.get_episodes(&anime.id).await?;
        let mut last_error = None;
        for episode in episodes.into_iter().rev().take(24) {
            match self.get_stream_url(&episode.id).await {
                Ok(stream) => match self.probe(&stream).await {
                    Ok(()) => return Ok(()),
                    Err(error) => last_error = Some(error),
                },
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("AniDB health check found no playable episodes")))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let id = Self::numeric_anime_id(anime_id)?;
        let value = self
            .json(
                Url::parse(&format!("{BASE_URL}/api/frontend/anime/{id}/episodes"))?,
                "AniDB episodes",
            )
            .await?;
        let episodes = Self::parse_episodes(&value);
        anyhow::ensure!(
            !episodes.is_empty(),
            "PROVIDER_UNAVAILABLE: AniDB returned no regular episodes"
        );
        Ok(episodes)
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        anyhow::ensure!(
            !episode_id.is_empty()
                && episode_id
                    .chars()
                    .all(|character| character.is_ascii_digit()),
            "AniDB episode ID is invalid"
        );
        let value = self
            .json(
                Url::parse(&format!(
                    "{BASE_URL}/api/frontend/episode/{episode_id}/languages"
                ))?,
                "AniDB languages",
            )
            .await?;
        let embed_url = Self::parse_embed_url(&value)?;
        let html = self.text(Url::parse(&embed_url)?, "AniDB embed").await?;
        Self::parse_stream(&html, &embed_url)
    }
}

fn decode_html(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_regular_episodes_and_hls_embed() {
        let search = r#"<a href="https://anidb.app/anime/one-piece-3880" class="anime-card" title="One Piece"><img src="https://cdn.example/3880.jpg" alt="One Piece"></a>"#;
        let results = AniDbProvider::parse_search(search).unwrap();
        assert_eq!(results[0].id, "one-piece-3880");
        assert_eq!(results[0].title, "One Piece");

        let episodes = AniDbProvider::parse_episodes(&serde_json::json!({
            "episodes": [
                { "id": 10, "number": 1, "number2": null },
                { "id": 11, "number": 1, "number2": 0.5 },
                { "id": 12, "number": 2, "number2": null }
            ]
        }));
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[1].id, "12");
        assert_eq!(episodes[1].aniskip_episode_number, Some(2));

        let stream = AniDbProvider::parse_stream(
            "sources: [{ file: 'https://hls.example/master.m3u8', type: 'hls' }]",
            "https://anidb.app/embed/token",
        )
        .unwrap();
        assert_eq!(stream.video_url, "https://hls.example/master.m3u8");
        assert_eq!(stream.headers["referer"], "https://anidb.app/embed/token");
        assert!(stream.use_curl);
    }
}
