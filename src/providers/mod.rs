pub mod allanime;
pub mod anidb;
pub mod animegg;
pub mod animevietsub;
pub mod anizone;
pub mod hianime;
pub mod kkphim;
pub mod moviebox;
pub mod niniyo;
pub mod ophim;

use crate::config::Config;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub search: bool,
    pub details: bool,
    pub episodes: bool,
    pub playback: bool,
    pub subtitles: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            search: true,
            details: true,
            episodes: true,
            playback: true,
            subtitles: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anime {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub cover_url: String,
    pub banner_url: Option<String>,
    pub language: Language,
    pub total_episodes: Option<u32>,
    pub synopsis: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub number: u32,
    #[serde(default)]
    pub aniskip_episode_number: Option<u32>,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub video_url: String,
    pub subtitles: Vec<Subtitle>,
    pub qualities: Vec<String>,
    pub headers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtitle {
    pub language: String,
    pub url: String,
    #[serde(default)]
    pub format: SubtitleFormat,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleFormat {
    Ass,
    WebVtt,
    Srt,
    #[default]
    Unknown,
}

pub async fn probe_stream(stream: &StreamInfo) -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut next_url = stream.video_url.clone();
    for depth in 0..=3 {
        let (url, content_type, body) =
            fetch_health_resource(&client, &next_url, &stream.headers).await?;
        anyhow::ensure!(
            !body.is_empty(),
            "STREAM_UNAVAILABLE: media response was empty"
        );
        anyhow::ensure!(
            !looks_like_html(&body),
            "STREAM_UNAVAILABLE: media request returned HTML"
        );
        let is_hls = content_type.contains("mpegurl")
            || url.path().to_ascii_lowercase().contains(".m3u8")
            || body.starts_with(b"#EXTM3U");
        if !is_hls {
            if content_type.contains("dash+xml") || url.path().to_ascii_lowercase().contains(".mpd")
            {
                let manifest = String::from_utf8(body)?;
                anyhow::ensure!(
                    manifest.contains("<MPD"),
                    "STREAM_UNAVAILABLE: DASH response was not a manifest"
                );
            }
            return Ok(());
        }

        anyhow::ensure!(
            body.starts_with(b"#EXTM3U"),
            "STREAM_UNAVAILABLE: HLS response was not a playlist"
        );
        anyhow::ensure!(
            depth < 3,
            "STREAM_UNAVAILABLE: HLS playlist nesting exceeded the safety limit"
        );
        let playlist = String::from_utf8(body)?;
        next_url = playlist
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .and_then(|line| url.join(line).ok())
            .context("STREAM_UNAVAILABLE: HLS playlist contained no media resource")?
            .to_string();
    }
    anyhow::bail!("STREAM_UNAVAILABLE: media probe did not resolve")
}

fn looks_like_html(body: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&body[..body.len().min(256)]).to_ascii_lowercase();
    let prefix = prefix.trim_start();
    prefix.starts_with("<!doctype html") || prefix.starts_with("<html")
}

async fn fetch_health_resource(
    client: &reqwest::Client,
    url: &str,
    headers: &std::collections::HashMap<String, String>,
) -> Result<(reqwest::Url, String, Vec<u8>)> {
    let mut request = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-65535")
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let mut response = request.send().await?;
    anyhow::ensure!(
        response.status().is_success() || response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
        "STREAM_UNAVAILABLE: media request returned HTTP {}",
        response.status()
    );
    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = 65_536usize.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    Ok((final_url, content_type, body))
}

pub fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn title_words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

