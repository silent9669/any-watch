use anyhow::Result;
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

    /// Search across all matching providers concurrently with a 6-second timeout per provider
    pub async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        source_filter: Option<&str>,
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
                    && !filter.eq_ignore_ascii_case(name)
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
                let search_fut = provider_clone.search(&query_owned, category);
                match timeout(Duration::from_secs(6), search_fut).await {
                    Ok(Ok(results)) => {
                        info!(
                            "Provider {} returned {} results",
                            provider_clone.name(),
                            results.len()
                        );
                        results
                    }
                    Ok(Err(e)) => {
                        warn!("Provider {} error: {:#}", provider_clone.name(), e);
                        Vec::new()
                    }
                    Err(_) => {
                        warn!("Provider {} timed out", provider_clone.name());
                        Vec::new()
                    }
                }
            }));
        }

        let mut all_results = Vec::new();
        for task in tasks {
            if let Ok(res) = task.await {
                all_results.extend(res);
            }
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

        Ok(all_results)
    }
}

fn extract_info_hash(magnet: &str) -> Option<String> {
    if let Some(idx) = magnet.find("xt=urn:btih:") {
        let after = &magnet[idx + 12..];
        let hash: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !hash.is_empty() {
            return Some(hash.to_lowercase());
        }
    }
    None
}
