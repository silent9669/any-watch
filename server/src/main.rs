mod db;

use any_watch_core::{
    catalog::{
        apply_personal_matches, CatalogAnime, CatalogClient, CatalogFilters, TastePreference,
    },
    config::Config,
    db::Database,
    metadata::MetadataCache,
    providers::{
        best_title_match, normalize_title, Anime, AnimeProvider, Language, ProviderRegistry,
        StreamInfo, SubtitleFormat,
    },
    skip_times::{fetch_skip_times, SkipTime},
};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use bytes::Bytes;
use db::{NewFavorite, NewHistory, SessionUser, WebDatabase};
use futures_util::{stream, FutureExt, StreamExt, TryStreamExt};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use reqwest::{header::HeaderMap as ReqwestHeaderMap, Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::{
    collections::{HashMap, VecDeque},
    env,
    future::Future,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    path::PathBuf,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::Mutex,
};
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use uuid::Uuid;

const SESSION_COOKIE: &str = "any_watch_session";
const MAX_MEDIA_SESSIONS: usize = 2_048;
const MAX_MEDIA_RESOURCES_PER_SESSION: usize = 8_192;
const MAX_MEDIA_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_SUBTITLE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CURL_PROXY_BYTES: usize = 64 * 1024 * 1024;
const MAX_CURL_METADATA_BYTES: usize = 8 * 1024;
const LOGIN_ATTEMPT_WINDOW: Duration = Duration::from_secs(15 * 60);
const LOGIN_ATTEMPT_LIMIT: usize = 8;
const LOGIN_ATTEMPT_KEY_LIMIT: usize = 10_000;
const PROVIDER_HEALTH_TTL: Duration = Duration::from_secs(5 * 60);
const PROVIDER_HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const PROVIDER_HEALTH_CONCURRENCY: usize = 4;

#[derive(Clone)]
struct AppState {
    db: WebDatabase,
    providers: Arc<ProviderRegistry>,
    catalog: CatalogClient,
    metadata: MetadataCache,
    secure_cookies: bool,
    login_attempts: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    download_tickets: Arc<Mutex<HashMap<String, DownloadTicket>>>,
    media_sessions: Arc<Mutex<HashMap<String, MediaSession>>>,
    provider_health: Arc<Mutex<ProviderHealthCache>>,
    provider_health_refresh: Arc<Mutex<()>>,
    media_client: Client,
}

#[derive(Default)]
struct ProviderHealthCache {
    checked_at: Option<Instant>,
    health: Vec<SourceDto>,
    full_refresh_version: u64,
    provider_refresh_versions: HashMap<String, u64>,
}

impl ProviderHealthCache {
    fn refresh_version(&self, selected: Option<&str>) -> u64 {
        selected.map_or(self.full_refresh_version, |provider| {
            self.provider_refresh_versions
                .get(provider)
                .copied()
                .unwrap_or_default()
        })
    }

    fn selected_health(&self, selected: Option<&str>) -> Vec<SourceDto> {
        self.health
            .iter()
            .filter(|item| selected.is_none_or(|provider| item.name == provider))
            .cloned()
            .collect()
    }

    fn apply_refresh(&mut self, selected: Option<&str>, health: &[SourceDto]) {
        for update in health {
            if let Some(current) = self.health.iter_mut().find(|item| item.name == update.name) {
                *current = update.clone();
            } else {
                self.health.push(update.clone());
            }
            let version = self
                .provider_refresh_versions
                .entry(update.name.clone())
                .or_default();
            *version = version.wrapping_add(1);
        }
        if selected.is_none() {
            self.checked_at = Some(Instant::now());
            self.full_refresh_version = self.full_refresh_version.wrapping_add(1);
        }
    }
}

#[derive(Clone)]
struct DownloadTicket {
    user_id: String,
    expires_at: Instant,
    request: BrowserDownloadInput,
    stream: StreamInfo,
}

#[derive(Clone)]
struct MediaSession {
    user_id: String,
    expires_at: Instant,
    stream: StreamInfo,
    secret: [u8; 32],
    resources: Arc<StdMutex<MediaResourceCache>>,
}

#[derive(Default)]
struct MediaResourceCache {
    entries: HashMap<String, MediaResource>,
    insertion_order: VecDeque<String>,
}

impl MediaResourceCache {
    fn insert(&mut self, id: String, resource: MediaResource) {
        if let Some(entry) = self.entries.get_mut(&id) {
            *entry = resource;
            return;
        }
        while self.entries.len() >= MAX_MEDIA_RESOURCES_PER_SESSION {
            let Some(oldest) = self.insertion_order.pop_front() else {
                self.entries.clear();
                break;
            };
            self.entries.remove(&oldest);
        }
        self.insertion_order.push_back(id.clone());
        self.entries.insert(id, resource);
    }

    fn get(&self, id: &str) -> Option<&MediaResource> {
        self.entries.get(id)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone)]
struct MediaResource {
    url: Url,
    allow_relative_paths: bool,
    transform: MediaTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaTransform {
    None,
    AssToWebVtt,
    SrtToWebVtt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorBody {
    code: String,
    message: String,
    operation: String,
    retryable: bool,
    correlation_id: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, ApiErrorBody);

impl ApiError {
    fn new(
        status: StatusCode,
        code: &str,
        operation: &str,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self(
            status,
            ApiErrorBody {
                code: code.into(),
                message: message.into(),
                operation: operation.into(),
                retryable,
                correlation_id: Uuid::new_v4().to_string(),
            },
        )
    }

    fn internal(operation: &str, error: impl std::fmt::Display) -> Self {
        tracing::error!(operation, error = %error, "request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERVER_ERROR",
            operation,
            "any-watch could not complete this request.",
            true,
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Deserialize)]
struct LoginInput {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserInput {
    username: String,
    password: String,
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserInput {
    username: String,
    enabled: bool,
    role: String,
    password: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProviderHealthInput {
    provider: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogInput {
    filters: CatalogFilters,
    sort: String,
    page: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvailabilityInput {
    catalog_id: i64,
    title: String,
    #[serde(default)]
    title_variants: Vec<String>,
    language_group_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceSearchInput {
    source: String,
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCatalogInput {
    provider: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimeDetailsInput {
    provider: String,
    anime_id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpisodesInput {
    provider: String,
    anime_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackInput {
    provider: String,
    episode_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkipTimesInput {
    catalog_id: i64,
    episode_number: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimeInput {
    id: String,
    catalog_id: Option<i64>,
    provider: String,
    title: String,
    cover_url: String,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserDownloadInput {
    id: String,
    provider: String,
    anime_id: String,
    episode_id: String,
    anime_title: String,
    cover_url: String,
    episode_number: u32,
    episode_title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveInput {
    anime_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    name: String,
    language: String,
    language_group: String,
    status: String,
    failure_code: Option<String>,
    capabilities: any_watch_core::providers::ProviderCapabilities,
    website_url: Option<String>,
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

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AnimeDetailsDto {
    cover_url: Option<String>,
    banner_url: Option<String>,
    total_episodes: Option<u32>,
    synopsis: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AvailabilityDto {
    provider: String,
    language: String,
    status: String,
    failure_code: Option<String>,
    anime: Option<AnimeDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackDto {
    session_id: String,
    playback_url: String,
    stream_kind: String,
    subtitles: Vec<SubtitleDto>,
    qualities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SubtitleDto {
    language: String,
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "any_watch_server=info,tower_http=info".into()),
        )
        .init();

    let data_dir =
        PathBuf::from(env::var("ANY_WATCH_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    tokio::fs::create_dir_all(&data_dir).await?;
    let db = WebDatabase::open(&data_dir.join("web.db")).await?;
    let admin_password = env::var("ANY_WATCH_ADMIN_PASSWORD")
        .context("ANY_WATCH_ADMIN_PASSWORD must be set for the hosted web service")?;
    let admin_username = env::var("ANY_WATCH_ADMIN_USERNAME").unwrap_or_else(|_| "root".into());
    db.bootstrap_admin(&admin_username, &admin_password).await?;

    let core_db = Arc::new(Database::new_at(data_dir.join("catalog.db")).await?);
    let mut config = Config::default();
    if let Ok(instance_url) = env::var("ANY_WATCH_INVIDIOUS_URL") {
        if !instance_url.trim().is_empty() {
            config.sources.invidious = true;
            config.invidious = Some(any_watch_core::config::InvidiousConfig {
                instance_url,
                local_proxy: env::var("ANY_WATCH_INVIDIOUS_LOCAL_PROXY").map_or(true, |value| {
                    value != "0" && !value.eq_ignore_ascii_case("false")
                }),
            });
        }
    }
    config.validate()?;

    let state = AppState {
        db,
        providers: Arc::new(ProviderRegistry::new(&config)),
        catalog: CatalogClient::new(),
        metadata: MetadataCache::new(core_db),
        secure_cookies: env::var_os("RAILWAY_ENVIRONMENT").is_some()
            || env::var("ANY_WATCH_SECURE_COOKIES").is_ok_and(|value| value != "0"),
        login_attempts: Arc::new(Mutex::new(HashMap::new())),
        download_tickets: Arc::new(Mutex::new(HashMap::new())),
        media_sessions: Arc::new(Mutex::new(HashMap::new())),
        provider_health: Arc::new(Mutex::new(ProviderHealthCache::default())),
        provider_health_refresh: Arc::new(Mutex::new(())),
        media_client: Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(6 * 60 * 60))
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()?,
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/session", get(session))
        .route("/admin/users", get(list_users).post(create_user))
        .route("/admin/users/:id", put(update_user).delete(delete_user))
        .route("/sources", get(list_sources))
        .route(
            "/providers/health",
            get(list_provider_health).post(retry_provider_health),
        )
        .route("/discovery", get(discovery))
        .route("/catalog/search", get(search_catalog))
        .route("/catalog/genre/:genre", get(genre_catalog))
        .route("/catalog", post(catalog))
        .route("/availability", post(availability))
        .route("/source/search", post(search_source))
        .route("/provider/catalog", post(provider_catalog))
        .route("/youtube/trending", get(youtube_trending))
        .route("/youtube/popular", get(youtube_popular))
        .route("/youtube/related/:id", get(youtube_related))
        .route("/anime/details", post(anime_details))
        .route("/anime/episodes", post(episodes))
        .route("/playback", post(playback))
        .route("/skip-times", post(skip_times))
        .route("/media/:id", get(media_main))
        .route("/media/:id/resource/:resource_id", get(media_resource))
        .route(
            "/media/:id/resource/:resource_id/*path",
            get(media_resource_path),
        )
        .route("/history", get(history).post(save_progress))
        .route("/history/remove", post(remove_history))
        .route("/my-list", get(my_list).post(add_favorite))
        .route("/my-list/remove", post(remove_favorite))
        .route("/downloads/ticket", post(create_download_ticket))
        .route("/downloads/:id", get(browser_download))
        .layer(DefaultBodyLimit::max(64 * 1024));

    let web_dist = env::var("ANY_WATCH_WEB_DIR").unwrap_or_else(|_| "web/dist".into());
    let index = PathBuf::from(&web_dist).join("index.html");
    let static_files = ServeDir::new(&web_dist).fallback(ServeFile::new(index));
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(static_files)
        .with_state(state)
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache")))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(HeaderName::from_static("strict-transport-security"), HeaderValue::from_static("max-age=31536000; includeSubDomains")))
        .layer(SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin")))
        .layer(SetResponseHeaderLayer::if_not_present(HeaderName::from_static("permissions-policy"), HeaderValue::from_static("camera=(), microphone=(), geolocation=()")))
        .layer(SetResponseHeaderLayer::if_not_present(HeaderName::from_static("content-security-policy"), HeaderValue::from_static("default-src 'self'; img-src 'self' https: data:; media-src 'self' https: blob:; connect-src 'self' https:; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'")))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3000);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "any-watch web server listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "any-watch"}))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<LoginInput>,
) -> ApiResult<Response> {
    require_app_request(&headers)?;
    let client = client_identity(&headers);
    let key = format!("{}:{}", client, input.username.to_lowercase());
    if !allow_login_attempt(&state, &key).await {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "LOGIN_RATE_LIMITED",
            "login",
            "Too many login attempts. Please wait before trying again.",
            true,
        ));
    }
    let user = state
        .db
        .authenticate(&input.username, &input.password)
        .await
        .map_err(|error| ApiError::internal("login", error))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "login",
                "The username or password is incorrect.",
                false,
            )
        })?;
    state.login_attempts.lock().await.remove(&key);
    let token = state
        .db
        .create_session(&user.id)
        .await
        .map_err(|error| ApiError::internal("login", error))?;
    let cookie = session_cookie(&token, state.secure_cookies, 30 * 24 * 60 * 60);
    let mut response = Json(user).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|error| ApiError::internal("login", error))?,
    );
    Ok(response)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    require_app_request(&headers)?;
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        state
            .db
            .revoke_session(&token)
            .await
            .map_err(|error| ApiError::internal("logout", error))?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie("", state.secure_cookies, 0)).unwrap(),
    );
    Ok(response)
}

async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<SessionUser>> {
    Ok(Json(require_user(&state, &headers).await?))
}

async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    require_admin(&state, &headers).await?;
    Ok(Json(json!(state.db.list_users().await.map_err(
        |error| ApiError::internal("admin-users", error)
    )?)))
}

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateUserInput>,
) -> ApiResult<Json<Value>> {
    require_app_request(&headers)?;
    require_admin(&state, &headers).await?;
    let user = state
        .db
        .create_user(&input.username, &input.password, &input.role)
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "USER_CREATE_FAILED",
                "admin-users",
                error.to_string(),
                false,
            )
        })?;
    Ok(Json(json!(user)))
}

async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpdateUserInput>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let admin = require_admin(&state, &headers).await?;
    if admin.id == id && (!input.enabled || input.role != "admin") {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ADMIN_SELF_LOCKOUT",
            "admin-users",
            "You cannot disable or demote the account used for this session.",
            false,
        ));
    }
    if state
        .db
        .is_protected_user(&id)
        .await
        .map_err(|error| ApiError::internal("admin-users", error))?
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "PROTECTED_ADMIN_IMMUTABLE",
            "admin-users",
            "The protected administrator account is managed by the server configuration and cannot be changed here.",
            false,
        ));
    }
    state
        .db
        .update_user(
            &id,
            &input.username,
            input.enabled,
            &input.role,
            input.password.as_deref(),
        )
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "USER_UPDATE_FAILED",
                "admin-users",
                error.to_string(),
                false,
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let admin = require_admin(&state, &headers).await?;
    if admin.id == id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ADMIN_SELF_DELETE",
            "admin-users",
            "You cannot delete the account used for this session.",
            false,
        ));
    }
    if state
        .db
        .is_protected_user(&id)
        .await
        .map_err(|error| ApiError::internal("admin-users", error))?
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "PROTECTED_ADMIN_IMMUTABLE",
            "admin-users",
            "The protected administrator account is managed by the server configuration and cannot be deleted here.",
            false,
        ));
    }
    state.db.delete_user(&id).await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "USER_DELETE_FAILED",
            "admin-users",
            error.to_string(),
            false,
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<SourceDto>>> {
    require_user(&state, &headers).await?;
    let cached = state.provider_health.lock().await;
    let mut sources = source_list(&state);
    for source in &mut sources {
        if let Some(health) = cached
            .health
            .iter()
            .find(|health| health.name == source.name)
        {
            *source = health.clone();
        }
    }
    Ok(Json(sources))
}

