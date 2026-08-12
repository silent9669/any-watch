#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod download;
mod proxy;

use ani_desk_core::catalog::{
    apply_personal_matches, CatalogAnime, CatalogClient, CatalogFilters, CatalogPage,
    DiscoveryCatalog, TastePreference,
};
use ani_desk_core::config::Config;
use ani_desk_core::db::{Database, DownloadRecord, WatchHistory};
use ani_desk_core::metadata::MetadataCache;
use ani_desk_core::player::Player;
use ani_desk_core::providers::{
    best_title_match, normalize_title, Anime, Episode, Language, ProviderCapabilities,
    ProviderRegistry, StreamInfo, Subtitle,
};
use ani_desk_core::skip_times::{fetch_skip_times, SkipTime};
use anyhow::Context;
use chrono::Utc;
use download::{DownloadEvent, DownloadRequest, DownloadResult};
use proxy::ProxyState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use uuid::Uuid;

struct AppState {
    db: Arc<Database>,
    providers: ProviderRegistry,
    proxy: ProxyState,
    metadata: MetadataCache,
    catalog: CatalogClient,
    provider_health: RwLock<HashMap<String, ProviderHealthDto>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    name: String,
    language: String,
    language_group: String,
    status: String,
    failure_code: Option<String>,
    capabilities: ProviderCapabilities,
    website_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderHealthDto {
    name: String,
    language: String,
    language_group: String,
    status: String,
    failure_code: Option<String>,
    checked_at: Option<String>,
    capabilities: ProviderCapabilities,
    website_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderAvailabilityDto {
    provider: String,
    language: String,
    status: String,
    failure_code: Option<String>,
    anime: Option<AnimeDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppErrorDto {
    code: String,
    message: String,
    provider: Option<String>,
    operation: String,
    retryable: bool,
    correlation_id: String,
    technical: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnimeDto {
    id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
    banner_url: Option<String>,
    language: String,
    total_episodes: Option<u32>,
    synopsis: Option<String>,
    is_favorite: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnimeDetailsDto {
    cover_url: Option<String>,
    banner_url: Option<String>,
    total_episodes: Option<u32>,
    synopsis: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeDto {
    id: String,
    number: u32,
    title: Option<String>,
    thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchHistoryDto {
    anime_id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
    episode_number: u32,
    episode_title: Option<String>,
    position_seconds: u64,
    total_seconds: u64,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteDto {
    anime_id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRecordDto {
    id: String,
    provider: String,
    anime_id: String,
    anime_title: String,
    cover_url: String,
    episode_id: String,
    episode_number: u32,
    episode_title: Option<String>,
    file_path: String,
    file_name: String,
    bytes_downloaded: u64,
    media_kind: String,
    completed_at: String,
    file_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackDto {
    session_id: String,
    playback_url: String,
    stream_kind: String,
    subtitles: Vec<SubtitleDto>,
    qualities: Vec<String>,
    can_fallback_to_mpv: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleDto {
    language: String,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimeInput {
    id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressInput {
    anime_id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
    episode_number: u32,
    episode_title: Option<String>,
    position_seconds: u64,
    total_seconds: u64,
}

#[tauri::command]
async fn list_sources(state: State<'_, AppState>) -> Result<Vec<SourceDto>, AppErrorDto> {
    let health = state.provider_health.read().await.clone();
    Ok(state
        .providers
        .list_providers()
        .iter()
        .map(|provider| {
            let current = health.get(provider.name());
            SourceDto {
                name: provider.name().to_string(),
                language: language_label(provider.language()).to_string(),
                language_group: language_group(provider.language()).to_string(),
                status: current
                    .map(|item| item.status.clone())
                    .unwrap_or_else(|| "unknown".into()),
                failure_code: current.and_then(|item| item.failure_code.clone()),
                capabilities: provider.capabilities(),
                website_url: provider.website_url().map(str::to_string),
            }
        })
        .collect())
}

#[tauri::command]
async fn list_provider_health(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderHealthDto>, AppErrorDto> {
    let health = ensure_provider_health(&state, None).await;
    Ok(provider_health_in_registry_order(&state, &health))
}

#[tauri::command]
async fn retry_provider_health(
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Result<Vec<ProviderHealthDto>, AppErrorDto> {
    let health = refresh_provider_health(&state, provider.as_deref()).await;
    Ok(provider_health_in_registry_order(&state, &health))
}

fn provider_health_in_registry_order(
    state: &AppState,
    health: &HashMap<String, ProviderHealthDto>,
) -> Vec<ProviderHealthDto> {
    state
        .providers
        .list_providers()
        .iter()
        .filter_map(|provider| health.get(provider.name()).cloned())
        .collect()
}

#[tauri::command]
async fn get_discovery(state: State<'_, AppState>) -> Result<DiscoveryCatalog, AppErrorDto> {
    let mut discovery = state
        .catalog
        .discovery()
        .await
        .map_err(|error| app_error("CATALOG_UNAVAILABLE", "discovery", None, error, true))?;
    personalize_items(&state, &mut discovery.trending).await;
    personalize_items(&state, &mut discovery.popular_this_season).await;
    Ok(discovery)
}

#[tauri::command]
async fn get_genre_catalog(
    state: State<'_, AppState>,
    genre: String,
) -> Result<Vec<CatalogAnime>, AppErrorDto> {
    let mut items = state
        .catalog
        .by_genre(genre.trim(), 18)
        .await
        .map_err(|error| app_error("CATALOG_UNAVAILABLE", "genre", None, error, true))?;
    personalize_items(&state, &mut items).await;
    Ok(items)
}

#[tauri::command]
async fn search_catalog(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<CatalogAnime>, AppErrorDto> {
    let query = query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }
    let mut items = state
        .catalog
        .search(query, 24)
        .await
        .map_err(|error| app_error("CATALOG_UNAVAILABLE", "search", None, error, true))?;
    personalize_items(&state, &mut items).await;
    Ok(items)
}

#[tauri::command]
async fn get_catalog(
    state: State<'_, AppState>,
    filters: CatalogFilters,
    sort: String,
    page: u32,
) -> Result<CatalogPage, AppErrorDto> {
    let mut result = state
        .catalog
        .catalog(&filters, &sort, page, 24)
        .await
        .map_err(|error| app_error("CATALOG_UNAVAILABLE", "catalog", None, error, true))?;
    personalize_items(&state, &mut result.items).await;
    if sort == "personalMatch" {
        result
            .items
            .sort_by_key(|item| std::cmp::Reverse(item.personal_match.unwrap_or(0)));
    }
    Ok(result)
}

#[tauri::command]
async fn resolve_availability(
    state: State<'_, AppState>,
    catalog_id: i64,
    title: String,
    language_group_filter: Option<String>,
) -> Result<Vec<ProviderAvailabilityDto>, AppErrorDto> {
    let health = ensure_provider_health(&state, None).await;
    let title_variants = catalog_title_variants(&state, catalog_id, &title).await;
    let mut availability = Vec::new();
    let mut tasks = tokio::task::JoinSet::new();
    let mut provider_order = Vec::new();
    for provider in state.providers.list_providers() {
        let group = language_group(provider.language());
        if language_group_filter
            .as_deref()
            .is_some_and(|filter| !filter.eq_ignore_ascii_case(group))
        {
            continue;
        }
        provider_order.push(provider.name().to_string());
        let current = health.get(provider.name());
        if current.is_some_and(|item| item.status == "unavailable") {
            availability.push(ProviderAvailabilityDto {
                provider: provider.name().into(),
                language: language_label(provider.language()).into(),
                status: "unavailable".into(),
                failure_code: current.and_then(|item| item.failure_code.clone()),
                anime: None,
            });
            continue;
        }

        let provider = provider.clone();
        let title = title.clone();
        let title_variants = title_variants.clone();
        tasks.spawn(async move {
            let name = provider.name().to_string();
            let language = provider.language();
            let search = async {
                if name == "AnimeTVN" {
                    Some(Anime {
                        id: catalog_id.to_string(),
                        provider: name.clone(),
                        title: title.clone(),
                        cover_url: String::new(),
                        banner_url: None,
                        language,
                        total_episodes: None,
                        synopsis: None,
                    })
                } else {
                    let mut candidates = Vec::new();
                    for variant in &title_variants {
                        if let Ok(items) = provider.search(variant).await {
                            candidates.extend(items);
                        }
                    }
                    best_title_match(candidates, &title_variants)
                }
            };
            let result = tokio::time::timeout(std::time::Duration::from_secs(8), search).await;
            let (anime, failure_code) = match result {
                Ok(anime) => {
                    let missing = anime.is_none();
                    (anime, missing.then(|| "TITLE_NOT_AVAILABLE".into()))
                }
                Err(_) => (None, Some("NETWORK_TIMEOUT".into())),
            };
            ProviderAvailabilityDto {
                provider: name,
                language: language_label(language).into(),
                status: if anime.is_some() {
                    "available"
                } else {
                    "unavailable"
                }
                .into(),
                failure_code,
                anime: anime.map(|item| map_anime(item, false, Some(catalog_id))),
            }
        });
    }

    while let Some(result) = tasks.join_next().await {
        if let Ok(item) = result {
            availability.push(item);
        }
    }
    availability.sort_by_key(|item| {
        provider_order
            .iter()
            .position(|provider| provider == &item.provider)
            .unwrap_or(usize::MAX)
    });
    Ok(availability)
}

async fn catalog_title_variants(state: &AppState, catalog_id: i64, title: &str) -> Vec<String> {
    let mut variants = Vec::new();
    push_title_variant(&mut variants, title);
    if let Ok(items) = state.catalog.by_ids(&[catalog_id]).await {
        if let Some(item) = items.into_iter().next() {
            push_title_variant(&mut variants, &item.title);
            if let Some(native) = item.native_title {
                push_title_variant(&mut variants, &native);
            }
        }
    }
    for alias in fixed_title_aliases(title) {
        push_title_variant(&mut variants, alias);
    }
    variants
}

#[tauri::command]
async fn get_continue_watching(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<WatchHistoryDto>, AppErrorDto> {
    state
        .db
        .get_continue_watching(limit.unwrap_or(20))
        .await
        .map(|items| items.into_iter().map(map_history).collect())
        .map_err(|error| app_error("DATABASE_ERROR", "history", None, error, true))
}

#[tauri::command]
async fn get_my_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<FavoriteDto>, AppErrorDto> {
    state
        .db
        .get_favorites(limit.unwrap_or(100))
        .await
        .map(|items| {
            items
                .into_iter()
                .map(
                    |(anime_id, catalog_id, provider, title, cover_url)| FavoriteDto {
                        anime_id,
                        catalog_id,
                        provider,
                        title,
                        cover_url,
                    },
                )
                .collect()
        })
        .map_err(|error| app_error("DATABASE_ERROR", "favorites", None, error, true))
}

#[tauri::command]
async fn search_source(
    state: State<'_, AppState>,
    source: String,
    query: String,
) -> Result<Vec<AnimeDto>, AppErrorDto> {
    let query = query.trim().to_string();
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let provider = state
        .providers
        .get_provider(&source)
        .cloned()
        .ok_or_else(|| {
            app_error_message(
                "PROVIDER_UNAVAILABLE",
                "search",
                Some(&source),
                "Source is not available",
                false,
            )
        })?;

    let results = provider
        .search(&query)
        .await
        .map_err(|error| provider_error("search", &source, error))?;
    let mut mapped = Vec::with_capacity(results.len());
    for anime in results {
        let key = anime_key(&anime.provider, &anime.id);
        let is_favorite = state.db.is_favorite(&key).await.unwrap_or(false);
        mapped.push(map_anime(anime, is_favorite, None));
    }

    Ok(mapped)
}

#[tauri::command]
async fn get_anime_details(
    state: State<'_, AppState>,
    provider: String,
    anime_id: String,
    title: String,
) -> Result<AnimeDetailsDto, AppErrorDto> {
    let mut details = AnimeDetailsDto::default();

    if let Some(provider_ref) = state.providers.get_provider(&provider).cloned() {
        match provider_ref.get_anime_details(&anime_id).await {
            Ok(Some(anime)) => {
                details.cover_url = non_empty(anime.cover_url);
                details.banner_url = anime.banner_url.and_then(non_empty);
                details.total_episodes = anime.total_episodes;
                details.synopsis = anime.synopsis.and_then(non_empty);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    "Provider detail lookup failed for {}:{}: {}",
                    provider,
                    anime_id,
                    error
                );
            }
        }
    }

    if details.synopsis.is_none() || details.banner_url.is_none() || details.cover_url.is_none() {
        match state.metadata.search_and_cache(title.trim()).await {
            Ok(metadata) => {
                if let Some(metadata) = metadata.into_iter().next() {
                    if details.synopsis.is_none() {
                        details.synopsis = metadata.description.and_then(non_empty);
                    }
                    if details.banner_url.is_none() {
                        details.banner_url = metadata.banner_url.and_then(non_empty);
                    }
                    if details.cover_url.is_none() {
                        details.cover_url = metadata.cover_url.and_then(non_empty);
                    }
                    if details.total_episodes.is_none() {
                        details.total_episodes = metadata
                            .episode_count
                            .and_then(|count| u32::try_from(count).ok());
                    }
                }
            }
            Err(error) => {
                tracing::warn!("AniList detail fallback failed for '{}': {}", title, error);
            }
        }
    }

    Ok(details)
}

#[tauri::command]
async fn get_episodes(
    state: State<'_, AppState>,
    provider: String,
    anime_id: String,
) -> Result<Vec<EpisodeDto>, AppErrorDto> {
    let provider_ref = state
        .providers
        .get_provider(&provider)
        .cloned()
        .ok_or_else(|| {
            app_error_message(
                "PROVIDER_UNAVAILABLE",
                "episodes",
                Some(&provider),
                "Provider is not available",
                false,
            )
        })?;

    provider_ref
        .get_episodes(&anime_id)
        .await
        .map(|episodes| episodes.into_iter().map(map_episode).collect())
        .map_err(|error| provider_error("episodes", &provider, error))
}

#[tauri::command]
async fn prepare_playback(
    state: State<'_, AppState>,
    provider: String,
    episode_id: String,
) -> Result<PlaybackDto, AppErrorDto> {
    let stream = resolve_stream(&state, &provider, &episode_id).await?;
    let stream_kind = playback_stream_kind(&stream.video_url).to_string();
    let session = state
        .proxy
        .create_session(&stream)
        .await
        .map_err(|error| app_error("PROXY_FAILED", "proxy", Some(&provider), error, true))?;

    Ok(PlaybackDto {
        session_id: session.session_id,
        playback_url: session.playback_url,
        stream_kind,
        subtitles: stream.subtitles.into_iter().map(map_subtitle).collect(),
        qualities: stream.qualities,
        can_fallback_to_mpv: true,
    })
}

#[tauri::command]
async fn get_skip_times(
    catalog_id: i64,
    episode_number: u32,
) -> Result<Vec<SkipTime>, AppErrorDto> {
    fetch_skip_times(catalog_id, episode_number)
        .await
        .map_err(|error| {
            tracing::warn!(%catalog_id, %episode_number, %error, "AniSkip lookup failed");
            app_error_message(
                "ANISKIP_UNAVAILABLE",
                "skip-times",
                None,
                "Automatic skip times are temporarily unavailable",
                true,
            )
        })
}

#[tauri::command]
async fn download_episode(
    app: AppHandle,
    state: State<'_, AppState>,
    request: DownloadRequest,
    on_event: Channel<DownloadEvent>,
) -> Result<DownloadResult, AppErrorDto> {
    let provider = request.provider.clone();
    let stream = resolve_stream(&state, &provider, &request.episode_id).await?;
    let result = download::download_episode(&app, &stream, &request, &on_event)
        .await
        .map_err(|error| app_error("DOWNLOAD_FAILED", "download", Some(&provider), error, true))?;
    let record = DownloadRecord {
        id: result.id.clone(),
        provider: request.provider,
        anime_id: request.anime_id,
        anime_title: request.anime_title,
        cover_url: request.cover_url,
        episode_id: request.episode_id,
        episode_number: request.episode_number,
        episode_title: request.episode_title,
        file_path: result.file_path.clone(),
        file_name: result.file_name.clone(),
        bytes_downloaded: result.bytes_downloaded,
        media_kind: result.media_kind.clone(),
        completed_at: Utc::now(),
    };
    for attempt in 1..=3 {
        match state.db.save_download(&record).await {
            Ok(()) => break,
            Err(error) if attempt < 3 => {
                tracing::warn!(
                    attempt,
                    path = %record.file_path,
                    %error,
                    "download finished but library persistence failed; retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(150 * attempt)).await;
            }
            Err(error) => {
                tracing::error!(
                    path = %record.file_path,
                    %error,
                    "downloaded media could not be recorded in the library"
                );
                return Err(app_error("DATABASE_ERROR", "downloads", None, error, true));
            }
        }
    }
    Ok(result)
}

#[tauri::command]
async fn list_downloads(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<DownloadRecordDto>, AppErrorDto> {
    let records = state
        .db
        .get_downloads(limit.unwrap_or(500).min(2_000))
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "downloads", None, error, true))?;
    let mut downloads = Vec::with_capacity(records.len());
    for record in records {
        let file_exists = tokio::fs::try_exists(&record.file_path)
            .await
            .unwrap_or(false);
        downloads.push(map_download_record(record, file_exists));
    }
    Ok(downloads)
}

#[tauri::command]
async fn open_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppErrorDto> {
    let record = get_download_record(&state, &id).await?;
    let path = download::validated_registered_path(&app, &record.file_path)
        .await
        .map_err(|error| app_error("DOWNLOAD_FILE_UNAVAILABLE", "downloads", None, error, false))?;
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(app_error_message(
            "DOWNLOAD_FILE_UNAVAILABLE",
            "downloads",
            None,
            "This downloaded file is no longer on this Mac or PC.",
            false,
        ));
    }
    open_path(&path, false)
        .map_err(|error| app_error("DOWNLOAD_OPEN_FAILED", "downloads", None, error, true))
}

#[tauri::command]
async fn reveal_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppErrorDto> {
    let record = get_download_record(&state, &id).await?;
    let path = download::validated_registered_path(&app, &record.file_path)
        .await
        .map_err(|error| app_error("DOWNLOAD_FILE_UNAVAILABLE", "downloads", None, error, false))?;
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(app_error_message(
            "DOWNLOAD_FILE_UNAVAILABLE",
            "downloads",
            None,
            "This downloaded file is no longer on this Mac or PC.",
            false,
        ));
    }
    open_path(&path, true)
        .map_err(|error| app_error("DOWNLOAD_OPEN_FAILED", "downloads", None, error, true))
}

#[tauri::command]
async fn delete_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppErrorDto> {
    let record = get_download_record(&state, &id).await?;
    let path = download::validated_registered_path(&app, &record.file_path)
        .await
        .map_err(|error| app_error("DOWNLOAD_FILE_UNAVAILABLE", "downloads", None, error, false))?;
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|error| app_error("DOWNLOAD_DELETE_FAILED", "downloads", None, error, true))?;
    }
    state
        .db
        .remove_download(&id)
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "downloads", None, error, true))
}

async fn get_download_record(state: &AppState, id: &str) -> Result<DownloadRecord, AppErrorDto> {
    state
        .db
        .get_download(id)
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "downloads", None, error, true))?
        .ok_or_else(|| {
            app_error_message(
                "DOWNLOAD_NOT_FOUND",
                "downloads",
                None,
                "This download is no longer in your library.",
                false,
            )
        })
}

fn map_download_record(record: DownloadRecord, file_exists: bool) -> DownloadRecordDto {
    DownloadRecordDto {
        id: record.id,
        provider: record.provider,
        anime_id: record.anime_id,
        anime_title: record.anime_title,
        cover_url: record.cover_url,
        episode_id: record.episode_id,
        episode_number: record.episode_number,
        episode_title: record.episode_title,
        file_path: record.file_path,
        file_name: record.file_name,
        bytes_downloaded: record.bytes_downloaded,
        media_kind: record.media_kind,
        completed_at: record.completed_at.to_rfc3339(),
        file_exists,
    }
}

fn open_path(path: &Path, reveal: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if reveal {
            command.arg("-R");
        }
        command.arg(path);
        command
    };

    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        if reveal {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(if reveal {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        command
    };

    command
        .spawn()
        .context("Could not open the downloaded file")?;
    Ok(())
}

#[tauri::command]
async fn open_in_mpv(
    state: State<'_, AppState>,
    provider: String,
    episode_id: String,
    start_time: Option<u64>,
) -> Result<(), AppErrorDto> {
    let stream = resolve_stream(&state, &provider, &episode_id).await?;
    Player::new()
        .start_detached(
            &stream.video_url,
            &stream.subtitles,
            &stream.headers,
            start_time,
        )
        .map_err(|error| {
            app_error(
                "PLAYER_LAUNCH_FAILED",
                "player",
                Some(&provider),
                error,
                true,
            )
        })
}

#[tauri::command]
async fn save_progress(
    state: State<'_, AppState>,
    progress: ProgressInput,
) -> Result<(), AppErrorDto> {
    let history = WatchHistory {
        anime_id: progress.anime_id,
        catalog_id: progress.catalog_id,
        provider: progress.provider,
        title: progress.title,
        cover_url: progress.cover_url,
        episode_number: progress.episode_number,
        episode_title: progress.episode_title,
        position_seconds: progress.position_seconds,
        total_seconds: progress.total_seconds,
        updated_at: Utc::now(),
    };

    state
        .db
        .save_watch_history(&history)
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "progress", None, error, true))
}

#[tauri::command]
async fn add_to_my_list(state: State<'_, AppState>, anime: AnimeInput) -> Result<(), AppErrorDto> {
    let key = anime_key(&anime.provider, &anime.id);
    state
        .db
        .save_favorite(
            &key,
            anime.catalog_id,
            &anime.provider,
            &anime.title,
            &anime.cover_url,
        )
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "favorites", None, error, true))
}

#[tauri::command]
async fn remove_from_my_list(
    state: State<'_, AppState>,
    anime_id: String,
) -> Result<(), AppErrorDto> {
    state
        .db
        .remove_favorite(&anime_id)
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "favorites", None, error, true))
}

#[tauri::command]
async fn remove_continue_watching(
    state: State<'_, AppState>,
    anime_id: String,
) -> Result<(), AppErrorDto> {
    state
        .db
        .remove_from_continue_watching(&anime_id)
        .await
        .map_err(|error| app_error("DATABASE_ERROR", "history", None, error, true))
}

async fn resolve_stream(
    state: &AppState,
    provider: &str,
    episode_id: &str,
) -> Result<StreamInfo, AppErrorDto> {
    let provider_ref = state
        .providers
        .get_provider(provider)
        .cloned()
        .ok_or_else(|| {
            app_error_message(
                "PROVIDER_UNAVAILABLE",
                "stream",
                Some(provider),
                "Provider is not available",
                false,
            )
        })?;

    provider_ref
        .get_stream_url(episode_id)
        .await
        .map_err(|error| provider_error("stream", provider, error))
}

fn map_anime(anime: Anime, is_favorite: bool, catalog_id: Option<i64>) -> AnimeDto {
    AnimeDto {
        id: anime.id,
        catalog_id,
        provider: anime.provider,
        title: anime.title,
        cover_url: anime.cover_url,
        banner_url: anime.banner_url,
        language: language_label(anime.language).to_string(),
        total_episodes: anime.total_episodes,
        synopsis: anime.synopsis,
        is_favorite,
    }
}

fn map_episode(episode: Episode) -> EpisodeDto {
    EpisodeDto {
        id: episode.id,
        number: episode.number,
        title: episode.title,
        thumbnail: episode.thumbnail,
    }
}

fn map_history(history: WatchHistory) -> WatchHistoryDto {
    WatchHistoryDto {
        anime_id: history.anime_id,
        catalog_id: history.catalog_id,
        provider: history.provider,
        title: history.title,
        cover_url: history.cover_url,
        episode_number: history.episode_number,
        episode_title: history.episode_title,
        position_seconds: history.position_seconds,
        total_seconds: history.total_seconds,
        updated_at: history.updated_at.to_rfc3339(),
    }
}

fn map_subtitle(subtitle: Subtitle) -> SubtitleDto {
    SubtitleDto {
        language: subtitle.language,
        url: subtitle.url,
    }
}

fn playback_stream_kind(url: &str) -> &'static str {
    let lowercase = url.to_ascii_lowercase();
    if lowercase.contains(".m3u8") {
        "hls"
    } else if lowercase.contains(".mpd") {
        "dash"
    } else {
        "native"
    }
}

fn language_label(language: Language) -> &'static str {
    match language {
        Language::English => "English",
        Language::Vietnamese => "Vietnamese",
    }
}