pub fn title_match_score(title: &str, target: &str) -> i32 {
    let title_compact = normalize_title(title);
    let target_compact = normalize_title(target);
    if title_compact.is_empty() || target_compact.is_empty() {
        return 0;
    }
    if title_compact == target_compact {
        return 1000;
    }
    if title_compact.starts_with(&target_compact) || target_compact.starts_with(&title_compact) {
        return 760;
    }
    if title_compact.contains(&target_compact) || target_compact.contains(&title_compact) {
        return 620;
    }

    let words_for_title = title_words(title);
    let target_words = title_words(target);
    if words_for_title.is_empty() || target_words.is_empty() {
        return 0;
    }
    let overlap = target_words
        .iter()
        .filter(|word| words_for_title.contains(word))
        .count();
    let required = target_words.len().min(words_for_title.len());
    if required > 0 && overlap == required {
        return 420;
    }
    if overlap >= 2 {
        return 300 + (overlap as i32 * 20);
    }
    0
}

pub fn best_title_match(items: Vec<Anime>, title_variants: &[String]) -> Option<Anime> {
    let mut scored = items
        .into_iter()
        .map(|item| {
            let score = title_variants
                .iter()
                .map(|variant| title_match_score(&item.title, variant))
                .max()
                .unwrap_or(0);
            (score, item)
        })
        .filter(|(score, _)| *score >= 300)
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, item)| {
        (
            std::cmp::Reverse(*score),
            std::cmp::Reverse(item.total_episodes.unwrap_or_default()),
        )
    });
    scored.into_iter().map(|(_, item)| item).next()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Language {
    English,
    Vietnamese,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::English => write!(f, "EN"),
            Language::Vietnamese => write!(f, "VN"),
        }
    }
}

#[async_trait]
pub trait AnimeProvider: Send + Sync {
    fn name(&self) -> &str;
    fn language(&self) -> Language;
    fn supported_languages(&self) -> Vec<String>;
    fn website_url(&self) -> Option<&'static str> {
        None
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    async fn health_check(&self) -> Result<()> {
        let variants = vec!["One Piece".to_string(), "Đảo Hải Tặc".to_string()];
        let anime = best_title_match(self.search("One Piece").await?, &variants)
            .context("Provider health check found no matching title")?;
        let episodes = self.get_episodes(&anime.id).await?;
        let mut last_error = None;
        for episode in episodes.into_iter().rev().take(24) {
            match self.get_stream_url(&episode.id).await {
                Ok(stream) => match probe_stream(&stream).await {
                    Ok(()) => return Ok(()),
                    Err(error) => last_error = Some(error),
                },
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("Provider health check found no playable episodes")))
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>>;
    async fn get_anime_details(&self, _anime_id: &str) -> Result<Option<Anime>> {
        Ok(None)
    }
    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>>;
    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo>;
}

pub struct ProviderRegistry {
    providers: Vec<Arc<dyn AnimeProvider>>,
}

impl ProviderRegistry {
    pub fn new(config: &Config) -> Self {
        let mut providers: Vec<Arc<dyn AnimeProvider>> = Vec::new();

        // --- English Sources ---
        if config.sources.anizone {
            providers.push(Arc::new(anizone::AniZoneProvider::new()));
        }

        if config.sources.anidb {
            providers.push(Arc::new(anidb::AniDbProvider::new()));
        }

        // 1. AllAnime (Anime & Films)
        if config.sources.allanime {
            providers.push(Arc::new(allanime::AllAnimeProvider::new()));
        }

        if config.sources.moviebox {
            providers.push(Arc::new(moviebox::MovieBoxProvider::new()));
        }

        if config.sources.animegg {
            providers.push(Arc::new(animegg::AnimeGgProvider::new()));
        }

        // HiAnime remains a parseable legacy adapter, but is not registered
        // until it passes live playback certification.

        // --- Vietnamese Sources ---
        // 2. KKPhim
        if config.sources.kkphim {
            providers.push(Arc::new(kkphim::KkphimProvider::new()));
        }

        // 3. OPhim
        if config.sources.ophim {
            providers.push(Arc::new(ophim::OphimProvider::new()));
        }

        // This adapter currently resolves through the same public OPhim API.
        // Keep it opt-in until a distinct AnimeVietSub integration is certified.
        if config.sources.animevietsub {
            providers.push(Arc::new(animevietsub::AnimeVietSubProvider::new()));
        }

        if config.sources.niniyo {
            providers.push(Arc::new(niniyo::NiniyoProvider::new()));
        }

        Self { providers }
    }