fn source_list(state: &AppState) -> Vec<SourceDto> {
    state
        .providers
        .list_providers()
        .iter()
        .map(|provider| SourceDto {
            name: provider.name().into(),
            language: language_label(provider.language()).into(),
            language_group: language_group(provider.language()).into(),
            status: "unknown".into(),
            failure_code: None,
            capabilities: provider.capabilities(),
            website_url: provider.website_url().map(str::to_string),
        })
        .collect()
}

fn source_dto(
    provider: &dyn AnimeProvider,
    status: &str,
    failure_code: Option<String>,
) -> SourceDto {
    SourceDto {
        name: provider.name().into(),
        language: language_label(provider.language()).into(),
        language_group: language_group(provider.language()).into(),
        status: status.into(),
        failure_code,
        capabilities: provider.capabilities(),
        website_url: provider.website_url().map(str::to_string),
    }
}

async fn list_provider_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<SourceDto>>> {
    require_user(&state, &headers).await?;
    {
        let cache = state.provider_health.lock().await;
        if cache
            .checked_at
            .is_some_and(|checked_at| checked_at.elapsed() < PROVIDER_HEALTH_TTL)
        {
            return Ok(Json(cache.health.clone()));
        }
    }

    let _refresh = state.provider_health_refresh.lock().await;
    let cache = state.provider_health.lock().await;
    if cache
        .checked_at
        .is_some_and(|checked_at| checked_at.elapsed() < PROVIDER_HEALTH_TTL)
    {
        return Ok(Json(cache.health.clone()));
    }
    drop(cache);
    let health = check_provider_health(&state, None).await?;
    let mut cache = state.provider_health.lock().await;
    cache.apply_refresh(None, &health);
    Ok(Json(health))
}

async fn retry_provider_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProviderHealthInput>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    require_app_request(&headers)?;
    require_user(&state, &headers).await?;
    let selected = input.provider.as_deref();
    let health = refresh_provider_health_coalesced(
        state.provider_health.as_ref(),
        state.provider_health_refresh.as_ref(),
        selected,
        check_provider_health(&state, selected),
    )
    .await?;
    Ok(Json(health))
}

async fn refresh_provider_health_coalesced(
    cache: &Mutex<ProviderHealthCache>,
    refresh: &Mutex<()>,
    selected: Option<&str>,
    check: impl std::future::Future<Output = ApiResult<Vec<SourceDto>>>,
) -> ApiResult<Vec<SourceDto>> {
    let initial_version = cache.lock().await.refresh_version(selected);
    let _refresh = refresh.lock().await;
    {
        let cache = cache.lock().await;
        if cache.refresh_version(selected) != initial_version {
            return Ok(cache.selected_health(selected));
        }
    }

    let health = check.await?;
    cache.lock().await.apply_refresh(selected, &health);
    Ok(health)
}

async fn run_provider_health_checks<I, F, T>(checks: I) -> Vec<T>
where
    I: IntoIterator<Item = F>,
    F: std::future::Future<Output = T>,
{
    stream::iter(checks)
        .buffered(PROVIDER_HEALTH_CONCURRENCY)
        .collect()
        .await
}

async fn check_provider_health(
    state: &AppState,
    selected: Option<&str>,
) -> ApiResult<Vec<SourceDto>> {
    if selected.is_some_and(|name| state.providers.get_provider(name).is_none()) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_NOT_FOUND",
            "provider-health",
            "The selected provider is not available.",
            false,
        ));
    }

    let mut checks: Vec<Pin<Box<dyn Future<Output = SourceDto> + Send>>> = Vec::new();
    for provider in state.providers.list_providers() {
        if selected.is_some_and(|name| name != provider.name()) {
            continue;
        }
        let provider: Arc<dyn AnimeProvider> = Arc::clone(provider);
        checks.push(Box::pin(async move {
            let result = tokio::time::timeout(
                PROVIDER_HEALTH_TIMEOUT,
                AssertUnwindSafe(provider.health_check()).catch_unwind(),
            )
            .await;
            match result {
                Ok(Ok(Ok(()))) => source_dto(provider.as_ref(), "healthy", None),
                Ok(Ok(Err(error))) => source_dto(
                    provider.as_ref(),
                    "unavailable",
                    Some(classify_provider_error(&error.to_string()).into()),
                ),
                Ok(Err(_)) => source_dto(
                    provider.as_ref(),
                    "unavailable",
                    Some("PROVIDER_UNAVAILABLE".into()),
                ),
                Err(_) => source_dto(
                    provider.as_ref(),
                    "unavailable",
                    Some("NETWORK_TIMEOUT".into()),
                ),
            }
        }));
    }

    Ok(run_provider_health_checks(checks).await)
}

async fn discovery(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    let mut discovery = state
        .catalog
        .discovery()
        .await
        .map_err(|error| ApiError::internal("catalog", error))?;
    let preferences = catalog_preferences(&state, &user.id).await;
    apply_personal_matches(&mut discovery.trending, &preferences);
    apply_personal_matches(&mut discovery.popular_this_season, &preferences);
    Ok(Json(json!(discovery)))
}

async fn search_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    let mut items = state
        .catalog
        .search(query.query.trim(), 24)
        .await
        .map_err(|error| ApiError::internal("catalog-search", error))?;
    personalize_catalog_items(&state, &user.id, &mut items).await;
    Ok(Json(json!(items)))
}

async fn genre_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(genre): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    let mut items = state
        .catalog
        .by_genre(&genre, 24)
        .await
        .map_err(|error| ApiError::internal("catalog", error))?;
    personalize_catalog_items(&state, &user.id, &mut items).await;
    Ok(Json(json!(items)))
}

async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CatalogInput>,
) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    let mut page = state
        .catalog
        .catalog(&input.filters, &input.sort, input.page.unwrap_or(1), 24)
        .await
        .map_err(|error| ApiError::internal("catalog", error))?;
    personalize_catalog_items(&state, &user.id, &mut page.items).await;
    Ok(Json(json!(page)))
}

async fn personalize_catalog_items(state: &AppState, user_id: &str, items: &mut [CatalogAnime]) {
    let preferences = catalog_preferences(state, user_id).await;
    apply_personal_matches(items, &preferences);
}

async fn catalog_preferences(state: &AppState, user_id: &str) -> Vec<TastePreference> {
    let histories = state.db.history(user_id, 100).await.unwrap_or_default();
    let favorites = state.db.favorites(user_id, 100).await.unwrap_or_default();
    let mut weighted_ids = HashMap::<i64, f64>::new();

    for history in &histories {
        if let Some(catalog_id) = history.catalog_id {
            let progress = if history.total_seconds > 0 {
                history.position_seconds as f64 / history.total_seconds as f64
            } else {
                0.0
            };
            *weighted_ids.entry(catalog_id).or_default() += 1.0 + 2.0 * progress.clamp(0.0, 1.0);
        }
    }
    for favorite in &favorites {
        if let Some(catalog_id) = favorite.catalog_id {
            *weighted_ids.entry(catalog_id).or_default() += 3.0;
        }
    }

    // Older rows may predate catalog IDs. Resolve only a few per request so
    // existing user data contributes without adding unbounded network work.
    let unresolved = histories
        .iter()
        .filter(|item| item.catalog_id.is_none())
        .map(|item| (item.title.as_str(), 1.0))
        .chain(
            favorites
                .iter()
                .filter(|item| item.catalog_id.is_none())
                .map(|item| (item.title.as_str(), 3.0)),
        )
        .take(4);
    for (title, weight) in unresolved {
        if let Ok(matches) = state.catalog.search(title, 3).await {
            if let Some(item) = matches
                .into_iter()
                .find(|item| normalize_title(&item.title) == normalize_title(title))
            {
                *weighted_ids.entry(item.catalog_id).or_default() += weight;
            }
        }
    }

    let metadata = state
        .catalog
        .by_ids(&weighted_ids.keys().copied().collect::<Vec<_>>())
        .await
        .unwrap_or_default();
    metadata
        .into_iter()
        .filter_map(|item| {
            weighted_ids
                .get(&item.catalog_id)
                .map(|weight| TastePreference {
                    genres: item.genres,
                    weight: *weight,
                })
        })
        .collect()
}

