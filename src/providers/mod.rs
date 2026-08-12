pub mod allanime;
pub mod animegg;
pub mod animevietsub;
pub mod hianime;
pub mod kkphim;
pub mod moviebox;
pub mod niniyo;
pub mod ophim;

use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
pub struct Episode {
    pub id: String,
    pub number: u32,
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
        if self.search("One Piece").await?.is_empty() {
            anyhow::bail!("Provider returned no health-check results");
        }
        Ok(())
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
    use super::{parse_episode_number, ProviderRegistry};
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
        assert!(!names.contains(&"HiAnime"));
        assert!(names.contains(&"AnimeVietSub"));
        assert!(!names.contains(&"AnimeTVN"));
        assert!(names.contains(&"Niniyo"));
    }
}
