mod db;

use ani_desk_core::{
    catalog::{
        apply_personal_matches, CatalogAnime, CatalogClient, CatalogFilters, TastePreference,
    },
    config::Config,
    db::Database,
    metadata::MetadataCache,
    providers::{
        best_title_match, normalize_title, Anime, AnimeProvider, Language, ProviderRegistry,
        StreamInfo,
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
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bytes::Bytes;
use db::{NewFavorite, NewHistory, SessionUser, WebDatabase};
use futures_util::TryStreamExt;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use reqwest::{header::HeaderMap as ReqwestHeaderMap, Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use uuid::Uuid;

const SESSION_COOKIE: &str = "ani_desk_session";
const MAX_MEDIA_SESSIONS: usize = 2_048;
const LOGIN_ATTEMPT_WINDOW: Duration = Duration::from_secs(15 * 60);
const LOGIN_ATTEMPT_LIMIT: usize = 8;
const LOGIN_ATTEMPT_KEY_LIMIT: usize = 10_000;

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
    media_client: Client,
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
}

#[derive(Debug, Serialize)]
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
            "ani-desk could not complete this request.",
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
    language_group_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceSearchInput {
    source: String,
    query: String,
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

#[derive(Debug, Deserialize)]
struct ResourceQuery {
    url: String,
    sig: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    name: String,
    language: String,
    language_group: String,
    status: String,
    failure_code: Option<String>,
    capabilities: ani_desk_core::providers::ProviderCapabilities,
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
    can_fallback_to_mpv: bool,
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
                .unwrap_or_else(|_| "ani_desk_server=info,tower_http=info".into()),
        )
        .init();

    let data_dir = PathBuf::from(env::var("ANI_DESK_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    tokio::fs::create_dir_all(&data_dir).await?;
    let db = WebDatabase::open(&data_dir.join("web.db")).await?;
    let admin_password = env::var("ANI_DESK_ADMIN_PASSWORD")
        .context("ANI_DESK_ADMIN_PASSWORD must be set for the hosted web service")?;
    let admin_username = env::var("ANI_DESK_ADMIN_USERNAME").unwrap_or_else(|_| "root".into());
    db.bootstrap_admin(&admin_username, &admin_password).await?;

    let core_db = Arc::new(Database::new_at(data_dir.join("catalog.db")).await?);
    let state = AppState {
        db,
        providers: Arc::new(ProviderRegistry::new(&Config::default())),
        catalog: CatalogClient::new(),
        metadata: MetadataCache::new(core_db),
        secure_cookies: env::var_os("RAILWAY_ENVIRONMENT").is_some()
            || env::var("ANI_DESK_SECURE_COOKIES").is_ok_and(|value| value != "0"),
        login_attempts: Arc::new(Mutex::new(HashMap::new())),
        download_tickets: Arc::new(Mutex::new(HashMap::new())),
        media_sessions: Arc::new(Mutex::new(HashMap::new())),
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
        .route("/anime/details", post(anime_details))
        .route("/anime/episodes", post(episodes))
        .route("/playback", post(playback))
        .route("/skip-times", post(skip_times))
        .route("/media/:id", get(media_main))
        .route("/media/:id/resource", get(media_resource))
        .route("/media/:id/dash/*path", get(media_dash_resource))
        .route("/history", get(history).post(save_progress))
        .route("/history/remove", post(remove_history))
        .route("/my-list", get(my_list).post(add_favorite))
        .route("/my-list/remove", post(remove_favorite))
        .route("/downloads/ticket", post(create_download_ticket))
        .route("/downloads/:id", get(browser_download))
        .layer(DefaultBodyLimit::max(64 * 1024));

    let web_dist = env::var("ANI_DESK_WEB_DIR").unwrap_or_else(|_| "web/dist".into());
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
    tracing::info!(%address, "ani-desk web server listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "ani-desk"}))
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
    Ok(Json(
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
            .collect(),
    ))
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
    Ok(Json(check_provider_health(&state, None).await?))
}

async fn retry_provider_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProviderHealthInput>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    require_app_request(&headers)?;
    require_user(&state, &headers).await?;
    Ok(Json(
        check_provider_health(&state, input.provider.as_deref()).await?,
    ))
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

    let mut tasks = tokio::task::JoinSet::new();
    for provider in state.providers.list_providers() {
        if selected.is_some_and(|name| name != provider.name()) {
            continue;
        }
        let provider = provider.clone();
        tasks.spawn(async move {
            let result =
                tokio::time::timeout(Duration::from_secs(30), provider.health_check()).await;
            match result {
                Ok(Ok(())) => source_dto(provider.as_ref(), "healthy", None),
                Ok(Err(error)) => source_dto(
                    provider.as_ref(),
                    "unavailable",
                    Some(classify_provider_error(&error.to_string()).into()),
                ),
                Err(_) => source_dto(
                    provider.as_ref(),
                    "unavailable",
                    Some("NETWORK_TIMEOUT".into()),
                ),
            }
        });
    }

    let mut health = Vec::new();
    while let Some(result) = tasks.join_next().await {
        health.push(result.map_err(|error| ApiError::internal("provider-health", error))?);
    }
    health.sort_by_key(|item| {
        state
            .providers
            .list_providers()
            .iter()
            .position(|provider| provider.name() == item.name)
            .unwrap_or(usize::MAX)
    });
    Ok(health)
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
    let _catalog_id = input.catalog_id;
    let mut values = Vec::new();
    for provider in state.providers.list_providers() {
        if input
            .language_group_filter
            .as_deref()
            .is_some_and(|group| !group.eq_ignore_ascii_case(language_group(provider.language())))
        {
            continue;
        }
        let result = provider.search(input.title.trim()).await;
        let (status, failure_code, anime) = match result {
            Ok(items) => {
                let selected = best_title_match(items, &[input.title.clone()])
                    .map(|anime| map_anime(anime, Some(input.catalog_id)));
                if selected.is_some() {
                    ("available".into(), None, selected)
                } else {
                    (
                        "unavailable".into(),
                        Some("TITLE_NOT_AVAILABLE".into()),
                        None,
                    )
                }
            }
            Err(error) => (
                "unavailable".into(),
                Some(classify_provider_error(&error.to_string()).into()),
                None,
            ),
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

async fn anime_details(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AnimeDetailsInput>,
) -> ApiResult<Json<AnimeDetailsDto>> {
    require_user(&state, &headers).await?;
    let mut details = AnimeDetailsDto::default();
    if let Some(provider) = state.providers.get_provider(&input.provider) {
        if let Ok(Some(anime)) = provider.get_anime_details(&input.anime_id).await {
            details.cover_url = non_empty(anime.cover_url);
            details.banner_url = anime.banner_url.and_then(non_empty);
            details.total_episodes = anime.total_episodes;
            details.synopsis = anime.synopsis.and_then(non_empty);
        }
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
    let subtitles = stream
        .subtitles
        .iter()
        .filter_map(|subtitle| {
            Url::parse(&subtitle.url).ok().map(|url| SubtitleDto {
                language: subtitle.language.clone(),
                url: signed_resource_url(&id, &secret, &url),
            })
        })
        .collect();
    let now = Instant::now();
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
    sessions.insert(
        id.clone(),
        MediaSession {
            user_id: user.id,
            expires_at: now + Duration::from_secs(6 * 60 * 60),
            stream: stream.clone(),
            secret,
        },
    );
    Ok(Json(PlaybackDto {
        session_id: id.clone(),
        playback_url: format!("/api/media/{id}"),
        stream_kind: if stream.video_url.to_ascii_lowercase().contains(".m3u8") {
            "hls"
        } else if stream.video_url.to_ascii_lowercase().contains(".mpd") {
            "dash"
        } else {
            "native"
        }
        .into(),
        subtitles,
        qualities: stream.qualities,
        can_fallback_to_mpv: false,
    }))
}

async fn skip_times(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SkipTimesInput>,
) -> ApiResult<Json<Vec<SkipTime>>> {
    require_user(&state, &headers).await?;
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
    Path(id): Path<String>,
    Query(query): Query<ResourceQuery>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let session = get_media_session(&state, &id, &user.id).await?;
    verify_resource_signature(&session.secret, &query.url, &query.sig)?;
    let url = Url::parse(&query.url).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_MEDIA_RESOURCE",
            "playback",
            error.to_string(),
            false,
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_MEDIA_RESOURCE",
            "playback",
            "Only HTTP media resources are supported.",
            false,
        ));
    }
    proxy_media_url(&state, &id, &session, url, &headers).await
}

async fn media_dash_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    let user = require_user(&state, &headers).await?;
    let session = get_media_session(&state, &id, &user.id).await?;
    let Some(value) = path.strip_prefix("base/") else {
        return Err(invalid_dash_resource());
    };
    let mut parts = value.splitn(3, '/');
    let encoded_base = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default();
    let relative_path = parts.next().unwrap_or_default();
    let base_bytes = URL_SAFE_NO_PAD
        .decode(encoded_base)
        .map_err(|_| invalid_dash_resource())?;
    let base_value = String::from_utf8(base_bytes).map_err(|_| invalid_dash_resource())?;
    verify_resource_signature(&session.secret, &base_value, signature)?;
    let base = Url::parse(&base_value).map_err(|_| invalid_dash_resource())?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err(invalid_dash_resource());
    }
    let upstream = resolve_dash_upstream(base, relative_path)?;
    proxy_media_url(&state, &id, &session, upstream, &headers).await
}

fn resolve_dash_upstream(base: Url, relative_path: &str) -> ApiResult<Url> {
    let origin = base.origin();
    let upstream = if relative_path.is_empty() {
        base
    } else {
        base.join(relative_path)
            .map_err(|_| invalid_dash_resource())?
    };
    if upstream.origin() != origin {
        return Err(invalid_dash_resource());
    }
    Ok(upstream)
}

fn invalid_dash_resource() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "INVALID_MEDIA_RESOURCE",
        "playback",
        "The DASH media resource is invalid.",
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

async fn proxy_media_url(
    state: &AppState,
    session_id: &str,
    session: &MediaSession,
    url: Url,
    incoming: &HeaderMap,
) -> ApiResult<Response> {
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
    let response = request.send().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "PROXY_FAILED",
            "playback",
            error.to_string(),
            true,
        )
    })?;
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
        let text = response
            .error_for_status()
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "PROXY_FAILED",
                    "playback",
                    error.to_string(),
                    true,
                )
            })?
            .text()
            .await
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "PROXY_FAILED",
                    "playback",
                    error.to_string(),
                    true,
                )
            })?;
        let rewritten = rewrite_hls_manifest(session_id, &session.secret, &url, &text);
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
        let text = response
            .error_for_status()
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "PROXY_FAILED",
                    "playback",
                    error.to_string(),
                    true,
                )
            })?
            .text()
            .await
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "PROXY_FAILED",
                    "playback",
                    error.to_string(),
                    true,
                )
            })?;
        let rewritten = rewrite_dash_manifest(session_id, &session.secret, &url, &text);
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