async fn availability(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AvailabilityInput>,
) -> ApiResult<Json<Vec<AvailabilityDto>>> {
    require_user(&state, &headers).await?;
    let mut titles = Vec::new();
    for title in std::iter::once(input.title).chain(input.title_variants) {
        let title = title.trim();
        if title.is_empty()
            || titles
                .iter()
                .any(|current: &String| current.eq_ignore_ascii_case(title))
        {
            continue;
        }
        titles.push(title.to_string());
        if titles.len() == 8 {
            break;
        }
    }
    let mut values = Vec::new();
    for provider in state.providers.list_providers() {
        if input
            .language_group_filter
            .as_deref()
            .is_some_and(|group| !group.eq_ignore_ascii_case(language_group(provider.language())))
        {
            continue;
        }
        let mut selected = None;
        let mut successful_search = false;
        let mut last_error = None;
        for title in &titles {
            match provider.search(title.trim()).await {
                Ok(items) => {
                    successful_search = true;
                    selected = best_title_match(items, &titles)
                        .map(|anime| map_anime(anime, Some(input.catalog_id)));
                    if selected.is_some() {
                        break;
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        let (status, failure_code, anime) = if selected.is_some() {
            ("available".into(), None, selected)
        } else if successful_search {
            (
                "unavailable".into(),
                Some("TITLE_NOT_AVAILABLE".into()),
                None,
            )
        } else {
            (
                "unavailable".into(),
                Some(
                    last_error
                        .as_ref()
                        .map(|error| classify_provider_error(&error.to_string()))
                        .unwrap_or("PROVIDER_UNAVAILABLE")
                        .into(),
                ),
                None,
            )
        };
        values.push(AvailabilityDto {
            provider: provider.name().into(),
            language: language_label(provider.language()).into(),
            status,
            failure_code,
            anime,
        });
    }
    Ok(Json(values))
}

async fn search_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SourceSearchInput>,
) -> ApiResult<Json<Vec<AnimeDto>>> {
    require_user(&state, &headers).await?;
    if input.query.trim().len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let provider = state.providers.get_provider(&input.source).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_UNAVAILABLE",
            "search",
            "Source is not available.",
            false,
        )
    })?;
    let values = provider.search(input.query.trim()).await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            classify_provider_error(&error.to_string()),
            "provider-search",
            "The provider could not complete this search.",
            true,
        )
    })?;
    Ok(Json(
        values
            .into_iter()
            .map(|anime| map_anime(anime, None))
            .collect(),
    ))
}

async fn provider_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProviderCatalogInput>,
) -> ApiResult<Json<Vec<AnimeDto>>> {
    require_user(&state, &headers).await?;
    let provider = state
        .providers
        .get_provider(&input.provider)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "PROVIDER_UNAVAILABLE",
                "provider-catalog",
                "Provider is not available.",
                false,
            )
        })?;
    let values = provider.catalog().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            classify_provider_error(&error.to_string()),
            "provider-catalog",
            "The provider could not load catalog.",
            true,
        )
    })?;
    Ok(Json(
        values
            .into_iter()
            .map(|anime| map_anime(anime, None))
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
struct YouTubeTrendingQuery {
    topic: Option<String>,
}

async fn youtube_trending(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<YouTubeTrendingQuery>,
) -> ApiResult<Json<Vec<AnimeDto>>> {
    require_user(&state, &headers).await?;
    let provider = state.providers.invidious().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_UNAVAILABLE",
            "youtube-trending",
            "Invidious provider is not configured or enabled.",
            false,
        )
    })?;
    let items = provider
        .trending(query.topic.as_deref())
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                classify_provider_error(&error.to_string()),
                "youtube-trending",
                "Failed to fetch trending videos from Invidious.",
                true,
            )
        })?;
    Ok(Json(
        items
            .into_iter()
            .map(|anime| map_anime(anime, None))
            .collect(),
    ))
}

async fn youtube_popular(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<AnimeDto>>> {
    require_user(&state, &headers).await?;
    let provider = state.providers.invidious().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_UNAVAILABLE",
            "youtube-popular",
            "Invidious provider is not configured or enabled.",
            false,
        )
    })?;
    let items = provider.popular().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            classify_provider_error(&error.to_string()),
            "youtube-popular",
            "Failed to fetch popular videos from Invidious.",
            true,
        )
    })?;
    Ok(Json(
        items
            .into_iter()
            .map(|anime| map_anime(anime, None))
            .collect(),
    ))
}

async fn youtube_related(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(video_id): Path<String>,
) -> ApiResult<Json<Vec<AnimeDto>>> {
    require_user(&state, &headers).await?;
    let provider = state.providers.invidious().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_UNAVAILABLE",
            "youtube-related",
            "Invidious provider is not configured or enabled.",
            false,
        )
    })?;
    let items = provider.related(&video_id).await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            classify_provider_error(&error.to_string()),
            "youtube-related",
            "Failed to fetch related videos from Invidious.",
            true,
        )
    })?;
    Ok(Json(
        items
            .into_iter()
            .map(|anime| map_anime(anime, None))
            .collect(),
    ))
}

async fn anime_details(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AnimeDetailsInput>,
) -> ApiResult<Json<AnimeDetailsDto>> {
    require_user(&state, &headers).await?;
    let mut details = AnimeDetailsDto::default();
    let mut metadata_allowed = true;
    if let Some(provider) = state.providers.get_provider(&input.provider) {
        metadata_allowed = provider.language() != Language::Youtube;
        if let Ok(Some(anime)) = provider.get_anime_details(&input.anime_id).await {
            details.cover_url = non_empty(anime.cover_url);
            details.banner_url = anime.banner_url.and_then(non_empty);
            details.total_episodes = anime.total_episodes;
            details.synopsis = anime.synopsis.and_then(non_empty);
        }
    }
    if !metadata_allowed {
        return Ok(Json(details));
    }
    if let Ok(Some(metadata)) = state
        .metadata
        .search_and_cache(input.title.trim())
        .await
        .map(|items| items.into_iter().next())
    {
        details.cover_url = details.cover_url.or(metadata.cover_url.and_then(non_empty));
        details.banner_url = details
            .banner_url
            .or(metadata.banner_url.and_then(non_empty));
        details.total_episodes = details.total_episodes.or(metadata
            .episode_count
            .and_then(|count| u32::try_from(count).ok()));
        details.synopsis = details
            .synopsis
            .or(metadata.description.and_then(non_empty));
    }
    Ok(Json(details))
}

async fn episodes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<EpisodesInput>,
) -> ApiResult<Json<Value>> {
    require_user(&state, &headers).await?;
    let provider = state
        .providers
        .get_provider(&input.provider)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "PROVIDER_UNAVAILABLE",
                "episodes",
                "Provider is not available.",
                false,
            )
        })?;
    Ok(Json(json!(provider
        .get_episodes(&input.anime_id)
        .await
        .map_err(|error| ApiError::new(
            StatusCode::BAD_GATEWAY,
            classify_provider_error(&error.to_string()),
            "episodes",
            "Episodes are currently unavailable from this provider.",
            true
        ))?)))
}

async fn playback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PlaybackInput>,
) -> ApiResult<Json<PlaybackDto>> {
    let user = require_user(&state, &headers).await?;
    let stream = resolve_stream(&state, &input.provider, &input.episode_id).await?;
    let id = Uuid::new_v4().to_string();
    let mut secret = [0_u8; 32];
    OsRng.fill_bytes(&mut secret);
    let now = Instant::now();
    let session = MediaSession {
        user_id: user.id,
        expires_at: now + Duration::from_secs(6 * 60 * 60),
        stream: stream.clone(),
        secret,
        resources: Arc::new(StdMutex::new(MediaResourceCache::default())),
    };
    let response = playback_dto(&id, &session);
    let mut sessions = state.media_sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > now);
    while sessions.len() >= MAX_MEDIA_SESSIONS {
        let Some(oldest_id) = sessions
            .iter()
            .min_by_key(|(_, session)| session.expires_at)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        sessions.remove(&oldest_id);
    }
    sessions.insert(id, session);
    Ok(Json(response))
}

fn playback_dto(id: &str, session: &MediaSession) -> PlaybackDto {
    let subtitles = session
        .stream
        .subtitles
        .iter()
        .filter_map(|subtitle| {
            Url::parse(&subtitle.url).ok().map(|url| SubtitleDto {
                language: subtitle.language.clone(),
                url: opaque_subtitle_url(id, session, url, subtitle.format),
            })
        })
        .collect();
    PlaybackDto {
        session_id: id.into(),
        playback_url: format!("/api/media/{id}"),
        stream_kind: if session
            .stream
            .video_url
            .to_ascii_lowercase()
            .contains(".m3u8")
            || session.stream.video_url.contains("/api/manifest/hls")
        {
            "hls"
        } else if session
            .stream
            .video_url
            .to_ascii_lowercase()
            .contains(".mpd")
            || session.stream.video_url.contains("/api/manifest/dash/")
        {
            "dash"
        } else {
            "native"
        }
        .into(),
        subtitles,
        qualities: session.stream.qualities.clone(),
    }
}

async fn skip_times(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SkipTimesInput>,
) -> ApiResult<Json<Vec<SkipTime>>> {
    require_user(&state, &headers).await?;
    if input.catalog_id <= 0 || input.episode_number == 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_SKIP_TIMES_INPUT",
            "skip-times",
            "A positive catalog ID and episode number are required.",
            false,
        ));
    }
    let times = fetch_skip_times(input.catalog_id, input.episode_number)
        .await
        .map_err(|error| {
            tracing::warn!(
                catalog_id = input.catalog_id,
                episode_number = input.episode_number,
                %error,
                "AniSkip lookup failed"
            );
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "ANISKIP_UNAVAILABLE",
                "skip-times",
                "Automatic skip times are temporarily unavailable.",
                true,
            )
        })?;
    Ok(Json(times))
}

async fn media_main(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let session = get_media_session(&state, &id, &user.id).await?;
    let url = Url::parse(&session.stream.video_url).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_STREAM",
            "playback",
            error.to_string(),
            false,
        )
    })?;
    proxy_media_url(&state, &id, &session, url, &headers).await
}

async fn media_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, resource_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let session = get_media_session(&state, &id, &user.id).await?;
    let resource = resolve_media_resource(&session, &resource_id)?;
    if matches!(
        resource.transform,
        MediaTransform::AssToWebVtt | MediaTransform::SrtToWebVtt
    ) {
        return proxy_text_subtitle(&state, &id, &session, resource.url, resource.transform).await;
    }
    proxy_media_url(&state, &id, &session, resource.url, &headers).await
}

async fn media_resource_path(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, resource_id, path)): Path<(String, String, String)>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let session = get_media_session(&state, &id, &user.id).await?;
    let upstream = resolve_opaque_resource(&session, &resource_id, Some(&path))?;
    proxy_media_url(&state, &id, &session, upstream, &headers).await
}

fn resolve_dash_upstream(base: Url, relative_path: &str) -> ApiResult<Url> {
    let origin = base.origin();
    let base_query = base.query().map(str::to_owned);
    let upstream = if relative_path.is_empty() {
        base
    } else {
        let mut upstream = base
            .join(relative_path)
            .map_err(|_| invalid_media_resource())?;
        if upstream.query().is_none() {
            upstream.set_query(base_query.as_deref());
        }
        upstream
    };
    if upstream.origin() != origin {
        return Err(invalid_media_resource());
    }
    Ok(upstream)
}

fn invalid_media_resource() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "INVALID_MEDIA_RESOURCE",
        "playback",
        "The media resource is invalid or expired.",
        false,
    )
}

async fn get_media_session(state: &AppState, id: &str, user_id: &str) -> ApiResult<MediaSession> {
    let now = Instant::now();
    let mut sessions = state.media_sessions.lock().await;
    sessions.retain(|_, session| session.expires_at > now);
    let session = sessions.get(id).cloned().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PLAYBACK_SESSION_EXPIRED",
            "playback",
            "This playback session expired. Open the episode again.",
            true,
        )
    })?;
    if session.user_id != user_id {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "PLAYBACK_SESSION_FORBIDDEN",
            "playback",
            "This playback session belongs to another account.",
            false,
        ));
    }
    Ok(session)
}