fn language_group(language: Language) -> &'static str {
    match language {
        Language::English => "english",
        Language::Vietnamese => "vietnamese",
    }
}

fn push_title_variant(variants: &mut Vec<String>, value: &str) {
    let trimmed = value.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        return;
    }
    if !variants
        .iter()
        .any(|existing| normalize_title(existing) == normalize_title(trimmed))
    {
        variants.push(trimmed.to_string());
    }
}

fn fixed_title_aliases(title: &str) -> Vec<&'static str> {
    match normalize_title(title).as_str() {
        "yourname" | "kiminonawa" => vec!["Your Name", "Kimi no Na wa"],
        "caseclosed" | "detectiveconan" | "meitanteiconan" => {
            vec!["Case Closed", "Detective Conan", "Meitantei Conan"]
        }
        "onepiece" | "daohaitac" | "đảohảitặc" => vec!["One Piece", "Đảo Hải Tặc"],
        _ => Vec::new(),
    }
}

fn anime_key(provider: &str, anime_id: &str) -> String {
    format!("{}:{}", provider, anime_id)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn provider_error(operation: &str, provider: &str, error: impl std::fmt::Display) -> AppErrorDto {
    let technical = error.to_string();
    let lower = technical.to_ascii_lowercase();
    let code = if lower.contains("need_captcha") || lower.contains("captcha") {
        "PROVIDER_CAPTCHA"
    } else if lower.contains("403") || lower.contains("forbidden") {
        "STREAM_FORBIDDEN"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "NETWORK_TIMEOUT"
    } else if lower.contains("mapping_not_found") || lower.contains("title_not_available") {
        "TITLE_NOT_AVAILABLE"
    } else if lower.contains("not certified") {
        "PROVIDER_NOT_CERTIFIED"
    } else if operation == "stream" {
        "STREAM_NOT_FOUND"
    } else {
        "PROVIDER_UNAVAILABLE"
    };
    app_error(code, operation, Some(provider), technical, true)
}

fn app_error(
    code: &str,
    operation: &str,
    provider: Option<&str>,
    error: impl std::fmt::Display,
    retryable: bool,
) -> AppErrorDto {
    let technical = sanitize_technical(&error.to_string());
    AppErrorDto {
        code: code.into(),
        message: user_error_message(code).into(),
        provider: provider.map(str::to_string),
        operation: operation.into(),
        retryable,
        correlation_id: Uuid::new_v4().to_string(),
        technical: (!technical.is_empty()).then_some(technical),
    }
}

fn app_error_message(
    code: &str,
    operation: &str,
    provider: Option<&str>,
    message: &str,
    retryable: bool,
) -> AppErrorDto {
    let mut error = app_error(code, operation, provider, message, retryable);
    error.message = message.into();
    error
}

fn user_error_message(code: &str) -> &'static str {
    match code {
        "PROVIDER_CAPTCHA" => "This provider requires verification and is temporarily unavailable.",
        "PROVIDER_NOT_CERTIFIED" => "This provider has not passed playback certification.",
        "PROVIDER_UNAVAILABLE" => "The selected provider is currently unavailable.",
        "TITLE_NOT_AVAILABLE" => "This title is not available from the selected provider.",
        "STREAM_FORBIDDEN" => "The stream host rejected the playback request.",
        "STREAM_NOT_FOUND" => "No working stream was found for this episode.",
        "NETWORK_TIMEOUT" => "The provider did not respond in time.",
        "CATALOG_UNAVAILABLE" => "Anime discovery is temporarily unavailable.",
        "PROXY_FAILED" => "The local playback proxy could not start this stream.",
        "PLAYER_LAUNCH_FAILED" => "The external player could not be opened.",
        "DATABASE_ERROR" => "The local library could not be updated.",
        "DOWNLOAD_FAILED" => {
            "This episode could not be downloaded. Check the source and try again."
        }
        _ => "ani-desk could not complete this request.",
    }
}