fn rewrite_hls_manifest(session_id: &str, secret: &[u8; 32], base: &Url, manifest: &str) -> String {
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
                    .map(|url| signed_resource_url(session_id, secret, &url))
                    .unwrap_or_else(|_| line.to_string());
            }
            if let Some(uri) = quoted_attribute(trimmed, "URI") {
                if let Ok(url) = base.join(&uri) {
                    return line.replacen(
                        &format!("URI=\"{uri}\""),
                        &format!("URI=\"{}\"", signed_resource_url(session_id, secret, &url)),
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
    secret: &[u8; 32],
    manifest_url: &Url,
    manifest: &str,
) -> String {
    let mut output = String::with_capacity(manifest.len() + 128);
    let mut remaining = manifest;
    let mut found_base = false;
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
        let absolute = manifest_url
            .join(original)
            .unwrap_or_else(|_| manifest_url.clone());
        output.push_str(&signed_dash_base(session_id, secret, &absolute));
        output.push_str("</BaseURL>");
        remaining = &remaining[close + "</BaseURL>".len()..];
        found_base = true;
    }
    output.push_str(remaining);
    if found_base {
        return output;
    }
    if let Some(mpd_start) = output.find("<MPD") {
        if let Some(relative_end) = output[mpd_start..].find('>') {
            let insert_at = mpd_start + relative_end + 1;
            let parent = manifest_url
                .join(".")
                .unwrap_or_else(|_| manifest_url.clone());
            output.insert_str(
                insert_at,
                &format!(
                    "<BaseURL>{}</BaseURL>",
                    signed_dash_base(session_id, secret, &parent)
                ),
            );
        }
    }
    output
}

fn signed_dash_base(session_id: &str, secret: &[u8; 32], base: &Url) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(base.as_str());
    let signature = resource_signature(secret, base.as_str());
    format!("/api/media/{session_id}/dash/base/{encoded}/{signature}/")
}