fn download_proxy_failure(url: &Url, failure_kind: &str) -> ApiError {
    tracing::warn!(
        upstream_host = url.host_str().unwrap_or("unknown"),
        failure_kind,
        "upstream download request failed"
    );
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "DOWNLOAD_FAILED",
        "download",
        "The upstream download is temporarily unavailable.",
        true,
    )
}

fn media_proxy_failure(session_id: &str, url: &Url, failure_kind: &str) -> ApiError {
    tracing::warn!(
        session_id,
        upstream_host = url.host_str().unwrap_or("unknown"),
        failure_kind,
        "upstream media request failed"
    );
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "PROXY_FAILED",
        "playback",
        "The upstream media resource is temporarily unavailable.",
        true,
    )
}

#[derive(Debug, PartialEq, Eq)]
enum ResponseBodyError {
    Read,
    TooLarge,
}

async fn read_limited_response_body(
    response: reqwest::Response,
    maximum_bytes: usize,
) -> Result<Bytes, ResponseBodyError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err(ResponseBodyError::TooLarge);
    }

    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks
        .try_next()
        .await
        .map_err(|_| ResponseBodyError::Read)?
    {
        if chunk.len() > maximum_bytes.saturating_sub(body.len()) {
            return Err(ResponseBodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(body))
}

async fn read_limited_async<R>(
    mut reader: R,
    maximum_bytes: usize,
) -> Result<Vec<u8>, ResponseBodyError>
where
    R: AsyncRead + Unpin,
{
    let mut body = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|_| ResponseBodyError::Read)?;
        if read == 0 {
            return Ok(body);
        }
        if read > maximum_bytes.saturating_sub(body.len()) {
            return Err(ResponseBodyError::TooLarge);
        }
        body.extend_from_slice(&chunk[..read]);
    }
}

fn enforce_body_limit(body: Vec<u8>, maximum_bytes: usize) -> Result<Vec<u8>, ResponseBodyError> {
    if body.len() > maximum_bytes {
        return Err(ResponseBodyError::TooLarge);
    }
    Ok(body)
}

fn manifest_body_error(session_id: &str, url: &Url, error: ResponseBodyError) -> ApiError {
    match error {
        ResponseBodyError::Read => media_proxy_failure(session_id, url, "body"),
        ResponseBodyError::TooLarge => ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_MANIFEST",
            "playback",
            "The upstream media manifest is too large.",
            false,
        ),
    }
}

fn subtitle_body_error(session_id: &str, url: &Url, error: ResponseBodyError) -> ApiError {
    match error {
        ResponseBodyError::Read => media_proxy_failure(session_id, url, "subtitle-body"),
        ResponseBodyError::TooLarge => ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_SUBTITLE",
            "playback",
            "The subtitle track is too large.",
            false,
        ),
    }
}

async fn proxy_media_url(
    state: &AppState,
    session_id: &str,
    session: &MediaSession,
    url: Url,
    incoming: &HeaderMap,
) -> ApiResult<Response> {
    if session.stream.use_curl {
        return proxy_media_url_via_curl(session_id, session, url, incoming).await;
    }
    let mut request = state
        .media_client
        .get(url.clone())
        .headers(stream_headers(&session.stream)?);
    if let Some(range) = incoming
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        request = request.header(reqwest::header::RANGE, range);
    }
    let response = request
        .send()
        .await
        .map_err(|_| media_proxy_failure(session_id, &url, "request"))?;
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let hls = url.path().to_ascii_lowercase().contains(".m3u8")
        || content_type
            .as_deref()
            .is_some_and(|value| value.contains("mpegurl"));
    if hls {
        let response = response
            .error_for_status()
            .map_err(|_| media_proxy_failure(session_id, &url, "status"))?;
        let bytes = read_limited_response_body(response, MAX_MEDIA_MANIFEST_BYTES)
            .await
            .map_err(|error| manifest_body_error(session_id, &url, error))?;
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| media_proxy_failure(session_id, &url, "body"))?;
        let rewritten = rewrite_hls_manifest(session_id, session, &url, &text);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(rewritten))
            .map_err(|error| ApiError::internal("playback", error));
    }

    let dash = url.path().to_ascii_lowercase().contains(".mpd")
        || content_type
            .as_deref()
            .is_some_and(|value| value.contains("dash+xml"));
    if dash {
        let response = response
            .error_for_status()
            .map_err(|_| media_proxy_failure(session_id, &url, "status"))?;
        let bytes = read_limited_response_body(response, MAX_MEDIA_MANIFEST_BYTES)
            .await
            .map_err(|error| manifest_body_error(session_id, &url, error))?;
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| media_proxy_failure(session_id, &url, "body"))?;
        let rewritten = rewrite_dash_manifest(session_id, session, &url, &text);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/dash+xml; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(rewritten))
            .map_err(|error| ApiError::internal("playback", error));
    }

    let mut builder = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "private, no-store");
    if let Some(value) = content_type {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    for (source, target) in [
        (reqwest::header::CONTENT_LENGTH, header::CONTENT_LENGTH),
        (reqwest::header::CONTENT_RANGE, header::CONTENT_RANGE),
        (reqwest::header::ACCEPT_RANGES, header::ACCEPT_RANGES),
    ] {
        if let Some(value) = response
            .headers()
            .get(source)
            .and_then(|value| value.to_str().ok())
        {
            builder = builder.header(target, value);
        }
    }
    builder
        .body(Body::from_stream(
            response.bytes_stream().map_err(std::io::Error::other),
        ))
        .map_err(|error| ApiError::internal("playback", error))
}

async fn proxy_media_url_via_curl(
    session_id: &str,
    session: &MediaSession,
    url: Url,
    incoming: &HeaderMap,
) -> ApiResult<Response> {
    let range = incoming
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (status, content_type, body) =
        curl_fetch(&url, &session.stream.headers, range.as_deref(), 600).await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "PROXY_FAILED",
            "playback",
            format!("upstream returned {status}"),
            true,
        ));
    }
    let hls = url.path().to_ascii_lowercase().contains(".m3u8") || content_type.contains("mpegurl");
    if hls {
        let body = enforce_body_limit(body, MAX_MEDIA_MANIFEST_BYTES)
            .map_err(|error| manifest_body_error(session_id, &url, error))?;
        let text =
            String::from_utf8(body).map_err(|_| media_proxy_failure(session_id, &url, "body"))?;
        let rewritten = rewrite_hls_manifest(session_id, session, &url, &text);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(rewritten))
            .map_err(|error| ApiError::internal("playback", error));
    }
    let dash =
        url.path().to_ascii_lowercase().contains(".mpd") || content_type.contains("dash+xml");
    if dash {
        let body = enforce_body_limit(body, MAX_MEDIA_MANIFEST_BYTES)
            .map_err(|error| manifest_body_error(session_id, &url, error))?;
        let text =
            String::from_utf8(body).map_err(|_| media_proxy_failure(session_id, &url, "body"))?;
        let rewritten = rewrite_dash_manifest(session_id, session, &url, &text);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/dash+xml; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(rewritten))
            .map_err(|error| ApiError::internal("playback", error));
    }
    let mut builder = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "private, no-store");
    if !content_type.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    builder
        .body(Body::from(body))
        .map_err(|error| ApiError::internal("playback", error))
}

async fn curl_fetch(
    url: &Url,
    headers: &std::collections::HashMap<String, String>,
    range: Option<&str>,
    max_time: u32,
) -> ApiResult<(u16, String, Vec<u8>)> {
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "-L".into(),
        "--max-redirs".into(),
        "8".into(),
        "--max-time".into(),
        max_time.to_string(),
    ];
    for (name, value) in headers {
        match name.to_ascii_lowercase().as_str() {
            "user-agent" => {
                args.push("-A".into());
                args.push(value.clone());
            }
            "referer" => {
                args.push("-e".into());
                args.push(value.clone());
            }
            "accept" | "accept-encoding" | "connection" | "host" | "content-length" => {}
            _ => args.extend(["-H".into(), format!("{name}: {value}")]),
        }
    }
    if let Some(range) = range {
        args.push("-H".into());
        args.push(format!("Range: {range}"));
    }
    args.push("-w".into());
    args.push("\n__ANY_WATCH__%{http_code}__%{content_type}".into());
    args.push(url.as_str().into());
    let mut child = tokio::process::Command::new("curl")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| media_proxy_failure("curl", url, "curl-start"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| media_proxy_failure("curl", url, "curl-stdout"))?;
    let output = match read_limited_async(
        stdout,
        MAX_CURL_PROXY_BYTES.saturating_add(MAX_CURL_METADATA_BYTES),
    )
    .await
    {
        Ok(output) => output,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(media_proxy_failure("curl", url, "curl-body"));
        }
    };
    let exit_status = child
        .wait()
        .await
        .map_err(|_| media_proxy_failure("curl", url, "curl-wait"))?;
    let (body, status, content_type) = parse_curl_output(output);
    let body = enforce_body_limit(body, MAX_CURL_PROXY_BYTES)
        .map_err(|_| media_proxy_failure("curl", url, "curl-body"))?;
    if !exit_status.success() || status == 0 {
        return Err(media_proxy_failure("curl", url, "curl-exit"));
    }
    Ok((status, content_type, body))
}

fn parse_curl_output(stdout: Vec<u8>) -> (Vec<u8>, u16, String) {
    const MARKER: &[u8] = b"\n__ANY_WATCH__";
    let marker_index = stdout
        .windows(MARKER.len())
        .rposition(|window| window == MARKER);
    let (body, meta) = match marker_index {
        Some(index) => (
            stdout[..index].to_vec(),
            stdout[index + MARKER.len()..].to_vec(),
        ),
        None => (stdout, Vec::new()),
    };
    let meta = String::from_utf8_lossy(&meta).into_owned();
    let mut fields = meta.split("__");
    let status = fields
        .next()
        .unwrap_or("000")
        .trim()
        .parse::<u16>()
        .unwrap_or(0);
    let content_type = fields.next().unwrap_or_default().to_string();
    (body, status, content_type)
}

async fn proxy_text_subtitle(
    state: &AppState,
    session_id: &str,
    session: &MediaSession,
    url: Url,
    transform: MediaTransform,
) -> ApiResult<Response> {
    let response = state
        .media_client
        .get(url.clone())
        .headers(stream_headers(&session.stream)?)
        .send()
        .await
        .map_err(|_| media_proxy_failure(session_id, &url, "subtitle-request"))?
        .error_for_status()
        .map_err(|_| media_proxy_failure(session_id, &url, "subtitle-status"))?;
    let bytes = read_limited_response_body(response, MAX_SUBTITLE_BYTES)
        .await
        .map_err(|error| subtitle_body_error(session_id, &url, error))?;
    let subtitle = String::from_utf8(bytes.to_vec()).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_SUBTITLE",
            "playback",
            "The subtitle track is not valid UTF-8.",
            false,
        )
    })?;
    let converted = match transform {
        MediaTransform::AssToWebVtt => ass_to_webvtt(&subtitle),
        MediaTransform::SrtToWebVtt => srt_to_webvtt(&subtitle),
        MediaTransform::None => {
            return Err(invalid_media_resource());
        }
    };
    let webvtt = converted.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_SUBTITLE",
            "playback",
            error.to_string(),
            false,
        )
    })?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(webvtt))
        .map_err(|error| ApiError::internal("playback", error))
}

