use anyhow::{bail, Result};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

use super::animetosho::AnimeToshoProvider;
use super::nyaa::NyaaProvider;
use super::piratebay::ThePirateBayProvider;
use super::yts::YtsProvider;
use super::{TorrentCategory, TorrentSearchProvider, TorrentSearchResult};

pub struct TorrentSearchHub {
    providers: Vec<Arc<dyn TorrentSearchProvider>>,
}

impl Default for TorrentSearchHub {
    fn default() -> Self {
        Self::new()
    }
}

impl TorrentSearchHub {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let providers: Vec<Arc<dyn TorrentSearchProvider>> = vec![
            Arc::new(AnimeToshoProvider::new(client.clone())),
            Arc::new(NyaaProvider::new(client.clone())),
            Arc::new(YtsProvider::new(client.clone())),
            Arc::new(ThePirateBayProvider::new(client)),
        ];

        Self { providers }
    }

    pub fn available_providers(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }

    /// Search across all matching providers concurrently with a 15-second timeout per provider
    pub async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        source_filter: Option<&str>,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let mut tasks = Vec::new();

        for provider in &self.providers {
            let name = provider.name();

            if let Some(filter) = source_filter {
                if !filter.is_empty()
                    && !filter.eq_ignore_ascii_case("all")
                    && !source_matches(filter, name)
                {
                    continue;
                }
            }

            if !provider
                .supported_categories()
                .contains(&TorrentCategory::All)
                && !provider.supported_categories().contains(&category)
                && category != TorrentCategory::All
            {
                continue;
            }

            let provider_clone = Arc::clone(provider);
            let query_owned = trimmed.to_string();

            tasks.push(tokio::spawn(async move {
                let search_fut = provider_clone.search(&query_owned, category, page.max(1));
                match timeout(Duration::from_secs(15), search_fut).await {
                    Ok(Ok(results)) => {
                        info!(
                            "Provider {} returned {} results",
                            provider_clone.name(),
                            results.len()
                        );
                        Ok(results)
                    }
                    Ok(Err(error)) => {
                        warn!("Provider {} error: {:#}", provider_clone.name(), error);
                        Err(provider_clone.name())
                    }
                    Err(_) => {
                        warn!("Provider {} timed out", provider_clone.name());
                        Err(provider_clone.name())
                    }
                }
            }));
        }

        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let mut all_results = Vec::new();
        let mut successful_providers = 0_usize;
        let mut failed_providers = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(results)) => {
                    successful_providers += 1;
                    all_results.extend(results);
                }
                Ok(Err(provider)) => failed_providers.push(provider),
                Err(error) => warn!(%error, "torrent provider task failed"),
            }
        }
        if successful_providers == 0 {
            bail!(
                "All selected torrent indexers failed: {}",
                failed_providers.join(", ")
            );
        }

        // Sort by seeders descending, then size
        all_results.sort_by(|a, b| {
            b.seeds
                .cmp(&a.seeds)
                .then_with(|| b.size_bytes.cmp(&a.size_bytes))
        });

        // Deduplicate similar magnet info_hashes
        let mut seen_hashes = std::collections::HashSet::new();
        all_results.retain(|r| {
            if let Some(hash) = extract_info_hash(&r.magnet_url) {
                seen_hashes.insert(hash)
            } else {
                true
            }
        });
        all_results.truncate(200);

        Ok(all_results)
    }
}

fn source_matches(filter: &str, provider_name: &str) -> bool {
    let normalize = |value: &str| {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let filter = normalize(filter);
    let provider = normalize(provider_name);
    filter == provider
        || (provider == "thepiratebay" && matches!(filter.as_str(), "piratebay" | "tpb"))
}

fn extract_info_hash(magnet: &str) -> Option<String> {
    let url = url::Url::parse(magnet).ok()?;
    if url.scheme() != "magnet" {
        return None;
    }
    url.query_pairs().find_map(|(key, value)| {
        if !key.eq_ignore_ascii_case("xt") {
            return None;
        }
        let value = value.as_ref();
        let lower = value.to_ascii_lowercase();
        let hash = lower.strip_prefix("urn:btih:")?;
        (!hash.is_empty()
            && hash
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
        .then(|| hash.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_info_hash, source_matches};

    #[test]
    fn source_filter_accepts_display_aliases() {
        assert!(source_matches("piratebay", "ThePirateBay"));
        assert!(source_matches("TPB", "ThePirateBay"));
        assert!(source_matches("anime-tosho", "AnimeTosho"));
        assert!(!source_matches("Nyaa", "YTS"));
    }

    #[test]
    fn magnet_hash_parser_handles_case_and_encoding() {
        let expected = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            extract_info_hash(&format!(
                "magnet:?dn=Film&XT=URN%3ABTIH%3A{}",
                expected.to_uppercase()
            ))
            .as_deref(),
            Some(expected)
        );
    }
}
