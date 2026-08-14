use super::{
    Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo, Subtitle,
    SubtitleFormat,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{REFERER, USER_AGENT};
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;
use url::Url;

const BASE_URL: &str = "https://anizone.to";
const USER_AGENT_VALUE: &str = "any-watch/1.0";
const OUTPUT_MARKER: &str = "\n__ANY_WATCH__%{http_code}";

pub struct AniZoneProvider;

impl Default for AniZoneProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AniZoneProvider {
    pub fn new() -> Self {
        Self
    }

    async fn html(&self, url: Url, operation: &str) -> Result<String> {
        let mut last_error = None;
        for _ in 0..2 {
            let output = match Command::new("curl")
                .args([
                    "-sS",
                    "-L",
                    "--max-redirs",
                    "8",
                    "--max-time",
                    "60",
                    "-A",
                    USER_AGENT_VALUE,
                    "-w",
                    OUTPUT_MARKER,
                    url.as_str(),
                ])
                .output()
                .await
            {
                Ok(output) => output,
                Err(error) => {
                    last_error = Some(
                        anyhow::Error::new(error)
                            .context("PROVIDER_UNAVAILABLE: AniZone requires the curl binary"),
                    );
                    continue;
                }
            };
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let (body, status) = stdout
                .rsplit_once("__ANY_WATCH__")
                .unwrap_or((stdout.as_str(), "000"));
            let status = status.trim().parse::<u16>().unwrap_or(0);
            if status == 0 {
                last_error = Some(anyhow::anyhow!(
                    "PROVIDER_UNAVAILABLE: {operation} request failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
            anyhow::ensure!(
                (200..300).contains(&status),
                "PROVIDER_UNAVAILABLE: {operation} returned HTTP {status}"
            );
            anyhow::ensure!(
                !body.contains("Just a moment") && !body.contains("cf-chl-"),
                "PROVIDER_CAPTCHA: AniZone returned a browser challenge"
            );
            return Ok(body.to_string());
        }
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("{operation} request failed after retries")))
    }

    fn parse_search(html: &str) -> Result<Vec<Anime>> {
        let payload =
            Regex::new(r#"(?s)items:\s*JSON\.parse\('(?P<payload>.*?)'\),\s*nextCursor:"#)?
                .captures(html)
                .and_then(|capture| capture.name("payload"))
                .context("AniZone search returned no result payload")?;
        let javascript_string: String = serde_json::from_str(&format!("\"{}\"", payload.as_str()))
            .context("AniZone search returned an invalid result payload")?;
        let items: Vec<Value> = serde_json::from_str(&javascript_string)
            .context("AniZone search returned invalid result JSON")?;
        Ok(items
            .into_iter()
            .filter_map(|item| {
                let id = item["slug"].as_str()?.to_string();
                let title = item["main_title"].as_str()?.to_string();
                Some(Anime {
                    id,
                    provider: "AniZone".into(),
                    title: decode_html(&title),
                    cover_url: item["cover"].as_str().unwrap_or_default().to_string(),
                    banner_url: None,
                    language: Language::English,
                    total_episodes: item["episode_count"]
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok()),
                    synopsis: None,
                })
            })
            .collect())
    }

    fn parse_details(html: &str, anime_id: &str) -> Result<Anime> {
        let title = Regex::new(r"(?s)<title>(?P<title>.*?)\s+—\s+AniZone</title>")?
            .captures(html)
            .map(|capture| decode_html(&capture["title"]))
            .context("AniZone details returned no title")?;
        let images = Regex::new(r#"https://anizone\.to/images/anime/[A-Za-z0-9-]+\.jpg"#)?
            .find_iter(html)
            .map(|value| value.as_str().to_string())
            .collect::<Vec<_>>();
        let total_episodes = Self::regular_episode_count(html).ok();
        Ok(Anime {
            id: anime_id.to_string(),
            provider: "AniZone".into(),
            title,
            cover_url: images
                .get(1)
                .or_else(|| images.first())
                .cloned()
                .unwrap_or_default(),
            banner_url: images.first().cloned(),
            language: Language::English,
            total_episodes,
            synopsis: None,
        })
    }

    fn regular_episode_count(html: &str) -> Result<u32> {
        let pattern = Regex::new(
            r#"Showing\s*<span[^>]*>[0-9,]+</span>\s*to\s*<span[^>]*>[0-9,]+</span>\s*of\s*<span[^>]*>(?P<count>[0-9,]+)</span>\s*results"#,
        )?;
        pattern
            .captures(html)
            .and_then(|capture| capture.name("count"))
            .and_then(|value| value.as_str().replace(',', "").parse().ok())
            .context("AniZone details returned no regular episode count")
    }

    fn parse_stream(html: &str) -> Result<StreamInfo> {
        let player_payload =
            Regex::new(r#"(?s)vidstackPlayer\(JSON\.parse\('(?P<payload>.*?)'\)\)"#)?
                .captures(html)
                .and_then(|capture| capture.name("payload"))
                .map(|payload| decode_player_payload(payload.as_str()))
                .transpose()?;

        let video_url = player_payload
            .as_ref()
            .and_then(|payload| payload["src"].as_str())
            .map(str::to_string)
            .or_else(|| {
                Regex::new(r#"<media-player[^>]+src="(?P<url>https?://[^"]+)""#)
                    .ok()?
                    .captures(html)
                    .map(|capture| capture["url"].to_string())
            })
            .context("STREAM_NOT_FOUND: AniZone returned no HLS player")?;

        let mut subtitles = if let Some(payload) = player_payload {
            payload["subtitles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|track| {
                    let language = track["title"].as_str()?.to_string();
                    if !language.to_ascii_lowercase().starts_with("english") {
                        return None;
                    }
                    let format = match track["format"].as_str().unwrap_or_default() {
                        "ass" => SubtitleFormat::Ass,
                        "vtt" | "webvtt" => SubtitleFormat::WebVtt,
                        "srt" => SubtitleFormat::Srt,
                        _ => SubtitleFormat::Unknown,
                    };
                    Some(Subtitle {
                        language,
                        url: track["file"].as_str()?.to_string(),
                        format,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            let track = Regex::new(
                r#"<track\s+src=(?:"(?P<quoted>https?://[^"]+)"|(?P<plain>https?://[^\s>]+))[^>]*data-type="ass"[^>]*label="(?P<label>[^"]+)""#,
            )?;
            track
                .captures_iter(html)
                .filter_map(|capture| {
                    let language = decode_html(&capture["label"]);
                    if !language.to_ascii_lowercase().starts_with("english") {
                        return None;
                    }
                    let url = capture
                        .name("quoted")
                        .or_else(|| capture.name("plain"))?
                        .as_str()
                        .to_string();
                    Some(Subtitle {
                        language,
                        url,
                        format: SubtitleFormat::Ass,
                    })
                })
                .collect::<Vec<_>>()
        };
        subtitles.sort_by_key(|track| {
            let label = track.language.to_ascii_lowercase();
            if label.contains("full subtitles") {
                0
            } else if label == "english" {
                1
            } else {
                2
            }
        });
        anyhow::ensure!(
            !subtitles.is_empty(),
            "STREAM_NOT_FOUND: AniZone returned no English subtitle track"
        );
        let mut headers = HashMap::new();
        headers.insert(REFERER.as_str().to_string(), format!("{BASE_URL}/"));
        headers.insert(
            USER_AGENT.as_str().to_string(),
            USER_AGENT_VALUE.to_string(),
        );
        Ok(StreamInfo {
            video_url,
            subtitles,
            qualities: vec!["Auto".into()],
            headers,
            use_curl: true,
        })
    }

    fn anime_url(anime_id: &str) -> Result<Url> {
        anyhow::ensure!(
            !anime_id.is_empty()
                && anime_id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || character == '-'
                        || character == '_'),
            "AniZone anime ID is invalid"
        );
        Url::parse(&format!("{BASE_URL}/anime/{anime_id}")).map_err(Into::into)
    }

    fn episode_url(episode_id: &str) -> Result<Url> {
        let (anime_id, episode) = episode_id
            .split_once('/')
            .context("AniZone episode ID is invalid")?;
        Self::anime_url(anime_id)?;
        anyhow::ensure!(
            !episode.is_empty() && episode.chars().all(|character| character.is_ascii_digit()),
            "AniZone episode ID is invalid"
        );
        Url::parse(&format!("{BASE_URL}/anime/{anime_id}/{episode}")).map_err(Into::into)
    }
}

#[async_trait]
impl AnimeProvider for AniZoneProvider {
    fn name(&self) -> &str {
        "AniZone"
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
        ProviderCapabilities::default()
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let mut url = Url::parse(&format!("{BASE_URL}/anime"))?;
        url.query_pairs_mut().append_pair("search", query);
        Self::parse_search(&self.html(url, "AniZone search").await?)
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let url = Self::anime_url(anime_id)?;
        Ok(Some(Self::parse_details(
            &self.html(url, "AniZone details").await?,
            anime_id,
        )?))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let url = Self::anime_url(anime_id)?;
        let html = self.html(url, "AniZone episodes").await?;
        let count = Self::regular_episode_count(&html)?;
        Ok((1..=count)
            .map(|number| Episode {
                id: format!("{anime_id}/{number}"),
                number,
                aniskip_episode_number: Some(number),
                title: None,
                thumbnail: None,
            })
            .collect())
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        let url = Self::episode_url(episode_id)?;
        Self::parse_stream(&self.html(url, "AniZone playback").await?)
    }

    async fn health_check(&self) -> Result<()> {
        let anime =
            super::best_title_match(self.search("One Piece").await?, &["One Piece".to_string()])
                .context("AniZone health check found no matching title")?;
        let episodes = self.get_episodes(&anime.id).await?;
        let mut last_error = None;
        for episode in episodes.into_iter().rev().take(12) {
            match self.get_stream_url(&episode.id).await {
                Ok(stream) => match super::probe_stream(&stream).await {
                    Ok(()) => return Ok(()),
                    Err(error) => last_error = Some(error),
                },
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("AniZone health check found no playable episode")))
    }
}

fn decode_player_payload(payload: &str) -> Result<serde_json::Value> {
    let javascript_string: String = serde_json::from_str(&format!("\"{payload}\""))
        .context("AniZone returned an invalid player payload")?;
    serde_json::from_str(&javascript_string).context("AniZone returned invalid player JSON")
}

fn decode_html(value: &str) -> String {
    value
        .replace("&amp;", "&")
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
    fn parses_search_episode_count_and_ass_playback() {
        let search = r#"items: JSON.parse('[{\u0022slug\u0022:\u0022uyyyn4kf\u0022,\u0022url\u0022:\u0022http:\\/\\/anizone.to\\/anime\\/uyyyn4kf\u0022,\u0022cover\u0022:\u0022https:\\/\\/anizone.to\\/images\\/anime\\/cover.jpg\u0022,\u0022main_title\u0022:\u0022One Piece\u0022,\u0022episode_count\u0022:1176}]'), nextCursor: null"#;
        let results = AniZoneProvider::parse_search(search).unwrap();
        assert_eq!(results[0].id, "uyyyn4kf");
        assert_eq!(results[0].title, "One Piece");
        assert_eq!(results[0].total_episodes, Some(1176));

        let details = r#"Showing <span>1</span> to <span>36</span> of <span>1,173</span> results"#;
        assert_eq!(
            AniZoneProvider::regular_episode_count(details).unwrap(),
            1173
        );

        let playback = r#"<media-player src="https://cdn.example/master.m3u8"><track src=https://cdn.example/subtitles/signs_en.ass data-type="ass" kind="subtitles" label="English - Signs/Songs" srclang="en" /><track src=https://cdn.example/subtitles/full_en.ass data-type="ass" kind="subtitles" label="English - Full Subtitles" srclang="en" default />"#;
        let stream = AniZoneProvider::parse_stream(playback).unwrap();
        assert_eq!(stream.video_url, "https://cdn.example/master.m3u8");
        assert_eq!(stream.subtitles[0].format, SubtitleFormat::Ass);
        assert_eq!(stream.subtitles[0].language, "English - Full Subtitles");
        assert!(stream.use_curl);

        let encoded = r#"<div x-data="vidstackPlayer(JSON.parse('{\u0022src\u0022:\u0022https:\\/\\/cdn.example\\/master.m3u8\u0022,\u0022subtitles\u0022:[{\u0022title\u0022:\u0022English\u0022,\u0022format\u0022:\u0022ass\u0022,\u0022file\u0022:\u0022https:\\/\\/cdn.example\\/sub.ass\u0022}]}'))"></div>"#;
        let stream = AniZoneProvider::parse_stream(encoded).unwrap();
        assert_eq!(stream.video_url, "https://cdn.example/master.m3u8");
        assert_eq!(stream.subtitles[0].url, "https://cdn.example/sub.ass");
    }
}