fn srt_to_webvtt(srt: &str) -> Result<String> {
    let normalized = srt
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    for block in normalized.split("\n\n") {
        let mut lines = block
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty());
        let Some(first) = lines.next() else {
            continue;
        };
        let timing = if first.chars().all(|character| character.is_ascii_digit()) {
            let Some(timing) = lines.next() else {
                continue;
            };
            timing
        } else {
            first
        };
        let Some((start, end_and_settings)) = timing.split_once("-->") else {
            continue;
        };
        let start = start.trim().replace(',', ".");
        let mut end_parts = end_and_settings.split_whitespace();
        let Some(end) = end_parts.next() else {
            continue;
        };
        let end = end.replace(',', ".");
        if !valid_webvtt_timestamp(&start) || !valid_webvtt_timestamp(&end) {
            continue;
        }
        let text = lines.collect::<Vec<_>>().join("\n");
        if text.trim().is_empty() {
            continue;
        }
        let text = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let settings = end_parts.collect::<Vec<_>>().join(" ");
        let timing = if settings.is_empty() {
            format!("{start} --> {end}")
        } else {
            format!("{start} --> {end} {settings}")
        };
        cues.push(format!("{timing}\n{text}"));
    }
    anyhow::ensure!(
        !cues.is_empty(),
        "The SRT subtitle contained no usable dialogue cues."
    );
    Ok(format!("WEBVTT\n\n{}\n\n", cues.join("\n\n")))
}

fn valid_webvtt_timestamp(value: &str) -> bool {
    let mut parts = value.split(':');
    let Some(hours) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    let Some(minutes) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    let Some(seconds) = parts.next().and_then(|part| part.parse::<f64>().ok()) else {
        return false;
    };
    parts.next().is_none() && hours <= 999 && minutes < 60 && (0.0..60.0).contains(&seconds)
}

fn ass_to_webvtt(ass: &str) -> Result<String> {
    let mut in_events = false;
    let mut columns = Vec::new();
    let mut cues = Vec::new();
    for raw_line in ass.trim_start_matches('\u{feff}').lines() {
        let line = raw_line.trim_end_matches('\r').trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(format) = line.strip_prefix("Format:") {
            columns = format
                .split(',')
                .map(|column| column.trim().to_ascii_lowercase())
                .collect();
            continue;
        }
        let Some(dialogue) = line.strip_prefix("Dialogue:") else {
            continue;
        };
        if columns.is_empty() {
            continue;
        }
        let values = dialogue
            .splitn(columns.len(), ',')
            .map(str::trim)
            .collect::<Vec<_>>();
        if values.len() != columns.len() {
            continue;
        }
        let value = |name: &str| {
            columns
                .iter()
                .position(|column| column == name)
                .and_then(|index| values.get(index).copied())
        };
        let (Some(start), Some(end), Some(text)) = (value("start"), value("end"), value("text"))
        else {
            continue;
        };
        let (Some(start), Some(end)) =
            (ass_timestamp_to_webvtt(start), ass_timestamp_to_webvtt(end))
        else {
            continue;
        };
        let text = ass_text_to_webvtt(text);
        if !text.trim().is_empty() {
            cues.push(format!("{start} --> {end}\n{text}"));
        }
    }
    anyhow::ensure!(
        !cues.is_empty(),
        "The ASS subtitle contained no usable dialogue cues."
    );
    Ok(format!("WEBVTT\n\n{}\n", cues.join("\n\n")))
}

fn ass_timestamp_to_webvtt(value: &str) -> Option<String> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<u32>().ok()?;
    let minutes = parts.next()?.parse::<u32>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() || minutes >= 60 || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    let total_milliseconds =
        ((hours * 3_600 + minutes * 60) as f64 * 1_000.0 + seconds * 1_000.0).round() as u64;
    Some(format!(
        "{:02}:{:02}:{:02}.{:03}",
        total_milliseconds / 3_600_000,
        total_milliseconds / 60_000 % 60,
        total_milliseconds / 1_000 % 60,
        total_milliseconds % 1_000
    ))
}

fn ass_text_to_webvtt(value: &str) -> String {
    let mut plain = String::with_capacity(value.len());
    let mut in_override = false;
    for character in value.chars() {
        match character {
            '{' => in_override = true,
            '}' if in_override => in_override = false,
            _ if !in_override => plain.push(character),
            _ => {}
        }
    }
    plain
        .replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn rewrite_hls_manifest(
    session_id: &str,
    session: &MediaSession,
    base: &Url,
    manifest: &str,
) -> String {
    manifest
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            if !trimmed.starts_with('#') {
                return base
                    .join(trimmed)
                    .map(|mut url| {
                        if url.query().is_none() {
                            url.set_query(base.query());
                        }
                        url
                    })
                    .map(|url| opaque_resource_url(session_id, session, url, false))
                    .unwrap_or_else(|_| line.to_string());
            }
            if let Some(uri) = quoted_attribute(trimmed, "URI") {
                if let Some(url) = join_preserving_query(base, &uri) {
                    return line.replacen(
                        &format!("URI=\"{uri}\""),
                        &format!(
                            "URI=\"{}\"",
                            opaque_resource_url(session_id, session, url, false)
                        ),
                        1,
                    );
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rewrite_dash_manifest(
    session_id: &str,
    session: &MediaSession,
    manifest_url: &Url,
    manifest: &str,
) -> String {
    let mut output = String::with_capacity(manifest.len() + 128);
    let mut remaining = manifest;
    let mut found_base = false;
    let mut resource_base = manifest_url.clone();
    while let Some(start) = remaining.find("<BaseURL") {
        let Some(open_end_relative) = remaining[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_relative + 1;
        let Some(close_relative) = remaining[open_end..].find("</BaseURL>") else {
            break;
        };
        let close = open_end + close_relative;
        output.push_str(&remaining[..open_end]);
        let original = remaining[open_end..close].trim();
        let absolute =
            join_preserving_query(manifest_url, original).unwrap_or_else(|| manifest_url.clone());
        if !found_base {
            resource_base = absolute.clone();
        }
        output.push_str(&opaque_resource_url(session_id, session, absolute, true));
        output.push('/');
        output.push_str("</BaseURL>");
        remaining = &remaining[close + "</BaseURL>".len()..];
        found_base = true;
    }
    output.push_str(remaining);
    if found_base {
        return rewrite_dash_resource_attributes(session_id, session, &resource_base, &output);
    } else if let Some(mpd_start) = output.find("<MPD") {
        if let Some(relative_end) = output[mpd_start..].find('>') {
            let insert_at = mpd_start + relative_end + 1;
            let parent =
                join_preserving_query(manifest_url, ".").unwrap_or_else(|| manifest_url.clone());
            output.insert_str(
                insert_at,
                &format!(
                    "<BaseURL>{}/</BaseURL>",
                    opaque_resource_url(session_id, session, parent, true)
                ),
            );
        }
    }
    rewrite_dash_resource_attributes(session_id, session, manifest_url, &output)
}

fn rewrite_dash_resource_attributes(
    session_id: &str,
    session: &MediaSession,
    manifest_url: &Url,
    manifest: &str,
) -> String {
    ["media", "initialization", "sourceURL", "index", "href"]
        .into_iter()
        .fold(manifest.to_string(), |manifest, attribute| {
            rewrite_dash_resource_attribute(session_id, session, manifest_url, &manifest, attribute)
        })
}

fn rewrite_dash_resource_attribute(
    session_id: &str,
    session: &MediaSession,
    manifest_url: &Url,
    manifest: &str,
    attribute: &str,
) -> String {
    let marker = format!("{attribute}=\"");
    let mut output = String::with_capacity(manifest.len());
    let mut remaining = manifest;
    while let Some(start) = remaining.find(&marker) {
        let value_start = start + marker.len();
        let Some(value_length) = remaining[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + value_length;
        let value = &remaining[value_start..value_end];
        output.push_str(&remaining[..value_start]);
        output.push_str(
            &opaque_dash_attribute_url(session_id, session, manifest_url, value)
                .unwrap_or_else(|| value.to_string()),
        );
        remaining = &remaining[value_end..];
    }
    output.push_str(remaining);
    output
}

fn opaque_dash_attribute_url(
    session_id: &str,
    session: &MediaSession,
    manifest_url: &Url,
    value: &str,
) -> Option<String> {
    if value.is_empty()
        || value.starts_with('#')
        || value.starts_with("data:")
        || value.starts_with("urn:")
        || value.starts_with("/api/media/")
    {
        return None;
    }
    let absolute = join_preserving_query(manifest_url, value)?;
    let Some(template_start) = absolute.path().find('$') else {
        return Some(opaque_resource_url(session_id, session, absolute, false));
    };
    let base_end = absolute.path()[..template_start]
        .rfind('/')
        .map(|index| index + 1)
        .unwrap_or(1);
    let template = absolute.path()[base_end..].to_string();
    let mut base = absolute;
    let base_path = base.path()[..base_end].to_owned();
    base.set_path(&base_path);
    Some(format!(
        "{}/{}",
        opaque_resource_url(session_id, session, base, true),
        template
    ))
}

fn join_preserving_query(base: &Url, value: &str) -> Option<Url> {
    let mut joined = base.join(value).ok()?;
    if joined.query().is_none() {
        joined.set_query(base.query());
    }
    Some(joined)
}

fn opaque_resource_url(
    session_id: &str,
    session: &MediaSession,
    url: Url,
    allow_relative_paths: bool,
) -> String {
    opaque_resource_url_with_transform(
        session_id,
        session,
        url,
        allow_relative_paths,
        MediaTransform::None,
    )
}

fn opaque_subtitle_url(
    session_id: &str,
    session: &MediaSession,
    url: Url,
    format: SubtitleFormat,
) -> String {
    let transform = match format {
        SubtitleFormat::Ass => MediaTransform::AssToWebVtt,
        SubtitleFormat::Srt => MediaTransform::SrtToWebVtt,
        _ => MediaTransform::None,
    };
    opaque_resource_url_with_transform(session_id, session, url, false, transform)
}

fn opaque_resource_url_with_transform(
    session_id: &str,
    session: &MediaSession,
    url: Url,
    allow_relative_paths: bool,
    transform: MediaTransform,
) -> String {
    let resource_id =
        opaque_resource_id_with_transform(&session.secret, &url, allow_relative_paths, transform);
    session
        .resources
        .lock()
        .expect("media resource map lock poisoned")
        .insert(
            resource_id.clone(),
            MediaResource {
                url,
                allow_relative_paths,
                transform,
            },
        );
    format!("/api/media/{session_id}/resource/{resource_id}")
}

#[cfg(test)]
fn opaque_resource_id(secret: &[u8; 32], url: &Url, allow_relative_paths: bool) -> String {
    opaque_resource_id_with_transform(secret, url, allow_relative_paths, MediaTransform::None)
}

fn opaque_resource_id_with_transform(
    secret: &[u8; 32],
    url: &Url,
    allow_relative_paths: bool,
    transform: MediaTransform,
) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("fixed-size HMAC key");
    mac.update(&[u8::from(allow_relative_paths)]);
    mac.update(&[match transform {
        MediaTransform::None => 0,
        MediaTransform::AssToWebVtt => 1,
        MediaTransform::SrtToWebVtt => 2,
    }]);
    mac.update(url.as_str().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn resolve_opaque_resource(
    session: &MediaSession,
    resource_id: &str,
    relative_path: Option<&str>,
) -> ApiResult<Url> {
    let resource = resolve_media_resource(session, resource_id)?;
    match relative_path {
        Some(path)
            if resource.allow_relative_paths && resource.transform == MediaTransform::None =>
        {
            resolve_dash_upstream(resource.url, path)
        }
        Some(_) => Err(invalid_media_resource()),
        None => Ok(resource.url),
    }
}

#[cfg(test)]
fn media_resource_count(session: &MediaSession) -> usize {
    session
        .resources
        .lock()
        .expect("media resource cache lock poisoned")
        .len()
}

fn resolve_media_resource(session: &MediaSession, resource_id: &str) -> ApiResult<MediaResource> {
    session
        .resources
        .lock()
        .expect("media resource map lock poisoned")
        .get(resource_id)
        .cloned()
        .ok_or_else(invalid_media_resource)
}

async fn my_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LimitQuery>,
) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    Ok(Json(json!(state
        .db
        .favorites(&user.id, query.limit.unwrap_or(100).min(500))
        .await
        .map_err(|error| ApiError::internal(
            "favorites",
            error
        ))?)))
}

async fn add_favorite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AnimeInput>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let user = require_user(&state, &headers).await?;
    let anime_id = format!("{}:{}", input.provider, input.id);
    state
        .db
        .save_favorite(
            &user.id,
            &NewFavorite {
                anime_id: &anime_id,
                catalog_id: input.catalog_id,
                provider: &input.provider,
                title: &input.title,
                cover_url: &input.cover_url,
            },
        )
        .await
        .map_err(|error| ApiError::internal("favorites", error))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_favorite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RemoveInput>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let user = require_user(&state, &headers).await?;
    state
        .db
        .remove_favorite(&user.id, &input.anime_id)
        .await
        .map_err(|error| ApiError::internal("favorites", error))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LimitQuery>,
) -> ApiResult<Json<Value>> {
    let user = require_user(&state, &headers).await?;
    Ok(Json(json!(state
        .db
        .history(&user.id, query.limit.unwrap_or(20).min(500))
        .await
        .map_err(|error| ApiError::internal("history", error))?)))
}

async fn save_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProgressInput>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let user = require_user(&state, &headers).await?;
    state
        .db
        .save_history(
            &user.id,
            &NewHistory {
                anime_id: &input.anime_id,
                catalog_id: input.catalog_id,
                provider: &input.provider,
                title: &input.title,
                cover_url: &input.cover_url,
                episode_number: input.episode_number,
                episode_title: input.episode_title.as_deref(),
                position_seconds: input.position_seconds,
                total_seconds: input.total_seconds,
            },
        )
        .await
        .map_err(|error| ApiError::internal("history", error))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RemoveInput>,
) -> ApiResult<StatusCode> {
    require_app_request(&headers)?;
    let user = require_user(&state, &headers).await?;
    state
        .db
        .remove_history(&user.id, &input.anime_id)
        .await
        .map_err(|error| ApiError::internal("history", error))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_download_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BrowserDownloadInput>,
) -> ApiResult<Json<Value>> {
    require_app_request(&headers)?;
    let user = require_user(&state, &headers).await?;
    if Uuid::parse_str(&request.id).is_err() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_DOWNLOAD",
            "download",
            "The download request identifier is invalid.",
            false,
        ));
    }
    if request.anime_id.trim().is_empty()
        || request.anime_title.trim().is_empty()
        || request.episode_id.trim().is_empty()
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_DOWNLOAD",
            "download",
            "The download request is incomplete.",
            false,
        ));
    }
    if !request.cover_url.trim().is_empty() && Url::parse(&request.cover_url).is_err() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_DOWNLOAD",
            "download",
            "The download artwork URL is invalid.",
            false,
        ));
    }
    let stream = resolve_stream(&state, &request.provider, &request.episode_id).await?;
    if stream.video_url.to_ascii_lowercase().contains(".mpd") {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DOWNLOAD_FORMAT_UNSUPPORTED",
            "download",
            "This provider uses DASH for this episode. Choose another source to download it.",
            false,
        ));
    }
    let id = Uuid::new_v4().to_string();
    let file_name = browser_download_file_name(&request, &stream);
    let mut tickets = state.download_tickets.lock().await;
    let now = Instant::now();
    tickets.retain(|_, ticket| ticket.expires_at > now);
    tickets.insert(
        id.clone(),
        DownloadTicket {
            user_id: user.id,
            expires_at: now + Duration::from_secs(5 * 60),
            request,
            stream,
        },
    );
    Ok(Json(
        json!({ "id": id, "url": format!("/api/downloads/{id}"), "fileName": file_name }),
    ))
}