fn sanitize_technical(value: &str) -> String {
    let sanitized = value
        .split_whitespace()
        .map(|part| {
            if part.starts_with("http://") || part.starts_with("https://") {
                "[redacted-url]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    sanitized.chars().take(600).collect()
}

async fn personalize_items(state: &AppState, items: &mut [CatalogAnime]) {
    let histories = state
        .db
        .get_continue_watching(100)
        .await
        .unwrap_or_default();
    let favorites = state.db.get_favorites(100).await.unwrap_or_default();
    let mut weighted_ids = Vec::new();

    for history in &histories {
        if let Some(catalog_id) = history.catalog_id {
            let progress = if history.total_seconds > 0 {
                history.position_seconds as f64 / history.total_seconds as f64
            } else {
                0.0
            };
            weighted_ids.push((catalog_id, 1.0 + 2.0 * progress.clamp(0.0, 1.0)));
        }
    }
    for (_, catalog_id, _, _, _) in &favorites {
        if let Some(catalog_id) = catalog_id {
            weighted_ids.push((*catalog_id, 3.0));
        }
    }

    // Resolve a bounded number of legacy rows per request so migration remains lazy.
    for history in histories
        .iter()
        .filter(|item| item.catalog_id.is_none())
        .take(2)
    {
        if let Ok(matches) = state.catalog.search(&history.title, 3).await {
            if let Some(item) = matches
                .into_iter()
                .find(|item| normalize_title(&item.title) == normalize_title(&history.title))
            {
                let _ = state
                    .db
                    .update_history_catalog_id(&history.anime_id, item.catalog_id)
                    .await;
                weighted_ids.push((item.catalog_id, 1.0));
            }
        }
    }
    for (anime_id, _catalog_id, _, title, _) in favorites
        .iter()
        .filter(|(_, catalog_id, _, _, _)| catalog_id.is_none())
        .take(2)
    {
        if let Ok(matches) = state.catalog.search(title, 3).await {
            if let Some(item) = matches
                .into_iter()
                .find(|item| normalize_title(&item.title) == normalize_title(title))
            {
                let _ = state
                    .db
                    .update_favorite_catalog_id(anime_id, item.catalog_id)
                    .await;
                weighted_ids.push((item.catalog_id, 3.0));
            }
        }
    }

    let ids = weighted_ids.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let metadata = state.catalog.by_ids(&ids).await.unwrap_or_default();
    let preferences = weighted_ids
        .into_iter()
        .filter_map(|(id, weight)| {
            metadata
                .iter()
                .find(|item| item.catalog_id == id)
                .map(|item| TastePreference {
                    genres: item.genres.clone(),
                    weight,
                })
        })
        .collect::<Vec<_>>();
    apply_personal_matches(items, &preferences);
}

async fn ensure_provider_health(
    state: &AppState,
    provider: Option<&str>,
) -> HashMap<String, ProviderHealthDto> {
    let cached = state.provider_health.read().await.clone();
    let cache_is_fresh = !cached.is_empty()
        && cached.values().all(|item| {
            item.checked_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .is_some_and(|checked| Utc::now().signed_duration_since(checked).num_minutes() < 5)
        });
    if provider.is_none() && cache_is_fresh {
        return cached;
    }
    refresh_provider_health(state, provider).await
}

async fn refresh_provider_health(
    state: &AppState,
    selected: Option<&str>,
) -> HashMap<String, ProviderHealthDto> {
    let mut tasks = tokio::task::JoinSet::new();
    for provider in state.providers.list_providers() {
        if selected.is_some_and(|name| name != provider.name()) {
            continue;
        }
        let provider = provider.clone();
        tasks.spawn(async move {
            let name = provider.name().to_string();
            let language = provider.language();
            let capabilities = provider.capabilities();
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(30), provider.health_check())
                    .await;
            let (status, failure_code) = match result {
                Ok(Ok(())) => ("healthy".to_string(), None),
                Ok(Err(error)) => {
                    let classified = provider_error("health", &name, error);
                    ("unavailable".to_string(), Some(classified.code))
                }
                Err(_) => (
                    "unavailable".to_string(),
                    Some("NETWORK_TIMEOUT".to_string()),
                ),
            };
            ProviderHealthDto {
                name,
                language: language_label(language).into(),
                language_group: language_group(language).into(),
                status,
                failure_code,
                checked_at: Some(Utc::now().to_rfc3339()),
                capabilities,
                website_url: provider.website_url().map(str::to_string),
            }
        });
    }

    let mut updates = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(item) = result {
            updates.push(item);
        }
    }
    let mut health = state.provider_health.write().await;
    for item in updates {
        health.insert(item.name.clone(), item);
    }
    health.clone()
}

fn main() {
    tracing_subscriber::fmt::init();

    let builder = tauri::Builder::default().plugin(tauri_plugin_process::init());

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|app| {
            let config = Config::load().context("Failed to load ani-desk config")?;
            config.validate().context("Invalid ani-desk config")?;
            let db = tauri::async_runtime::block_on(Database::new())
                .context("Failed to open ani-desk database")?;
            let db = Arc::new(db);
            let providers = ProviderRegistry::new(&config);
            let proxy = tauri::async_runtime::block_on(ProxyState::start())
                .context("Failed to start playback proxy")?;
            let metadata = MetadataCache::new(db.clone());
            let catalog = CatalogClient::new();

            app.manage(AppState {
                db,
                providers,
                proxy,
                metadata,
                catalog,
                provider_health: RwLock::new(HashMap::new()),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            list_provider_health,
            retry_provider_health,
            get_discovery,
            get_genre_catalog,
            get_catalog,
            search_catalog,
            resolve_availability,
            get_continue_watching,
            get_my_list,
            search_source,
            get_anime_details,
            get_episodes,
            prepare_playback,
            get_skip_times,
            download_episode,
            list_downloads,
            open_download,
            reveal_download,
            delete_download,
            open_in_mpv,
            save_progress,
            add_to_my_list,
            remove_from_my_list,
            remove_continue_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ani-desk");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_errors_have_stable_codes() {
        assert_eq!(
            provider_error("stream", "AllAnime", "NEED_CAPTCHA").code,
            "PROVIDER_CAPTCHA"
        );
        assert_eq!(
            provider_error("stream", "Example", "HTTP 403 forbidden").code,
            "STREAM_FORBIDDEN"
        );
        assert_eq!(
            provider_error("stream", "Example", "no candidates").code,
            "STREAM_NOT_FOUND"
        );
    }

    #[test]
    fn diagnostics_redact_urls_and_are_bounded() {
        let sanitized = sanitize_technical(&format!(
            "request https://example.com/private?token=secret {}",
            "x".repeat(900)
        ));
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.contains("[redacted-url]"));
        assert!(sanitized.chars().count() <= 600);
    }
}
