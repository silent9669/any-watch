use any_watch_core::config::Config;
use any_watch_core::providers::{
    normalize_title, probe_stream, AnimeProvider, Language, ProviderRegistry,
};
use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let require_english = std::env::args().any(|argument| argument == "--require-english");
    let certify_all = std::env::args().any(|argument| argument == "--all");
    let mut config = Config::default();
    if certify_all {
        config.sources.anizone = true;
        config.sources.allanime = true;
        config.sources.animegg = true;
        config.sources.moviebox = true;
        config.sources.ophim = true;
    }
    let registry = ProviderRegistry::new(&config);
    let mut healthy_english = 0usize;
    let mut failures = Vec::new();

    for provider in registry.list_providers() {
        if !provider.capabilities().playback {
            println!("SKIP {}: playback is not certified", provider.name());
            continue;
        }
        match certify(provider.as_ref()).await {
            Ok(()) => {
                println!("PASS {} ({})", provider.name(), provider.language());
                if provider.language() == Language::English {
                    healthy_english += 1;
                }
            }
            Err(error) => {
                println!("FAIL {}: {error:#}", provider.name());
                failures.push(provider.name().to_string());
            }
        }
    }

    if require_english && healthy_english == 0 {
        anyhow::bail!(
            "release blocked: no English provider passed live playback certification; failures: {}",
            failures.join(", ")
        );
    }
    if !failures.is_empty() && !certify_all {
        anyhow::bail!(
            "release blocked: enabled providers failed live playback certification: {}",
            failures.join(", ")
        );
    }
    Ok(())
}

async fn certify(provider: &dyn AnimeProvider) -> Result<()> {
    let queries = match (provider.language(), provider.name()) {
        (Language::English, "MovieBox") => &["One Piece", "Your Name"][..],
        (Language::Vietnamese, "Niniyo") => &["Solo Leveling", "Attack on Titan"][..],
        (Language::English, _) | (Language::Vietnamese, _) => &["One Piece"][..],
        (Language::Youtube, _) => &["YouTube"][..],
    };
    for query in queries {
        certify_query(provider, query)
            .await
            .with_context(|| format!("{query} certification failed"))?;
    }
    Ok(())
}

async fn certify_query(provider: &dyn AnimeProvider, query: &str) -> Result<()> {
    let mut results = Vec::new();
    for candidate_query in query_aliases(query) {
        let mut found = provider
            .search(candidate_query)
            .await
            .with_context(|| format!("search failed for {candidate_query}"))?;
        results.append(&mut found);
    }
    dedupe_results(&mut results);
    let aliases = query_aliases(query)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let anime = results
        .into_iter()
        .find(|anime| exact_title_match(&anime.title, &aliases))
        .with_context(|| format!("no exact canonical match for {query}"))?;
    certify_anime(provider, &anime).await.with_context(|| {
        format!(
            "{} [{}] did not produce playable media",
            anime.title, anime.id
        )
    })
}

fn query_aliases(query: &str) -> Vec<&str> {
    match query {
        "One Piece" => vec!["One Piece", "Đảo Hải Tặc"],
        "Attack on Titan" => vec!["Attack on Titan", "Đại Chiến Titan"],
        "Solo Leveling" => vec!["Solo Leveling", "Thăng Cấp Một Mình"],
        "Your Name" => vec!["Your Name", "Kimi no Na wa"],
        "Kimi no Na wa" => vec!["Kimi no Na wa", "Your Name"],
        _ => vec![query],
    }
}

fn dedupe_results(results: &mut Vec<any_watch_core::providers::Anime>) {
    let mut seen = std::collections::HashSet::new();
    results.retain(|anime| seen.insert(format!("{}:{}", anime.provider, anime.id)));
}

fn exact_title_match(title: &str, variants: &[String]) -> bool {
    let title = normalize_title(title);
    variants
        .iter()
        .any(|variant| normalize_title(variant) == title)
}

async fn certify_anime(
    provider: &dyn AnimeProvider,
    anime: &any_watch_core::providers::Anime,
) -> Result<()> {
    let episodes = provider
        .get_episodes(&anime.id)
        .await
        .context("episode listing failed")?;
    anyhow::ensure!(!episodes.is_empty(), "episode listing returned no episodes");

    let candidate_episodes = if episodes.len() > 24 {
        let mut selected = Vec::new();
        // Check latest episodes first, then earlier archive episodes if recent CDN has transient issues
        selected.extend(episodes.iter().rev().take(12).cloned());
        selected.extend(episodes.iter().take(6).cloned());
        selected.extend(episodes.iter().skip(episodes.len() / 2).take(6).cloned());
        selected
    } else {
        episodes.into_iter().rev().collect()
    };

    let mut last_error = None;
    for episode in candidate_episodes {
        let stream = match provider.get_stream_url(&episode.id).await {
            Ok(stream) => stream,
            Err(error) => {
                last_error = Some(error.context("stream resolution failed"));
                continue;
            }
        };
        let stream_host = url::Url::parse(&stream.video_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_else(|| "unknown-host".to_string());
        println!(
            "  {} stream: {} [{}] episode {} -> {}",
            provider.name(),
            anime.title,
            anime.id,
            episode.number,
            stream_host
        );

        match probe_stream(&stream).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error.context("resolved media was not playable"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no recent episode produced a stream")))
}