async fn browser_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let ticket = state
        .download_tickets
        .lock()
        .await
        .remove(&id)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "DOWNLOAD_TICKET_EXPIRED",
                "download",
                "This download link expired. Start the download again.",
                false,
            )
        })?;
    if ticket.user_id != user.id || ticket.expires_at <= Instant::now() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "DOWNLOAD_TICKET_EXPIRED",
            "download",
            "This download link is no longer valid.",
            false,
        ));
    }
    proxy_download_response(&state.media_client, ticket.stream, &ticket.request).await
}

async fn proxy_download_response(
    client: &Client,
    stream: StreamInfo,
    request: &BrowserDownloadInput,
) -> ApiResult<Response> {
    let source = Url::parse(&stream.video_url).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "INVALID_STREAM",
            "download",
            error.to_string(),
            false,
        )
    })?;
    let upstream_headers = stream_headers(&stream)?;
    let is_hls = source.path().to_ascii_lowercase().contains(".m3u8");
    let file_name = browser_download_file_name(request, &stream);

    let body = if is_hls {
        let segments = if stream.use_curl {
            resolve_hls_segments_via_curl(&stream.headers, source).await?
        } else {
            resolve_hls_segments(client, &upstream_headers, source).await?
        };
        if stream.use_curl {
            Body::from_stream(hls_body_stream_via_curl(stream.headers.clone(), segments))
        } else {
            Body::from_stream(hls_body_stream(
                client.clone(),
                upstream_headers.clone(),
                segments,
            ))
        }
    } else if stream.use_curl {
        let (status, _, body) = curl_fetch(&source, &stream.headers, None, 900).await?;
        if status != 200 {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "DOWNLOAD_FAILED",
                "download",
                format!("upstream returned HTTP {status}"),
                true,
            ));
        }
        Body::from(body)
    } else {
        let response = client
            .get(source.clone())
            .headers(upstream_headers)
            .send()
            .await
            .map_err(|_| download_proxy_failure(&source, "request"))?
            .error_for_status()
            .map_err(|_| download_proxy_failure(&source, "status"))?;
        Body::from_stream(response.bytes_stream().map_err(std::io::Error::other))
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            if is_hls {
                "video/mp2t"
            } else {
                "application/octet-stream"
            },
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"",
                file_name.replace(['\"', '\\'], "_")
            ),
        )
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(body)
        .map_err(|error| ApiError::internal("download", error))
}

async fn resolve_hls_segments(
    client: &Client,
    headers: &ReqwestHeaderMap,
    source: Url,
) -> ApiResult<Vec<Url>> {
    let master = fetch_text(client, headers, source.clone()).await?;
    let media_url = highest_bandwidth_variant(&source, &master).unwrap_or(source.clone());
    let media = if media_url == source {
        master
    } else {
        fetch_text(client, headers, media_url.clone()).await?
    };
    parse_hls_segments(&media_url, &media)
}

async fn resolve_hls_segments_via_curl(
    headers: &std::collections::HashMap<String, String>,
    source: Url,
) -> ApiResult<Vec<Url>> {
    let (status, _, master) = curl_fetch(&source, headers, None, 900).await?;
    if status != 200 {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "DOWNLOAD_FAILED",
            "download",
            format!("upstream returned HTTP {status}"),
            true,
        ));
    }
    let master = String::from_utf8_lossy(&master).into_owned();
    let media_url = highest_bandwidth_variant(&source, &master).unwrap_or(source.clone());
    let media = if media_url == source {
        master
    } else {
        let (status, _, body) = curl_fetch(&media_url, headers, None, 900).await?;
        if status != 200 {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "DOWNLOAD_FAILED",
                "download",
                format!("upstream returned HTTP {status}"),
                true,
            ));
        }
        String::from_utf8_lossy(&body).into_owned()
    };
    parse_hls_segments(&media_url, &media)
}

fn parse_hls_segments(media_url: &Url, media: &str) -> ApiResult<Vec<Url>> {
    if media.lines().any(|line| {
        let line = line.trim().to_ascii_uppercase();
        line.starts_with("#EXT-X-KEY") && !line.contains("METHOD=NONE")
    }) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DOWNLOAD_FORMAT_UNSUPPORTED",
            "download",
            "This provider encrypts the HLS download. Choose another source for this episode.",
            false,
        ));
    }
    if media
        .lines()
        .any(|line| line.trim().starts_with("#EXT-X-BYTERANGE"))
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DOWNLOAD_FORMAT_UNSUPPORTED",
            "download",
            "This provider uses byte-range HLS downloads. Choose another source for this episode.",
            false,
        ));
    }
    let mut segments = Vec::new();
    for line in media.lines() {
        let value = line.trim();
        if value.starts_with("#EXT-X-MAP:") {
            if let Some(uri) = quoted_attribute(value, "URI") {
                if let Some(url) = join_preserving_query(media_url, &uri) {
                    segments.push(url);
                }
            }
        } else if !value.is_empty() && !value.starts_with('#') {
            if let Some(url) = join_preserving_query(media_url, value) {
                segments.push(url);
            }
        }
    }
    if segments.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "DOWNLOAD_FAILED",
            "download",
            "The provider returned an empty media playlist.",
            true,
        ));
    }
    Ok(segments)
}

fn hls_body_stream(
    client: Client,
    headers: ReqwestHeaderMap,
    segments: Vec<Url>,
) -> impl futures_util::Stream<Item = std::result::Result<Bytes, std::io::Error>> {
    async_stream::try_stream! {
        for segment in segments {
            let mut response = client.get(segment).headers(headers.clone()).send().await
                .map_err(|_| std::io::Error::other("upstream segment request failed"))?
                .error_for_status()
                .map_err(|_| std::io::Error::other("upstream segment returned an error"))?;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| std::io::Error::other("upstream segment body failed"))?
            {
                yield chunk;
            }
        }
    }
}

fn hls_body_stream_via_curl(
    headers: std::collections::HashMap<String, String>,
    segments: Vec<Url>,
) -> impl futures_util::Stream<Item = std::result::Result<Bytes, std::io::Error>> {
    async_stream::try_stream! {
        for segment in segments {
            let (status, _, body) = curl_fetch(&segment, &headers, None, 900)
                .await
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
            if status != 200 {
                Err(std::io::Error::other(format!(
                    "upstream returned HTTP {status}"
                )))?;
            }
            yield Bytes::from(body);
        }
    }
}

async fn fetch_text(client: &Client, headers: &ReqwestHeaderMap, url: Url) -> ApiResult<String> {
    client
        .get(url.clone())
        .headers(headers.clone())
        .send()
        .await
        .map_err(|_| download_proxy_failure(&url, "playlist-request"))?
        .error_for_status()
        .map_err(|_| download_proxy_failure(&url, "playlist-status"))?
        .text()
        .await
        .map_err(|_| download_proxy_failure(&url, "playlist-body"))
}

fn highest_bandwidth_variant(base: &Url, playlist: &str) -> Option<Url> {
    let lines = playlist.lines().collect::<Vec<_>>();
    let mut variants = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.trim().starts_with("#EXT-X-STREAM-INF:") {
            continue;
        }
        let bandwidth = line
            .split("BANDWIDTH=")
            .nth(1)
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        if let Some(path) = lines
            .get(index + 1)
            .map(|value| value.trim())
            .filter(|value| !value.starts_with('#') && !value.is_empty())
        {
            if let Ok(url) = base.join(path) {
                variants.push((bandwidth, url));
            }
        }
    }
    variants
        .into_iter()
        .max_by_key(|value| value.0)
        .map(|value| value.1)
}

fn quoted_attribute(line: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('\"')? + start;
    Some(line[start..end].to_string())
}

