use any_watch_core::providers::{
    allanime::AllAnimeProvider, anidb::AniDbProvider, animegg::AnimeGgProvider,
    animevietsub::AnimeVietSubProvider, anizone::AniZoneProvider, kkphim::KkphimProvider,
    moviebox::MovieBoxProvider, niniyo::NiniyoProvider, normalize_title, ophim::OphimProvider,
    AnimeProvider, StreamInfo,
};
use anyhow::{Context, Result};
use reqwest::{header, Client, Url};
use std::time::Duration;

async fn assert_live_playback(provider: &dyn AnimeProvider, query: &str) -> Result<()> {
    let anime_results = provider.search(query).await?;
    let mut last_error = None;
    let aliases = query_aliases(query);

    for anime in anime_results
        .into_iter()
        .filter(|anime| {
            aliases
                .iter()
                .any(|alias| normalize_title(alias) == normalize_title(&anime.title))
        })
        .take(5)
    {
        let episodes = match provider.get_episodes(&anime.id).await {
            Ok(episodes) => episodes,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };

        for episode in episodes.into_iter().rev().take(24) {
            match provider.get_stream_url(&episode.id).await {
                Ok(stream) => {
                    match probe_stream(&stream).await.with_context(|| {
                        format!(
                            "{} resolved {} episode {}, but its media was not playable",
                            provider.name(),
                            anime.title,
                            episode.number
                        )
                    }) {
                        Ok(()) => {
                            eprintln!(
                                "{} playback passed: {} episode {}",
                                provider.name(),
                                anime.title,
                                episode.number
                            );
                            return Ok(());
                        }
                        Err(error) => last_error = Some(error),
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("{} returned no playable episodes", provider.name())))
}

fn query_aliases(query: &str) -> Vec<&str> {
    match query {
        "One Piece" | "Đảo Hải Tặc" => vec!["One Piece", "Đảo Hải Tặc"],
        "Solo Leveling" => vec!["Solo Leveling", "Thăng Cấp Một Mình"],
        _ => vec![query],
    }
}

async fn probe_stream(stream: &StreamInfo) -> Result<()> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(20))
        .build()?;
    let (url, content_type, body) =
        fetch_media(&client, &stream.video_url, &stream.headers).await?;

    if content_type.contains("mpegurl")
        || url.path().to_ascii_lowercase().contains(".m3u8")
        || body.starts_with(b"#EXTM3U")
    {
        let playlist = String::from_utf8(body)?;
        let media_url = first_playlist_resource(&url, &playlist)
            .context("HLS playlist contained no media playlist or segment")?;
        let (media_url, media_type, media_body) =
            fetch_media(&client, media_url.as_str(), &stream.headers).await?;

        if media_type.contains("mpegurl")
            || media_url.path().to_ascii_lowercase().contains(".m3u8")
            || media_body.starts_with(b"#EXTM3U")
        {
            let media_playlist = String::from_utf8(media_body)?;
            let segment_url = first_playlist_resource(&media_url, &media_playlist)
                .context("HLS media playlist contained no segment")?;
            let (_, _, segment) =
                fetch_media(&client, segment_url.as_str(), &stream.headers).await?;
            anyhow::ensure!(!segment.is_empty(), "HLS segment was empty");
        } else {
            anyhow::ensure!(!media_body.is_empty(), "HLS resource was empty");
        }
    } else if content_type.contains("dash+xml") || url.path().to_ascii_lowercase().contains(".mpd")
    {
        let manifest = String::from_utf8(body)?;
        anyhow::ensure!(manifest.contains("<MPD"), "DASH manifest was invalid");
        let lowercase = manifest.to_ascii_lowercase();
        anyhow::ensure!(
            !lowercase.contains("codecs=\"hev1") && !lowercase.contains("codecs=\"hvc1"),
            "DASH manifest advertises HEVC-only video, which is not browser-safe"
        );
    } else {
        anyhow::ensure!(!body.is_empty(), "media response was empty");
    }

    Ok(())
}

async fn fetch_media(
    client: &Client,
    url: &str,
    headers: &std::collections::HashMap<String, String>,
) -> Result<(Url, String, Vec<u8>)> {
    let mut request = client
        .get(url)
        .header(header::RANGE, "bytes=0-262143")
        .header(header::ACCEPT_ENCODING, "identity");
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let mut response = request.send().await?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT,
        "media request returned HTTP {status}"
    );
    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = 262_144usize.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    Ok((final_url, content_type, body))
}

fn first_playlist_resource(base: &Url, playlist: &str) -> Option<Url> {
    playlist
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| base.join(line).ok())
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_allanime_live_health() -> Result<()> {
    AllAnimeProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_animegg_live_health() -> Result<()> {
    AnimeGgProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_anidb_live_health() -> Result<()> {
    AniDbProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_anizone_live_health() -> Result<()> {
    AniZoneProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_kkphim_live_health() -> Result<()> {
    KkphimProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_ophim_live_health() -> Result<()> {
    OphimProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_moviebox_live_health() -> Result<()> {
    MovieBoxProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_niniyo_live_health() -> Result<()> {
    NiniyoProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_animevietsub_live_health() -> Result<()> {
    AnimeVietSubProvider::new().health_check().await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_allanime_live_playback() -> Result<()> {
    assert_live_playback(&AllAnimeProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_animegg_live_playback() -> Result<()> {
    assert_live_playback(&AnimeGgProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_anidb_live_playback() -> Result<()> {
    assert_live_playback(&AniDbProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_anizone_live_playback() -> Result<()> {
    assert_live_playback(&AniZoneProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_kkphim_live_playback() -> Result<()> {
    assert_live_playback(&KkphimProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_ophim_live_playback() -> Result<()> {
    assert_live_playback(&OphimProvider::new(), "Đảo Hải Tặc").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_moviebox_live_playback() -> Result<()> {
    assert_live_playback(&MovieBoxProvider::new(), "One Piece").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_niniyo_live_playback() -> Result<()> {
    assert_live_playback(&NiniyoProvider::new(), "Solo Leveling").await
}

#[tokio::test]
#[ignore = "requires live provider network access"]
async fn test_animevietsub_live_playback() -> Result<()> {
    assert_live_playback(&AnimeVietSubProvider::new(), "Đảo Hải Tặc").await
}