fn signed_resource_url(session_id: &str, secret: &[u8; 32], url: &Url) -> String {
    let encoded = url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect::<String>();
    format!(
        "/api/media/{session_id}/resource?url={encoded}&sig={}",
        resource_signature(secret, url.as_str())
    )
}

fn resource_signature(secret: &[u8; 32], value: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("fixed-size HMAC key");
    mac.update(value.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn verify_resource_signature(secret: &[u8; 32], value: &str, signature: &str) -> ApiResult<()> {
    let signature = URL_SAFE_NO_PAD.decode(signature).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "INVALID_MEDIA_RESOURCE",
            "playback",
            "The media resource signature is invalid.",
            false,
        )
    })?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("fixed-size HMAC key");
    mac.update(value.as_bytes());
    mac.verify_slice(&signature).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "INVALID_MEDIA_RESOURCE",
            "playback",
            "The media resource signature is invalid.",
            false,
        )
    })
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
        let segments = resolve_hls_segments(client, &upstream_headers, source).await?;
        Body::from_stream(hls_body_stream(
            client.clone(),
            upstream_headers.clone(),
            segments,
        ))
    } else {
        let response = client
            .get(source)
            .headers(upstream_headers)
            .send()
            .await
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "DOWNLOAD_FAILED",
                    "download",
                    error.to_string(),
                    true,
                )
            })?
            .error_for_status()
            .map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "DOWNLOAD_FAILED",
                    "download",
                    error.to_string(),
                    true,
                )
            })?;
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
                if let Ok(url) = media_url.join(&uri) {
                    segments.push(url);
                }
            }
        } else if !value.is_empty() && !value.starts_with('#') {
            if let Ok(url) = media_url.join(value) {
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
                .map_err(std::io::Error::other)?
                .error_for_status().map_err(std::io::Error::other)?;
            while let Some(chunk) = response.chunk().await.map_err(std::io::Error::other)? {
                yield chunk;
            }
        }
    }
}