    pub async fn search_all(&self, query: &str) -> Result<Vec<Anime>> {
        let mut all_results = Vec::new();

        for provider in &self.providers {
            if let Ok(mut results) = provider.search(query).await {
                all_results.append(&mut results);
            }
        }

        Ok(all_results)
    }

    pub async fn search_filtered(&self, query: &str, languages: &[Language]) -> Result<Vec<Anime>> {
        let mut all_results = Vec::new();

        for provider in &self.providers {
            // Only search providers that match the selected languages
            if languages.contains(&provider.language()) {
                if let Ok(mut results) = provider.search(query).await {
                    all_results.append(&mut results);
                }
            }
        }

        Ok(all_results)
    }

    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn AnimeProvider>> {
        self.providers.iter().find(|p| p.name() == name)
    }

    pub fn list_providers(&self) -> &[Arc<dyn AnimeProvider>] {
        &self.providers
    }
}

pub fn parse_episode_number(name: &str) -> u32 {
    let normalized = name.replace("Tập ", "").replace("Tap ", "");
    let token = normalized
        .split(|character: char| !character.is_ascii_digit())
        .find(|token| !token.is_empty())
        .unwrap_or("");
    let mut ep_num = token.parse::<u32>().unwrap_or(0);

    if ep_num == 0 && name.trim().eq_ignore_ascii_case("full") {
        ep_num = 1;
    }
    ep_num
}

#[cfg(test)]
mod tests {
    use super::{best_title_match, parse_episode_number, Anime, Language, ProviderRegistry};
    use crate::config::Config;

    #[test]
    fn episode_parser_does_not_merge_decimal_specials() {
        assert_eq!(parse_episode_number("Tập 1004.5"), 1004);
        assert_eq!(parse_episode_number("Episode 1167"), 1167);
        assert_eq!(parse_episode_number("Full"), 1);
    }

    #[test]
    fn registry_includes_certified_sources_and_omits_duplicates() {
        let mut config = Config::default();
        config.sources.moviebox = true;
        config.sources.animegg = true;
        config.sources.anizone = true;
        config.sources.hianime = true;
        config.sources.animevietsub = true;
        config.sources.animetvn = true;
        config.sources.niniyo = true;
        let registry = ProviderRegistry::new(&config);
        let names = registry
            .list_providers()
            .iter()
            .map(|provider| provider.name())
            .collect::<Vec<_>>();

        assert!(names.contains(&"MovieBox"));
        assert!(names.contains(&"AnimeGG"));
        assert!(names.contains(&"AniZone"));
        assert!(!names.contains(&"HiAnime"));
        assert!(names.contains(&"AnimeVietSub"));
        assert!(!names.contains(&"AnimeTVN"));
        assert!(names.contains(&"Niniyo"));
    }

    #[test]
    fn title_match_rejects_unrelated_results_and_prefers_complete_series() {
        let anime = |id: &str, title: &str, episodes| Anime {
            id: id.into(),
            provider: "test".into(),
            title: title.into(),
            cover_url: String::new(),
            banner_url: None,
            language: Language::English,
            total_episodes: episodes,
            synopsis: None,
        };
        let variants = vec!["One Piece".to_string()];

        assert!(best_title_match(vec![anime("x", "Naruto", Some(220))], &variants).is_none());
        assert_eq!(
            best_title_match(
                vec![
                    anime("short", "One Piece", Some(12)),
                    anime("complete", "One Piece", Some(1100)),
                ],
                &variants,
            )
            .map(|item| item.id)
            .as_deref(),
            Some("complete")
        );
    }
}