fn stream_headers(stream: &StreamInfo) -> ApiResult<ReqwestHeaderMap> {
    let mut headers = ReqwestHeaderMap::new();
    for (name, value) in &stream.headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "INVALID_STREAM",
                "download",
                error.to_string(),
                false,
            )
        })?;
        let value = reqwest::header::HeaderValue::from_str(value).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "INVALID_STREAM",
                "download",
                error.to_string(),
                false,
            )
        })?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn browser_download_file_name(request: &BrowserDownloadInput, stream: &StreamInfo) -> String {
    let title = request
        .episode_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !is_generic_episode_title(value, request.episode_number));
    let stem = title
        .map(|value| format!("E{:02} - {value}", request.episode_number))
        .unwrap_or_else(|| format!("Episode {:02}", request.episode_number));
    let source_path = Url::parse(&stream.video_url)
        .ok()
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = if source_path.contains(".m3u8") {
        "ts"
    } else {
        source_path
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .filter(|extension| matches!(*extension, "mp4" | "m4v" | "mkv" | "webm" | "mov"))
            .unwrap_or("mp4")
    };
    format!(
        "{} - {}.{extension}",
        sanitize_file_name(&request.anime_title),
        sanitize_file_name(&stem)
    )
}

fn is_generic_episode_title(title: &str, episode_number: u32) -> bool {
    title.eq_ignore_ascii_case(&format!("Episode {episode_number}"))
        || title.eq_ignore_ascii_case(&format!("Episode {episode_number:02}"))
}

fn sanitize_file_name(value: &str) -> String {
    let clean = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(['.', ' '])
        .chars()
        .take(100)
        .collect::<String>();
    if clean.is_empty() {
        "any-watch".into()
    } else {
        clean
    }
}

async fn resolve_stream(
    state: &AppState,
    provider: &str,
    episode_id: &str,
) -> ApiResult<StreamInfo> {
    let provider_ref = state.providers.get_provider(provider).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "PROVIDER_UNAVAILABLE",
            "stream",
            "Provider is not available.",
            false,
        )
    })?;
    provider_ref
        .get_stream_url(episode_id)
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                classify_provider_error(&error.to_string()),
                "stream",
                "This provider could not prepare the episode stream.",
                true,
            )
        })
}

async fn require_user(state: &AppState, headers: &HeaderMap) -> ApiResult<SessionUser> {
    let token = cookie_value(headers, SESSION_COOKIE).ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "AUTH_REQUIRED",
            "auth",
            "Sign in to continue.",
            false,
        )
    })?;
    state
        .db
        .session_user(&token)
        .await
        .map_err(|error| ApiError::internal("auth", error))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "SESSION_EXPIRED",
                "auth",
                "Your session expired. Sign in again.",
                false,
            )
        })
}

async fn require_admin(state: &AppState, headers: &HeaderMap) -> ApiResult<SessionUser> {
    let user = require_user(state, headers).await?;
    if user.role != "admin" {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "ADMIN_REQUIRED",
            "admin",
            "Administrator access is required.",
            false,
        ));
    }
    Ok(user)
}

fn require_app_request(headers: &HeaderMap) -> ApiResult<()> {
    if headers
        .get("x-any-watch-request")
        .and_then(|value| value.to_str().ok())
        != Some("1")
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "REQUEST_VERIFICATION_FAILED",
            "security",
            "The request could not be verified.",
            false,
        ));
    }
    Ok(())
}

async fn allow_login_attempt(state: &AppState, key: &str) -> bool {
    let now = Instant::now();
    let mut attempts = state.login_attempts.lock().await;
    attempts.retain(|_, values| {
        values.retain(|value| now.duration_since(*value) < LOGIN_ATTEMPT_WINDOW);
        !values.is_empty()
    });

    if !attempts.contains_key(key) && attempts.len() >= LOGIN_ATTEMPT_KEY_LIMIT {
        if let Some(oldest_key) = attempts
            .iter()
            .min_by_key(|(_, values)| values.last().copied())
            .map(|(key, _)| key.clone())
        {
            attempts.remove(&oldest_key);
        }
    }

    let values = attempts.entry(key.into()).or_default();
    if values.len() >= LOGIN_ATTEMPT_LIMIT {
        return false;
    }
    values.push(now);
    true
}

fn client_identity(headers: &HeaderMap) -> String {
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .trim()
        .chars()
        .take(80)
        .collect()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_string())
        })
}

fn session_cookie(token: &str, secure: bool, max_age: u64) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

fn language_label(language: Language) -> &'static str {
    match language {
        Language::English => "English",
        Language::Vietnamese => "Vietnamese",
        Language::Youtube => "YouTube",
    }
}
fn language_group(language: Language) -> &'static str {
    match language {
        Language::English => "english",
        Language::Vietnamese => "vietnamese",
        Language::Youtube => "youtube",
    }
}

fn map_anime(anime: Anime, catalog_id: Option<i64>) -> AnimeDto {
    AnimeDto {
        id: anime.id,
        catalog_id,
        provider: anime.provider,
        title: anime.title,
        cover_url: anime.cover_url,
        banner_url: anime.banner_url,
        language: language_label(anime.language).into(),
        total_episodes: anime.total_episodes,
        synopsis: anime.synopsis,
        is_favorite: false,
    }
}