async fn fetch_text(client: &Client, headers: &ReqwestHeaderMap, url: Url) -> ApiResult<String> {
    client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "DOWNLOAD_FAILED",
                "download",
                error.to_string(),
                true,
            )
        })?
        .error_for_status()
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "DOWNLOAD_FAILED",
                "download",
                error.to_string(),
                true,
            )
        })?
        .text()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "DOWNLOAD_FAILED",
                "download",
                error.to_string(),
                true,
            )
        })
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
        "ani-desk".into()
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
        .get("x-ani-desk-request")
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
    }
}
fn language_group(language: Language) -> &'static str {
    match language {
        Language::English => "english",
        Language::Vietnamese => "vietnamese",
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
    fn dash_rewrite_signs_existing_and_default_base_urls() {
        let manifest_url = Url::parse("https://cdn.example/show/manifest.mpd").unwrap();
        let secret = [7_u8; 32];
        let with_base = rewrite_dash_manifest(
            "session",
            &secret,
            &manifest_url,
            "<MPD><Period><BaseURL>video/</BaseURL></Period></MPD>",
        );
        assert!(with_base.contains("/api/media/session/dash/base/"));
        assert!(!with_base.contains(">video/</BaseURL>"));

        let without_base =
            rewrite_dash_manifest("session", &secret, &manifest_url, "<MPD><Period /></MPD>");
        assert!(without_base.starts_with("<MPD><BaseURL>/api/media/session/dash/base/"));
    }

    #[test]
    fn dash_resources_must_remain_on_the_signed_origin() {
        let base = Url::parse("https://cdn.example/show/manifest.mpd").unwrap();
        assert_eq!(
            resolve_dash_upstream(base.clone(), "segments/1.m4s")
                .unwrap()
                .as_str(),
            "https://cdn.example/show/segments/1.m4s"
        );
        assert!(resolve_dash_upstream(base, "https://attacker.example/1.m4s").is_err());
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
