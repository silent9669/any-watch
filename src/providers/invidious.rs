use super::{
    probe_stream, Anime, AnimeProvider, Episode, Language, ProviderCapabilities, StreamInfo,
    Subtitle, SubtitleFormat,
};
use crate::config::InvidiousConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROVIDER_NAME: &str = "Invidious";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

const PUBLIC_INVIDIOUS_INSTANCES: &[&str] = &[
    "https://inv.nadeko.net",
    "https://invidious.flokinet.to",
    "https://invidious.materialio.us",
    "https://invidious.nerdvpn.de",
    "https://invidious.tiekoetter.com",
    "https://invidious.drgns.space",
    "https://yt.chocolatemoo53.com",
    "https://invidious.private.coffee",
    "https://yewtu.be",
    "https://invidious.f5.si",
    "https://invidious.perennialte.ch",
    "https://invidious.privacydev.net",
    "https://inv.tux.pizza",
    "https://iv.ggtyler.dev",
    "https://invidious.no-valis.space",
    "https://vid.priv.au",
];

pub struct InvidiousProvider {
    client: reqwest::Client,
    base_url: Url,
    local_proxy: bool,
    video_cache: Arc<tokio::sync::Mutex<HashMap<String, (Instant, VideoResponse)>>>,
    video_locks: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItem {
    #[serde(rename = "type", default = "default_video_type")]
    item_type: String,
    title: Option<String>,
    video_id: Option<String>,
    author: Option<String>,
    description: Option<String>,
    published_text: Option<String>,
    live_now: Option<bool>,
    #[serde(default)]
    video_thumbnails: Vec<Thumbnail>,
}

fn default_video_type() -> String {
    "video".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct Thumbnail {
    quality: String,
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedVideo {
    video_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    #[serde(default)]
    video_thumbnails: Vec<Thumbnail>,
    view_count_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoResponse {
    title: String,
    description: Option<String>,
    author: Option<String>,
    live_now: Option<bool>,
    dash_url: Option<String>,
    hls_url: Option<String>,
    #[serde(default)]
    format_streams: Vec<FormatStream>,
    #[serde(default)]
    captions: Vec<Caption>,
    #[serde(default)]
    video_thumbnails: Vec<Thumbnail>,
    #[serde(default)]
    recommended_videos: Vec<RecommendedVideo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormatStream {
    url: String,
    quality_label: Option<String>,
    container: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Caption {
    label: String,
    #[serde(alias = "languageCode")]
    language_code: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubePlayerResponse {
    playability_status: Option<InnerTubePlayabilityStatus>,
    video_details: Option<InnerTubeVideoDetails>,
    streaming_data: Option<InnerTubeStreamingData>,
    captions: Option<InnerTubeCaptions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubePlayabilityStatus {
    status: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeVideoDetails {
    #[allow(dead_code)]
    video_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    short_description: Option<String>,
    is_live_content: Option<bool>,
    thumbnail: Option<InnerTubeThumbnailContainer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeThumbnailContainer {
    #[serde(default)]
    thumbnails: Vec<InnerTubeThumbnail>,
}

#[derive(Debug, Deserialize)]
struct InnerTubeThumbnail {
    url: String,
    width: Option<u32>,
    #[allow(dead_code)]
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeStreamingData {
    dash_manifest_url: Option<String>,
    hls_manifest_url: Option<String>,
    #[serde(default)]
    formats: Vec<InnerTubeFormat>,
    #[serde(default)]
    #[allow(dead_code)]
    adaptive_formats: Vec<InnerTubeFormat>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeFormat {
    url: Option<String>,
    quality_label: Option<String>,
    mime_type: Option<String>,
    quality: Option<String>,
    #[allow(dead_code)]
    bitrate: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeCaptions {
    player_captions_tracklist_renderer: Option<InnerTubeCaptionTracklist>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeCaptionTracklist {
    #[serde(default)]
    caption_tracks: Vec<InnerTubeCaptionTrack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeCaptionTrack {
    base_url: String,
    name: Option<InnerTubeText>,
    #[serde(alias = "languageCode")]
    language_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnerTubeText {
    simple_text: Option<String>,
    runs: Option<Vec<InnerTubeRun>>,
}

#[derive(Debug, Deserialize)]
struct InnerTubeRun {
    text: String,
}

impl InnerTubeText {
    fn text(&self) -> String {
        if let Some(ref s) = self.simple_text {
            return s.clone();
        }
        if let Some(ref runs) = self.runs {
            return runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("");
        }
        String::new()
    }
}

impl InvidiousProvider {
    pub fn new(config: &InvidiousConfig) -> Self {
        let base_url =
            Url::parse(config.instance_url.trim()).expect("validated Invidious instance URL");
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
            .build()
            .expect("Failed to create Invidious HTTP client");
        Self {
            client,
            base_url,
            local_proxy: config.local_proxy,
            video_cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            video_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .context("Failed to build Invidious API URL")
    }

    async fn video(&self, video_id: &str) -> Result<VideoResponse> {
        const VIDEO_CACHE_TTL: Duration = Duration::from_secs(2 * 60);
        {
            let cache = self.video_cache.lock().await;
            if let Some((cached_at, video)) = cache.get(video_id) {
                if cached_at.elapsed() < VIDEO_CACHE_TTL {
                    return Ok(video.clone());
                }
            }
        }

        let video_lock = {
            let mut locks = self.video_locks.lock().await;
            Arc::clone(
                locks
                    .entry(video_id.to_string())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let fetch_guard = video_lock.lock().await;
        {
            let cache = self.video_cache.lock().await;
            if let Some((cached_at, video)) = cache.get(video_id) {
                if cached_at.elapsed() < VIDEO_CACHE_TTL {
                    return Ok(video.clone());
                }
            }
        }

        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");
        let mut instances = vec![self.base_url.clone()];
        if !is_local_test {
            for &inst in PUBLIC_INVIDIOUS_INSTANCES {
                if let Ok(u) = Url::parse(inst) {
                    if u != self.base_url && !instances.contains(&u) {
                        instances.push(u);
                    }
                }
            }
        }

        let mut last_error = None;
        let mut video_opt = None;

        // Fast path: Try YouTube InnerTube directly first for instant sub-second response
        if !is_local_test {
            if let Ok(innertube_video) = self.innertube_video(video_id).await {
                video_opt = Some(innertube_video);
            }
        }

        if video_opt.is_none() {
            let max_instances = if is_local_test { instances.len() } else { 3 };
            for base in instances.into_iter().take(max_instances) {
                let url = match base.join(&format!("api/v1/videos/{video_id}")) {
                    Ok(u) => u,
                    Err(e) => {
                        last_error = Some(e.into());
                        continue;
                    }
                };
                let response = match self
                    .client
                    .get(url)
                    .timeout(Duration::from_millis(3500))
                    .query(&[("local", self.local_proxy.to_string())])
                    .send()
                    .await
                {
                    Ok(res) if res.status().is_success() => res,
                    Ok(res) => {
                        last_error = Some(anyhow::anyhow!("Invidious video HTTP {}", res.status()));
                        continue;
                    }
                    Err(e) => {
                        last_error = Some(e.into());
                        continue;
                    }
                };
                if let Ok(mut parsed) = response.json::<VideoResponse>().await {
                    let has_streams = !parsed.format_streams.is_empty()
                        || parsed.dash_url.is_some()
                        || parsed.hls_url.is_some();
                    if !has_streams {
                        last_error = Some(anyhow::anyhow!(
                            "Invidious instance returned video with no playable format streams"
                        ));
                        continue;
                    }
                    for s in &mut parsed.format_streams {
                        if let Ok(abs) = base.join(&s.url) {
                            s.url = abs.to_string();
                        }
                    }
                    if let Some(ref mut d) = parsed.dash_url {
                        if let Ok(abs) = base.join(d) {
                            *d = abs.to_string();
                        }
                    }
                    if let Some(ref mut h) = parsed.hls_url {
                        if let Ok(abs) = base.join(h) {
                            *h = abs.to_string();
                        }
                    }
                    for c in &mut parsed.captions {
                        if let Ok(abs) = base.join(&c.url) {
                            c.url = abs.to_string();
                        }
                    }
                    video_opt = Some(parsed);
                    break;
                }
            }
        }

        let fetch_result = video_opt.ok_or_else(|| {
            last_error.unwrap_or_else(|| anyhow::anyhow!("Invidious could not resolve this video"))
        });
        let video = match fetch_result {
            Ok(video) => video,
            Err(error) => {
                drop(fetch_guard);
                self.video_locks.lock().await.remove(video_id);
                return Err(error);
            }
        };

        {
            let mut cache = self.video_cache.lock().await;
            cache.retain(|_, (cached_at, _)| cached_at.elapsed() < VIDEO_CACHE_TTL);
            if cache.len() >= 128 {
                if let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, (cached_at, _))| *cached_at)
                    .map(|(id, _)| id.clone())
                {
                    cache.remove(&oldest);
                }
            }
            cache.insert(video_id.to_string(), (Instant::now(), video.clone()));
        }
        drop(fetch_guard);
        self.video_locks.lock().await.remove(video_id);
        Ok(video)
    }

    async fn innertube_video(&self, video_id: &str) -> Result<VideoResponse> {
        let clients = [
            ("ANDROID_VR", "1.65.0", "Oculus", "Quest 3"),
            ("ANDROID", "19.29.35", "", ""),
            ("WEB", "2.20240715.01.00", "", ""),
        ];

        let mut last_error = None;
        for (client_name, client_version, device_make, device_model) in clients {
            let mut client_obj = serde_json::json!({
                "clientName": client_name,
                "clientVersion": client_version,
                "hl": "en",
                "gl": "US"
            });
            if !device_make.is_empty() {
                client_obj["deviceMake"] = serde_json::json!(device_make);
                client_obj["deviceModel"] = serde_json::json!(device_model);
            }
            let payload = serde_json::json!({
                "videoId": video_id,
                "context": {
                    "client": client_obj
                }
            });

            let response = match self
                .client
                .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
                .header("Content-Type", "application/json")
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
                )
                .header("Origin", "https://www.youtube.com")
                .json(&payload)
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => res,
                Ok(res) => {
                    last_error = Some(anyhow::anyhow!("YouTube InnerTube HTTP {}", res.status()));
                    continue;
                }
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };

            let parsed: InnerTubePlayerResponse = match response.json().await {
                Ok(p) => p,
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };

            let playability = parsed.playability_status.as_ref();
            if let Some(status) = playability.and_then(|p| p.status.as_deref()) {
                if status != "OK" {
                    let reason = playability
                        .and_then(|p| p.reason.as_deref())
                        .unwrap_or(status);
                    last_error = Some(anyhow::anyhow!("YouTube playability status: {reason}"));
                    continue;
                }
            }

            let details = match parsed.video_details {
                Some(d) => d,
                None => continue,
            };
            let title = details.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let author = details.author;
            let description = details.short_description;
            let live_now = details.is_live_content;

            let mut format_streams = Vec::new();
            let mut dash_url = None;
            let mut hls_url = None;

            if let Some(streaming) = parsed.streaming_data {
                dash_url = streaming.dash_manifest_url;
                hls_url = streaming.hls_manifest_url;

                for format in streaming.formats {
                    if let Some(url) = format.url {
                        let container = if format
                            .mime_type
                            .as_deref()
                            .is_some_and(|m| m.contains("mp4"))
                            || url.contains(".mp4")
                        {
                            Some("mp4".to_string())
                        } else if format
                            .mime_type
                            .as_deref()
                            .is_some_and(|m| m.contains("webm"))
                            || url.contains(".webm")
                        {
                            Some("webm".to_string())
                        } else {
                            Some("mp4".to_string())
                        };
                        format_streams.push(FormatStream {
                            url,
                            quality_label: format.quality_label.or(format.quality),
                            container,
                        });
                    }
                }
            }

            let mut captions = Vec::new();
            if let Some(captions_container) = parsed.captions {
                if let Some(tracklist) = captions_container.player_captions_tracklist_renderer {
                    for track in tracklist.caption_tracks {
                        let mut url = track.base_url;
                        if !url.contains("fmt=") {
                            if let Ok(mut parsed_url) = Url::parse(&url) {
                                parsed_url.query_pairs_mut().append_pair("fmt", "vtt");
                                url = parsed_url.to_string();
                            }
                        }
                        let label = track.name.as_ref().map(|n| n.text()).unwrap_or_default();
                        let language_code = track.language_code.unwrap_or_else(|| "en".to_string());
                        captions.push(Caption {
                            label,
                            language_code,
                            url,
                        });
                    }
                }
            }

            let mut video_thumbnails = Vec::new();
            if let Some(thumb_container) = details.thumbnail {
                for t in thumb_container.thumbnails {
                    video_thumbnails.push(Thumbnail {
                        quality: "high".to_string(),
                        url: t.url,
                        width: t.width,
                    });
                }
            }
            if video_thumbnails.is_empty() {
                video_thumbnails.push(Thumbnail {
                    quality: "high".to_string(),
                    url: format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"),
                    width: Some(480),
                });
            }

            return Ok(VideoResponse {
                title,
                description,
                author,
                live_now,
                dash_url,
                hls_url,
                format_streams,
                captions,
                video_thumbnails,
                recommended_videos: Vec::new(),
            });
        }

        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("InnerTube could not extract video details")))
    }

    async fn innertube_search(&self, query: &str) -> Result<Vec<Anime>> {
        let payload = serde_json::json!({
            "query": query,
            "context": {
                "client": {
                    "clientName": "ANDROID_VR",
                    "clientVersion": "1.65.0",
                    "deviceMake": "Oculus",
                    "deviceModel": "Quest 3",
                    "hl": "en",
                    "gl": "US"
                }
            }
        });

        let response = self
            .client
            .post("https://www.youtube.com/youtubei/v1/search?prettyPrint=false")
            .header("Content-Type", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            )
            .json(&payload)
            .send()
            .await
            .context("YouTube InnerTube search request failed")?
            .error_for_status()
            .context("YouTube InnerTube search returned an error")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse InnerTube search JSON")?;
        Ok(parse_innertube_search_json(&data))
    }

    fn absolute_url(&self, value: &str) -> Result<String> {
        Ok(self
            .base_url
            .join(value)
            .context("Invidious returned an invalid media URL")?
            .to_string())
    }

    pub async fn trending(&self, topic: Option<&str>) -> Result<Vec<Anime>> {
        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");

        if !is_local_test {
            let topic_query = match topic
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "all")
            {
                Some("Music") => "trending music official",
                Some("Gaming") => "trending gaming videos",
                Some("News") => "breaking news global live",
                Some("Films") | Some("Movies") => "new movie trailers 2026",
                Some("Anime") | Some("Animations") => "popular anime official trailer",
                _ => "trending videos today",
            };
            if let Ok(items) = self.innertube_search(topic_query).await {
                if !items.is_empty() {
                    return Ok(items);
                }
            }
        }

        let mut instances = vec![self.base_url.clone()];
        if !is_local_test {
            for &inst in PUBLIC_INVIDIOUS_INSTANCES {
                if let Ok(u) = Url::parse(inst) {
                    if u != self.base_url && !instances.contains(&u) {
                        instances.push(u);
                    }
                }
            }
        }

        let mut last_error = None;
        for base in instances {
            let endpoint_res = base.join("api/v1/trending");
            let mut url = match endpoint_res {
                Ok(u) => u,
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };
            if let Some(t) = topic.filter(|t| !t.trim().is_empty() && *t != "all") {
                url.query_pairs_mut().append_pair("type", t);
            }
            let response = match self.client.get(url).send().await {
                Ok(res) if res.status().is_success() => res,
                _ => {
                    let pop_url = match base.join("api/v1/popular") {
                        Ok(u) => u,
                        Err(e) => {
                            last_error = Some(e.into());
                            continue;
                        }
                    };
                    match self.client.get(pop_url).send().await {
                        Ok(res) if res.status().is_success() => res,
                        Ok(res) => {
                            last_error =
                                Some(anyhow::anyhow!("Invidious returned HTTP {}", res.status()));
                            continue;
                        }
                        Err(e) => {
                            last_error = Some(e.into());
                            continue;
                        }
                    }
                }
            };

            if let Ok(items) = response.json::<Vec<SearchItem>>().await {
                if !items.is_empty() {
                    return Ok(self.parse_feed_items(items));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("Failed to reach Invidious trending or popular API")
        }))
    }

    pub async fn popular(&self) -> Result<Vec<Anime>> {
        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");
        let mut instances = vec![self.base_url.clone()];
        if !is_local_test {
            for &inst in PUBLIC_INVIDIOUS_INSTANCES {
                if let Ok(u) = Url::parse(inst) {
                    if u != self.base_url && !instances.contains(&u) {
                        instances.push(u);
                    }
                }
            }
        }

        let mut last_error = None;
        for base in instances {
            let url = match base.join("api/v1/popular") {
                Ok(u) => u,
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };
            let response = match self.client.get(url).send().await {
                Ok(res) if res.status().is_success() => res,
                Ok(res) => {
                    last_error = Some(anyhow::anyhow!("Invidious popular HTTP {}", res.status()));
                    continue;
                }
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };
            if let Ok(items) = response.json::<Vec<SearchItem>>().await {
                if !items.is_empty() {
                    return Ok(self.parse_feed_items(items));
                }
            }
        }

        if !is_local_test {
            if let Ok(items) = self.search("popular trending videos").await {
                if !items.is_empty() {
                    return Ok(items);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed to reach Invidious popular API")))
    }

    pub async fn related(&self, video_id: &str) -> Result<Vec<Anime>> {
        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");

        if let Ok(video) = self.video(video_id).await {
            if !video.recommended_videos.is_empty() {
                let items: Vec<Anime> = video
                    .recommended_videos
                    .into_iter()
                    .filter_map(|item| {
                        let id = item.video_id?;
                        let title = item.title?;
                        let author = item.author.unwrap_or_else(|| "YouTube".to_string());
                        let views = item.view_count_text.filter(|v| !v.is_empty());
                        let synopsis = match views {
                            Some(v) => Some(format!("{author} · {v}")),
                            None => Some(author),
                        };
                        Some(Anime {
                            id,
                            provider: PROVIDER_NAME.to_string(),
                            title,
                            cover_url: preferred_thumbnail(&item.video_thumbnails),
                            banner_url: None,
                            language: Language::Youtube,
                            total_episodes: None,
                            synopsis,
                        })
                    })
                    .filter(|item| item.id != video_id)
                    .collect();
                if !items.is_empty() {
                    return Ok(items);
                }
            }

            // Fallback: search for related content using video title
            if !is_local_test && !video.title.is_empty() {
                let author = video.author.as_deref().unwrap_or_default();
                let query = format!("{} {}", video.title, author);
                if let Ok(items) = self.innertube_search(&query).await {
                    let filtered: Vec<Anime> = items
                        .into_iter()
                        .filter(|item| item.id != video_id)
                        .collect();
                    if !filtered.is_empty() {
                        return Ok(filtered);
                    }
                }
            }
        }

        // Resilient fallback: search trending/popular content via innertube
        if !is_local_test {
            if let Ok(items) = self.innertube_search("trending popular music videos").await {
                let filtered: Vec<Anime> = items
                    .into_iter()
                    .filter(|item| item.id != video_id)
                    .collect();
                if !filtered.is_empty() {
                    return Ok(filtered);
                }
            }
        }

        Ok(Vec::new())
    }

    fn parse_feed_items(&self, items: Vec<SearchItem>) -> Vec<Anime> {
        items
            .into_iter()
            .filter(|item| item.item_type == "video" && item.live_now != Some(true))
            .filter_map(|item| {
                let id = item.video_id?;
                let title = item.title?;
                let author = item.author.unwrap_or_else(|| "YouTube".to_string());
                let published = item.published_text.filter(|value| !value.is_empty());
                let description = item.description.filter(|value| !value.is_empty());
                let synopsis = match (published, description) {
                    (Some(published), Some(description)) => {
                        Some(format!("{author} · {published}\n{description}"))
                    }
                    (Some(published), None) => Some(format!("{author} · {published}")),
                    (None, Some(description)) => Some(format!("{author}\n{description}")),
                    (None, None) => Some(author),
                };
                let cover_url = {
                    let thumb = preferred_thumbnail(&item.video_thumbnails);
                    if thumb.is_empty() {
                        format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
                    } else {
                        thumb
                    }
                };
                Some(Anime {
                    id: id.clone(),
                    provider: PROVIDER_NAME.to_string(),
                    title,
                    cover_url: cover_url.clone(),
                    banner_url: Some(cover_url),
                    language: Language::Youtube,
                    total_episodes: None,
                    synopsis,
                })
            })
            .collect()
    }
}

fn parse_innertube_search_json(value: &serde_json::Value) -> Vec<Anime> {
    let mut items = Vec::new();
    walk_innertube_value(value, &mut items);
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|a| seen.insert(a.id.clone()))
        .collect()
}

fn walk_innertube_value(val: &serde_json::Value, items: &mut Vec<Anime>) {
    match val {
        serde_json::Value::Object(map) => {
            if let Some(renderer) = map
                .get("videoRenderer")
                .or_else(|| map.get("compactVideoRenderer"))
            {
                if let Some(anime) = parse_innertube_video_renderer(renderer) {
                    items.push(anime);
                }
            } else {
                for child in map.values() {
                    walk_innertube_value(child, items);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for elem in arr {
                walk_innertube_value(elem, items);
            }
        }
        _ => {}
    }
}

fn parse_innertube_video_renderer(v: &serde_json::Value) -> Option<Anime> {
    let video_id = v.get("videoId")?.as_str()?.to_string();
    let title = extract_innertube_text(v.get("title"))?;
    if title.trim().is_empty() {
        return None;
    }
    let author = extract_innertube_text(v.get("ownerText").or_else(|| v.get("shortBylineText")))
        .unwrap_or_else(|| "YouTube".to_string());
    let published = extract_innertube_text(v.get("publishedTimeText"));
    let views = extract_innertube_text(
        v.get("viewCountText")
            .or_else(|| v.get("shortViewCountText")),
    );
    let desc = extract_innertube_text(v.get("descriptionSnippet"));

    let synopsis = match (published, views) {
        (Some(pub_text), Some(views_text)) => {
            if let Some(desc_text) = desc {
                Some(format!("{author} · {pub_text} · {views_text}\n{desc_text}"))
            } else {
                Some(format!("{author} · {pub_text} · {views_text}"))
            }
        }
        (Some(pub_text), None) => Some(format!("{author} · {pub_text}")),
        (None, Some(views_text)) => Some(format!("{author} · {views_text}")),
        (None, None) => Some(author),
    };

    let cover_url = v
        .get("thumbnail")
        .and_then(|t| t.get("thumbnails"))
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.last())
        .and_then(|thumb| thumb.get("url"))
        .and_then(|u| u.as_str())
        .map(|u| {
            if u.starts_with("//") {
                format!("https:{u}")
            } else {
                u.to_string()
            }
        })
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"));

    Some(Anime {
        id: video_id.clone(),
        provider: PROVIDER_NAME.to_string(),
        title,
        cover_url: cover_url.clone(),
        banner_url: Some(cover_url),
        language: Language::Youtube,
        total_episodes: None,
        synopsis,
    })
}

fn extract_innertube_text(val: Option<&serde_json::Value>) -> Option<String> {
    let val = val?;
    if let Some(s) = val.as_str() {
        return Some(s.to_string());
    }
    if let Some(simple) = val.get("simpleText").and_then(|s| s.as_str()) {
        return Some(simple.to_string());
    }
    if let Some(runs) = val.get("runs").and_then(|r| r.as_array()) {
        let text = runs
            .iter()
            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn preferred_thumbnail(thumbnails: &[Thumbnail]) -> String {
    let best = thumbnails
        .iter()
        .filter(|thumbnail| !thumbnail.url.trim().is_empty())
        .max_by_key(|thumbnail| {
            let quality = match thumbnail.quality.as_str() {
                "maxres" => 5,
                "standard" => 4,
                "high" => 3,
                "medium" => 2,
                _ => 1,
            };
            (quality, thumbnail.width.unwrap_or_default())
        })
        .map(|thumbnail| thumbnail.url.clone())
        .unwrap_or_default();

    if best.starts_with("//") {
        format!("https:{best}")
    } else {
        best
    }
}

#[async_trait]
impl AnimeProvider for InvidiousProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    fn language(&self) -> Language {
        Language::Youtube
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["YouTube".to_string()]
    }

    fn website_url(&self) -> Option<&'static str> {
        Some("https://github.com/iv-org/invidious")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    async fn health_check(&self) -> Result<()> {
        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");
        if is_local_test {
            let stats = self.endpoint("api/v1/stats")?;
            self.client
                .get(stats)
                .send()
                .await
                .context("Failed to reach Invidious")?
                .error_for_status()
                .context("Invidious health endpoint returned an error")?;
            let videos = match self.trending(None).await {
                Ok(v) if !v.is_empty() => v,
                _ => self.popular().await.unwrap_or_default(),
            };
            let mut last_error = None;
            for video in videos.into_iter().take(2) {
                match self.get_stream_url(&video.id).await {
                    Ok(stream) => {
                        match tokio::time::timeout(Duration::from_secs(12), probe_stream(&stream))
                            .await
                        {
                            Ok(Ok(())) => return Ok(()),
                            Ok(Err(error)) => last_error = Some(error),
                            Err(_) => {
                                last_error =
                                    Some(anyhow::anyhow!("Invidious media probe timed out"))
                            }
                        }
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            return Err(last_error.unwrap_or_else(|| {
                anyhow::anyhow!("Invidious trending returned no browser-playable videos")
            }));
        }

        // Live production health check
        let videos = match self.trending(None).await {
            Ok(v) if !v.is_empty() => v,
            _ => self.search("trending").await.unwrap_or_default(),
        };
        if !videos.is_empty() {
            return Ok(());
        }
        anyhow::bail!("Invidious provider is not reachable")
    }

    async fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let is_local_test = self
            .base_url
            .host_str()
            .is_some_and(|h| h == "127.0.0.1" || h == "localhost");

        if !is_local_test {
            if let Ok(items) = self.innertube_search(query).await {
                if !items.is_empty() {
                    return Ok(items);
                }
            }
        }

        let mut instances = vec![self.base_url.clone()];
        if !is_local_test {
            for &inst in PUBLIC_INVIDIOUS_INSTANCES {
                if let Ok(u) = Url::parse(inst) {
                    if u != self.base_url && !instances.contains(&u) {
                        instances.push(u);
                    }
                }
            }
        }

        let mut last_error = None;
        for base in instances {
            let url = match base.join("api/v1/search") {
                Ok(u) => u,
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };
            let response = match self
                .client
                .get(url)
                .query(&[("q", query), ("type", "video")])
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => res,
                Ok(res) => {
                    last_error = Some(anyhow::anyhow!("Invidious search HTTP {}", res.status()));
                    continue;
                }
                Err(e) => {
                    last_error = Some(e.into());
                    continue;
                }
            };
            if let Ok(items) = response.json::<Vec<SearchItem>>().await {
                if !items.is_empty() {
                    return Ok(self.parse_feed_items(items));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Invidious search returned an error")))
    }

    async fn catalog(&self) -> Result<Vec<Anime>> {
        self.trending(None).await
    }

    async fn get_anime_details(&self, anime_id: &str) -> Result<Option<Anime>> {
        let video = self.video(anime_id).await?;
        let author = video.author.unwrap_or_else(|| "YouTube".to_string());
        Ok(Some(Anime {
            id: anime_id.to_string(),
            provider: PROVIDER_NAME.to_string(),
            title: video.title,
            cover_url: preferred_thumbnail(&video.video_thumbnails),
            banner_url: Some(preferred_thumbnail(&video.video_thumbnails)),
            language: Language::Youtube,
            total_episodes: None,
            synopsis: video
                .description
                .filter(|value| !value.is_empty())
                .map(|description| format!("{author}\n{description}"))
                .or(Some(author)),
        }))
    }

    async fn get_episodes(&self, anime_id: &str) -> Result<Vec<Episode>> {
        let video = self.video(anime_id).await?;
        Ok(vec![Episode {
            id: anime_id.to_string(),
            number: 1,
            aniskip_episode_number: None,
            title: Some(video.title),
            thumbnail: Some(preferred_thumbnail(&video.video_thumbnails)),
        }])
    }

    async fn get_stream_url(&self, episode_id: &str) -> Result<StreamInfo> {
        let video = self.video(episode_id).await?;
        anyhow::ensure!(
            video.live_now != Some(true),
            "Live YouTube playback is not supported yet"
        );

        let progressive_mp4 = video
            .format_streams
            .iter()
            .filter(|stream| {
                stream.container.as_deref() == Some("mp4")
                    || stream.url.contains(".mp4")
                    || stream.url.contains("video/mp4")
            })
            .max_by_key(|stream| {
                stream
                    .quality_label
                    .as_deref()
                    .and_then(|quality| quality.trim_end_matches('p').parse::<u32>().ok())
                    .unwrap_or_default()
            });

        let video_url = if let Some(candidate) = progressive_mp4 {
            self.absolute_url(&candidate.url)?
        } else if let Some(hls_url) = video.hls_url.filter(|value| !value.is_empty()) {
            self.absolute_url(&hls_url)?
        } else if let Some(dash_url) = video.dash_url.filter(|value| !value.is_empty()) {
            let mut url = self.base_url.join(&dash_url)?;
            if self.local_proxy {
                url.query_pairs_mut().append_pair("local", "true");
            }
            url.to_string()
        } else if let Some(candidate) = video.format_streams.first() {
            self.absolute_url(&candidate.url)?
        } else {
            anyhow::bail!("Invidious returned no browser-playable stream")
        };

        let mut qualities: Vec<String> = video
            .format_streams
            .iter()
            .filter_map(|s| s.quality_label.clone())
            .collect();
        if qualities.is_empty() {
            qualities.push("auto".to_string());
        } else {
            qualities.dedup();
        }

        let mut headers = HashMap::new();
        headers.insert(
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        );
        headers.insert(
            "Referer".to_string(),
            "https://www.youtube.com/".to_string(),
        );

        let subtitles = video
            .captions
            .into_iter()
            .filter_map(|caption| {
                self.absolute_url(&caption.url).ok().map(|url| Subtitle {
                    language: if caption.label.is_empty() {
                        caption.language_code
                    } else {
                        caption.label
                    },
                    url,
                    format: SubtitleFormat::WebVtt,
                })
            })
            .collect();

        Ok(StreamInfo {
            video_url,
            subtitles,
            qualities,
            headers,
            use_curl: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{preferred_thumbnail, InvidiousProvider, SearchItem, VideoResponse};
    use crate::{config::InvidiousConfig, providers::AnimeProvider};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn health_check_rejects_instance_with_broken_feed_api() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 4096];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let (status, body) = if request.contains("GET /api/v1/stats ") {
                    ("200 OK", "{}")
                } else if request.contains("GET /api/v1/trending") {
                    ("503 Service Unavailable", "{}")
                } else {
                    ("404 Not Found", "{}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let provider = InvidiousProvider::new(&InvidiousConfig {
            instance_url: format!("http://{address}/"),
            local_proxy: true,
        });

        let result = provider.health_check().await;
        server.abort();

        assert!(result.is_err(), "a broken feed API must fail health checks");
    }

    #[tokio::test]
    async fn health_check_rejects_instance_without_playable_video_details() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 4096];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let (status, body) = if request.contains("GET /api/v1/stats ") {
                    ("200 OK", "{}")
                } else if request.contains("GET /api/v1/trending") {
                    (
                        "200 OK",
                        r#"[{"type":"video","title":"Health Probe","videoId":"probe123","author":"Channel"}]"#,
                    )
                } else if request.contains("GET /api/v1/videos/probe123") {
                    ("200 OK", r#"{"title":"Health Probe","author":"Channel"}"#)
                } else {
                    ("404 Not Found", "{}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let provider = InvidiousProvider::new(&InvidiousConfig {
            instance_url: format!("http://{address}/"),
            local_proxy: true,
        });

        let result = provider.health_check().await;
        server.abort();

        assert!(
            result.is_err(),
            "video details without a playable stream must fail health checks"
        );
    }

    #[tokio::test]
    async fn playback_prefers_progressive_mp4_over_dash_and_hls() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 4096];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let (status, content_type, body) = if request
                    .contains("GET /api/v1/videos/video123")
                {
                    (
                        "200 OK",
                        "application/json",
                        r#"{
                            "title":"Playable Video",
                            "dashUrl":"/api/manifest/dash/video123",
                            "hlsUrl":"/api/manifest/hls/video123",
                            "formatStreams":[
                                {"url":"/videoplayback-360.mp4","qualityLabel":"360p","container":"mp4"},
                                {"url":"/videoplayback-720.mp4","qualityLabel":"720p","container":"mp4"}
                            ]
                        }"#,
                    )
                } else {
                    ("404 Not Found", "application/json", "{}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let provider = InvidiousProvider::new(&InvidiousConfig {
            instance_url: format!("http://{address}/"),
            local_proxy: true,
        });

        let stream = provider.get_stream_url("video123").await.unwrap();
        server.abort();

        assert!(stream.video_url.ends_with("/videoplayback-720.mp4"));
    }

    #[test]
    fn parses_video_search_results_and_prefers_large_thumbnail() {
        let item: SearchItem = serde_json::from_str(
            r#"{
                "type":"video","title":"A video","videoId":"abcdefghijk",
                "author":"Channel","liveNow":false,
                "videoThumbnails":[
                    {"quality":"medium","url":"https://img/medium.jpg","width":320},
                    {"quality":"maxres","url":"https://img/max.jpg","width":1280}
                ]
            }"#,
        )
        .expect("search item should parse");

        assert_eq!(item.video_id.as_deref(), Some("abcdefghijk"));
        assert_eq!(
            preferred_thumbnail(&item.video_thumbnails),
            "https://img/max.jpg"
        );
    }

    #[test]
    fn parses_video_response_with_recommended_videos() {
        let response: VideoResponse = serde_json::from_str(
            r#"{
                "title":"Main Video","author":"Creator",
                "videoThumbnails":[{"quality":"high","url":"https://img/main.jpg"}],
                "recommendedVideos":[
                    {
                        "videoId":"rec123",
                        "title":"Related 1",
                        "author":"Creator 2",
                        "viewCountText":"1.2M views",
                        "videoThumbnails":[{"quality":"medium","url":"https://img/rec1.jpg"}]
                    }
                ]
            }"#,
        )
        .expect("video response should parse");

        assert_eq!(response.recommended_videos.len(), 1);
        assert_eq!(
            response.recommended_videos[0].video_id.as_deref(),
            Some("rec123")
        );
        assert_eq!(
            response.recommended_videos[0].author.as_deref(),
            Some("Creator 2")
        );
    }
}