fn classify_provider_error(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("captcha") || lower.contains("cloudflare") {
        "PROVIDER_CAPTCHA"
    } else if lower.contains("403") || lower.contains("forbidden") {
        "STREAM_FORBIDDEN"
    } else if lower.contains("429") {
        "PROVIDER_RATE_LIMITED"
    } else {
        "PROVIDER_UNAVAILABLE"
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    const UPSTREAM_HOST: &str = "private-cdn.example";
    const SIGNED_VALUE: &str = "signed-query-secret";
    const COOKIE_VALUE: &str = "upstream_session=private-cookie";
    const REQUIRED_HEADER_VALUE: &str = "required-header-secret";

    fn media_session(video_url: &str) -> MediaSession {
        let mut stream = stream(video_url);
        stream.subtitles.push(any_watch_core::providers::Subtitle {
            language: "English".into(),
            url: format!("https://{UPSTREAM_HOST}/subtitles/en.vtt?token={SIGNED_VALUE}"),
            format: SubtitleFormat::WebVtt,
        });
        stream.headers.insert("Cookie".into(), COOKIE_VALUE.into());
        stream
            .headers
            .insert("X-Required-Auth".into(), REQUIRED_HEADER_VALUE.into());
        MediaSession {
            user_id: "user".into(),
            expires_at: Instant::now() + Duration::from_secs(60),
            stream,
            secret: [7_u8; 32],
            resources: Arc::new(StdMutex::new(MediaResourceCache::default())),
        }
    }

    fn assert_private_material_absent(value: &str, urls: &[&str]) {
        assert!(
            !value.contains(UPSTREAM_HOST),
            "upstream hostname leaked: {value}"
        );
        assert!(
            !value.contains(SIGNED_VALUE),
            "signed value leaked: {value}"
        );
        assert!(!value.contains(COOKIE_VALUE), "cookie leaked: {value}");
        assert!(
            !value.contains(REQUIRED_HEADER_VALUE),
            "required header leaked: {value}"
        );
        for url in urls {
            let percent_encoded =
                url::form_urlencoded::byte_serialize(url.as_bytes()).collect::<String>();
            let base64 = base64::engine::general_purpose::STANDARD.encode(url);
            let url_safe = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url);
            assert!(!value.contains(url), "upstream URL leaked: {value}");
            assert!(
                !value.contains(&percent_encoded),
                "percent-encoded upstream URL leaked: {value}"
            );
            assert!(
                !value.contains(&base64),
                "base64 upstream URL leaked: {value}"
            );
            assert!(
                !value.contains(&url_safe),
                "URL-safe base64 upstream URL leaked: {value}"
            );
        }
    }

    fn resource_id(resource_url: &str) -> &str {
        resource_url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
    }

    fn download_request() -> BrowserDownloadInput {
        BrowserDownloadInput {
            id: Uuid::new_v4().to_string(),
            provider: "AllAnime".into(),
            anime_id: "one-piece".into(),
            episode_id: "one-piece-1163".into(),
            anime_title: "One Piece".into(),
            cover_url: String::new(),
            episode_number: 1163,
            episode_title: Some("Episode 1163".into()),
        }
    }

    fn stream(video_url: &str) -> StreamInfo {
        StreamInfo {
            video_url: video_url.into(),
            subtitles: Vec::new(),
            qualities: Vec::new(),
            headers: HashMap::new(),
            use_curl: false,
        }
    }

    #[test]
    fn browser_download_uses_the_actual_direct_media_extension() {
        let request = download_request();
        assert_eq!(
            browser_download_file_name(&request, &stream("https://cdn.example/video.webm?x=1")),
            "One Piece - Episode 1163.webm"
        );
        assert_eq!(
            browser_download_file_name(&request, &stream("https://cdn.example/master.m3u8")),
            "One Piece - Episode 1163.ts"
        );
    }

    #[test]
    fn browser_download_omits_padded_generic_episode_titles() {
        let mut request = download_request();
        request.episode_number = 1;
        request.episode_title = Some("Episode 01".into());
        assert_eq!(
            browser_download_file_name(&request, &stream("https://cdn.example/video.mp4")),
            "One Piece - Episode 01.mp4"
        );
    }

    #[test]
    fn curl_metadata_is_removed_without_changing_binary_media() {
        let media = (0_u8..=255).cycle().take(262_144).collect::<Vec<_>>();
        let mut output = media.clone();
        output.extend_from_slice(b"\n__ANY_WATCH__206__video/mp2t");

        let (body, status, content_type) = parse_curl_output(output);

        assert_eq!(body, media);
        assert_eq!(status, 206);
        assert_eq!(content_type, "video/mp2t");
    }

    #[test]
    fn curl_metadata_uses_the_final_marker_in_binary_media() {
        let media = b"segment\n__ANY_WATCH__bytes".to_vec();
        let mut output = media.clone();
        output.extend_from_slice(b"\n__ANY_WATCH__200__video/mp4");

        let (body, status, content_type) = parse_curl_output(output);

        assert_eq!(body, media);
        assert_eq!(status, 200);
        assert_eq!(content_type, "video/mp4");
    }

    #[tokio::test]
    async fn limited_async_reader_rejects_oversized_curl_payloads() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, reader) = tokio::io::duplex(64);
        let write = tokio::spawn(async move {
            writer.write_all(&[b'x'; 33]).await.unwrap();
            writer.shutdown().await.unwrap();
        });

        let result = read_limited_async(reader, 32).await;

        write.await.unwrap();
        assert_eq!(result, Err(ResponseBodyError::TooLarge));
    }

    #[test]
    fn curl_manifests_reject_bodies_over_manifest_limit() {
        let body = vec![b'x'; MAX_MEDIA_MANIFEST_BYTES + 1];

        assert_eq!(
            enforce_body_limit(body, MAX_MEDIA_MANIFEST_BYTES),
            Err(ResponseBodyError::TooLarge)
        );
    }

    #[test]
    fn download_proxy_errors_do_not_expose_upstream_urls_or_tokens() {
        let url = Url::parse(&format!(
            "https://{UPSTREAM_HOST}/show/master.m3u8?token={SIGNED_VALUE}"
        ))
        .unwrap();
        let error = download_proxy_failure(&url, "request");
        let json = serde_json::to_string(&error.1).unwrap();

        assert_private_material_absent(&json, &[url.as_str()]);
        assert_eq!(
            error.1.message,
            "The upstream download is temporarily unavailable."
        );
    }

    #[test]
    fn media_proxy_errors_do_not_expose_upstream_urls_or_tokens() {
        let url = Url::parse(&format!(
            "https://{UPSTREAM_HOST}/show/master.m3u8?token={SIGNED_VALUE}"
        ))
        .unwrap();
        let error = media_proxy_failure("session", &url, "connect");
        let json = serde_json::to_string(&error.1).unwrap();

        assert_private_material_absent(&json, &[url.as_str()]);
        assert_eq!(
            error.1.message,
            "The upstream media resource is temporarily unavailable."
        );
    }

    #[test]
    fn playback_json_exposes_only_opaque_same_origin_resources() {
        let video_url = format!("https://{UPSTREAM_HOST}/show/master.m3u8?token={SIGNED_VALUE}");
        let subtitle_url = format!("https://{UPSTREAM_HOST}/subtitles/en.vtt?token={SIGNED_VALUE}");
        let session = media_session(&video_url);
        let json = serde_json::to_string(&playback_dto("session", &session)).unwrap();

        assert_private_material_absent(&json, &[&video_url, &subtitle_url]);
        let value: Value = serde_json::from_str(&json).unwrap();
        let resource_url = value["subtitles"][0]["url"].as_str().unwrap();
        assert!(resource_url.starts_with("/api/media/session/resource/"));
        assert!(!resource_url.contains('?'));
        assert_eq!(resource_id(resource_url).len(), 64);
        assert!(resource_id(resource_url)
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            resource_id(resource_url),
            opaque_resource_id(&[7_u8; 32], &Url::parse(&subtitle_url).unwrap(), false)
        );
        assert_eq!(
            resolve_opaque_resource(&session, resource_id(resource_url), None)
                .unwrap()
                .as_str(),
            subtitle_url
        );
    }

    #[test]
    fn playback_classifies_invidious_extensionless_dash_manifest() {
        let session =
            media_session("https://invidious.example/api/manifest/dash/id/abcdefghijk?local=true");
        let value = serde_json::to_value(playback_dto("session", &session)).unwrap();

        assert_eq!(value["streamKind"], "dash");
    }

    #[test]
    fn hls_download_segments_inherit_playlist_authorization_query() {
        let media_url =
            Url::parse("https://private-cdn.example/show/media.m3u8?token=signed-query-secret")
                .unwrap();
        let segments = parse_hls_segments(
            &media_url,
            "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\nsegment-1.ts\nsegment-2.ts?part=2\n",
        )
        .unwrap();

        assert_eq!(
            segments[0].as_str(),
            "https://private-cdn.example/show/init.mp4?token=signed-query-secret"
        );
        assert_eq!(
            segments[1].as_str(),
            "https://private-cdn.example/show/segment-1.ts?token=signed-query-secret"
        );
        assert_eq!(
            segments[2].as_str(),
            "https://private-cdn.example/show/segment-2.ts?part=2"
        );
    }

    #[test]
    fn hls_rewrite_hides_private_material_and_resolves_opaque_resources() {
        let manifest_url = Url::parse(&format!(
            "https://{UPSTREAM_HOST}/show/master.m3u8?token={SIGNED_VALUE}"
        ))
        .unwrap();
        let absolute_segment =
            format!("https://{UPSTREAM_HOST}/show/absolute.ts?token={SIGNED_VALUE}");
        let session = media_session(manifest_url.as_str());
        let manifest = format!(
            "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"keys/key.bin\"\nsegment.ts\n{absolute_segment}"
        );
        let rewritten = rewrite_hls_manifest("session", &session, &manifest_url, &manifest);

        assert_private_material_absent(&rewritten, &[manifest_url.as_str(), &absolute_segment]);
        let resource_urls = rewritten
            .split(['\n', '"'])
            .filter(|value| value.starts_with("/api/media/session/resource/"))
            .collect::<Vec<_>>();
        assert_eq!(resource_urls.len(), 3);
        assert!(resource_urls.iter().all(|url| !url.contains('?')));
        assert_eq!(
            resolve_opaque_resource(&session, resource_id(resource_urls[1]), None)
                .unwrap()
                .as_str(),
            format!("https://{UPSTREAM_HOST}/show/segment.ts?token={SIGNED_VALUE}")
        );
    }

    #[test]
    fn dash_rewrite_hides_private_material_and_resolves_opaque_paths() {
        let manifest_url = Url::parse(&format!(
            "https://{UPSTREAM_HOST}/show/manifest.mpd?token={SIGNED_VALUE}"
        ))
        .unwrap();
        let upstream_base = format!("https://{UPSTREAM_HOST}/show/video/?token={SIGNED_VALUE}");
        let session = media_session(manifest_url.as_str());
        let with_base = rewrite_dash_manifest(
            "session",
            &session,
            &manifest_url,
            &format!(
                "<MPD><Period><BaseURL>{upstream_base}</BaseURL><SegmentTemplate media=\"https://{UPSTREAM_HOST}/show/chunks/$Number$.m4s?token={SIGNED_VALUE}\" initialization=\"init.mp4\" /></Period></MPD>"
            ),
        );
        assert_private_material_absent(&with_base, &[manifest_url.as_str(), &upstream_base]);
        assert!(with_base.contains("<BaseURL>/api/media/session/resource/"));
        assert!(!with_base.contains('?'));
        assert!(with_base.contains("media=\"/api/media/session/resource/"));
        assert!(with_base.contains("/$Number$.m4s\""));
        assert!(with_base.contains("initialization=\"/api/media/session/resource/"));
        let media_url = quoted_attribute(&with_base, "media").unwrap();
        let (media_base, _) = media_url.rsplit_once('/').unwrap();
        assert_eq!(
            resolve_opaque_resource(&session, resource_id(media_base), Some("1.m4s"))
                .unwrap()
                .as_str(),
            format!("https://{UPSTREAM_HOST}/show/chunks/1.m4s?token={SIGNED_VALUE}")
        );
        let base_url = with_base
            .split("<BaseURL>")
            .nth(1)
            .unwrap()
            .split("</BaseURL>")
            .next()
            .unwrap();
        assert_eq!(
            resolve_opaque_resource(&session, resource_id(base_url), Some("segments/1.m4s"))
                .unwrap()
                .as_str(),
            format!("https://{UPSTREAM_HOST}/show/video/segments/1.m4s?token={SIGNED_VALUE}")
        );

        let without_base =
            rewrite_dash_manifest("session", &session, &manifest_url, "<MPD><Period /></MPD>");
        assert_private_material_absent(&without_base, &[manifest_url.as_str()]);
        assert!(without_base.starts_with("<MPD><BaseURL>/api/media/session/resource/"));
    }

    #[test]
    fn media_resource_cache_evicts_old_entries_at_its_limit() {
        let session = media_session("https://cdn.example/master.m3u8");
        for index in 0..=MAX_MEDIA_RESOURCES_PER_SESSION {
            let url = Url::parse(&format!("https://cdn.example/segment-{index}.ts")).unwrap();
            opaque_resource_url("session", &session, url, false);
        }

        assert_eq!(
            media_resource_count(&session),
            MAX_MEDIA_RESOURCES_PER_SESSION
        );
        let oldest = Url::parse("https://cdn.example/segment-0.ts").unwrap();
        let oldest_id = opaque_resource_id(&session.secret, &oldest, false);
        assert!(resolve_media_resource(&session, &oldest_id).is_err());
    }

    #[test]
    fn opaque_resources_reject_unknown_ids_and_cross_origin_paths() {
        let base = Url::parse(&format!(
            "https://{UPSTREAM_HOST}/show/?token={SIGNED_VALUE}"
        ))
        .unwrap();
        let session = media_session(base.as_str());
        let resource_url = opaque_resource_url("session", &session, base, true);

        assert!(resolve_opaque_resource(&session, "0", None).is_err());
        assert!(resolve_opaque_resource(
            &session,
            resource_id(&resource_url),
            Some("https://attacker.example/1.m4s")
        )
        .is_err());
    }

    #[test]
    fn opaque_proxy_resources_retain_required_upstream_headers() {
        let session = media_session(&format!(
            "https://{UPSTREAM_HOST}/show/master.m3u8?token={SIGNED_VALUE}"
        ));
        let headers = stream_headers(&session.stream).unwrap();

        assert_eq!(headers.get(reqwest::header::COOKIE).unwrap(), COOKIE_VALUE);
        assert_eq!(
            headers.get("x-required-auth").unwrap(),
            REQUIRED_HEADER_VALUE
        );
    }

    #[test]
    fn converts_ass_dialogue_to_browser_native_webvtt() {
        let ass = "\u{feff}[Script Info]\r\nTitle: Test\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:01:02.34,0:01:05.60,Default,,0,0,0,,{\\an8}Hello, world!\\NSecond <line>\r\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,Ignored\r\n";
        let output = ass_to_webvtt(ass).unwrap();

        assert_eq!(
            output,
            "WEBVTT\n\n00:01:02.340 --> 00:01:05.600\nHello, world!\nSecond &lt;line&gt;\n"
        );
    }

    #[test]
    fn converts_srt_to_browser_native_webvtt() {
        let srt = "1\r\n00:01:02,340 --> 00:01:05,600\r\nHello, world!\r\nSecond line\r\n\r\n";
        let output = srt_to_webvtt(srt).unwrap();

        assert_eq!(
            output,
            "WEBVTT\n\n00:01:02.340 --> 00:01:05.600\nHello, world!\nSecond line\n\n"
        );
    }

    #[test]
    fn transformed_subtitles_use_distinct_opaque_resources() {
        let session = media_session("https://cdn.example/master.m3u8");
        let url = Url::parse("https://cdn.example/subtitle.ass").unwrap();
        let raw = opaque_resource_url("session", &session, url.clone(), false);
        let converted = opaque_subtitle_url("session", &session, url, SubtitleFormat::Ass);

        assert_ne!(resource_id(&raw), resource_id(&converted));
        assert_eq!(
            resolve_media_resource(&session, resource_id(&converted))
                .unwrap()
                .transform,
            MediaTransform::AssToWebVtt
        );
    }

    #[tokio::test]
    async fn limited_response_body_rejects_chunked_oversized_payloads() {
        use std::convert::Infallible;

        let app = Router::new().route(
            "/",
            get(|| async {
                let chunks = stream::iter([
                    Ok::<_, Infallible>(Bytes::from_static(b"12345678")),
                    Ok::<_, Infallible>(Bytes::from_static(b"abcdefgh")),
                ]);
                Response::builder().body(Body::from_stream(chunks)).unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let response = Client::new()
            .get(format!("http://{address}/"))
            .send()
            .await
            .unwrap();

        let error = read_limited_response_body(response, 12).await.unwrap_err();
        server.abort();

        assert_eq!(error, ResponseBodyError::TooLarge);
    }

    #[tokio::test]
    async fn provider_health_checks_use_bounded_concurrency() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let checks = (0..PROVIDER_HEALTH_CONCURRENCY * 3).map(|index| {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                index
            }
        });

        let results = run_provider_health_checks(checks).await;

        assert_eq!(
            results,
            (0..PROVIDER_HEALTH_CONCURRENCY * 3).collect::<Vec<_>>()
        );
        assert_eq!(maximum.load(Ordering::SeqCst), PROVIDER_HEALTH_CONCURRENCY);
    }

    #[tokio::test]
    async fn concurrent_provider_health_retries_share_one_refresh() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(Mutex::new(ProviderHealthCache::default()));
        let refresh = Arc::new(Mutex::new(()));
        let checks = Arc::new(AtomicUsize::new(0));
        let retry = || {
            let checks = Arc::clone(&checks);
            async move {
                checks.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(vec![SourceDto {
                    name: "Invidious".into(),
                    language: "YouTube".into(),
                    language_group: "YouTube".into(),
                    status: "healthy".into(),
                    failure_code: None,
                    capabilities: any_watch_core::providers::ProviderCapabilities::default(),
                    website_url: None,
                }])
            }
        };

        let first = refresh_provider_health_coalesced(
            cache.as_ref(),
            refresh.as_ref(),
            Some("Invidious"),
            retry(),
        );
        let second = refresh_provider_health_coalesced(
            cache.as_ref(),
            refresh.as_ref(),
            Some("Invidious"),
            retry(),
        );
        let (first, second) = tokio::join!(first, second);

        assert_eq!(checks.load(Ordering::SeqCst), 1);
        assert_eq!(first.unwrap()[0].name, "Invidious");
        assert_eq!(second.unwrap()[0].name, "Invidious");
    }

    #[test]
    fn client_identity_uses_railways_canonical_client_ip_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.7"));
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.9, 192.0.2.4"),
        );

        assert_eq!(client_identity(&headers), "203.0.113.7");
    }
}
