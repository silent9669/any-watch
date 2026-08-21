use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command;
use tokio::sync::{broadcast, Notify, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const MAX_ACTIVE_OR_QUEUED_TASKS_PER_USER: usize = 10;
const MAX_TASK_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const TASK_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "m4v", "ts", "m2ts", "flv",
];
const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt", "ass", "ssa"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Downloading {
        progress: f32,
        speed_bytes_per_sec: u64,
        eta_seconds: u64,
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Remuxing {
        progress: f32,
        message: String,
    },
    Ready {
        file_name: String,
        file_size: u64,
        has_mp4: bool,
        subtitles: Vec<SubtitleFileMeta>,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtitleFileMeta {
    pub language: String,
    pub language_code: String,
    pub format: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentTask {
    pub id: String,
    pub title: String,
    pub magnet_url: String,
    pub status: TaskStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    pub sub_pref: Option<String>,
    #[serde(skip)]
    pub owner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub magnet_url: String,
    #[serde(default)]
    pub torrent_url: Option<String>,
    #[serde(default)]
    pub expected_size_bytes: Option<u64>,
    pub sub_pref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedTorrentTask {
    owner_id: String,
    task: TorrentTask,
}

#[derive(Clone)]
struct TaskControl {
    cancel: CancellationToken,
    done: Arc<Notify>,
}

#[derive(Clone)]
struct ToolPaths {
    torrent_client: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    archive: PathBuf,
    unrar: PathBuf,
}

impl ToolPaths {
    fn from_env() -> Self {
        let path = |name: &str, fallback: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(fallback))
        };
        Self {
            torrent_client: path("ANY_WATCH_TORRENT_CLIENT", "aria2c"),
            ffmpeg: path("ANY_WATCH_FFMPEG_BIN", "ffmpeg"),
            ffprobe: path("ANY_WATCH_FFPROBE_BIN", "ffprobe"),
            archive: path("ANY_WATCH_ARCHIVE_TOOL", "7z"),
            unrar: path("ANY_WATCH_UNRAR_BIN", "unrar-free"),
        }
    }
}

pub struct TorrentTaskManager {
    tasks: Arc<RwLock<HashMap<String, TorrentTask>>>,
    controls: Arc<RwLock<HashMap<String, TaskControl>>>,
    base_dir: PathBuf,
    event_tx: broadcast::Sender<TorrentTask>,
    download_slots: Arc<Semaphore>,
    tools: ToolPaths,
}

impl TorrentTaskManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self::with_tools(base_dir, ToolPaths::from_env())
    }

    fn with_tools(base_dir: PathBuf, tools: ToolPaths) -> Self {
        if let Err(error) = std::fs::create_dir_all(&base_dir) {
            warn!(path = %base_dir.display(), %error, "failed to create torrent task directory");
        }
        let loaded_tasks = load_persisted_tasks(&base_dir);
        let (event_tx, _) = broadcast::channel(128);
        let manager = Self {
            tasks: Arc::new(RwLock::new(loaded_tasks)),
            controls: Arc::new(RwLock::new(HashMap::new())),
            base_dir,
            event_tx,
            download_slots: Arc::new(Semaphore::new(3)),
            tools,
        };

        let tasks = Arc::clone(&manager.tasks);
        let base_dir = manager.base_dir.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                Self::prune_stale_tasks(&tasks, &base_dir).await;
            }
        });

        manager
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TorrentTask> {
        self.event_tx.subscribe()
    }

    pub async fn list_tasks(&self, owner_id: &str) -> Vec<TorrentTask> {
        let tasks = self.tasks.read().await;
        let mut list: Vec<TorrentTask> = tasks
            .values()
            .filter(|task| task.owner_id == owner_id)
            .cloned()
            .collect();
        list.sort_by_key(|task| std::cmp::Reverse(task.created_at));
        list
    }

    pub async fn get_task(&self, owner_id: &str, id: &str) -> Option<TorrentTask> {
        let tasks = self.tasks.read().await;
        tasks
            .get(id)
            .filter(|task| task.owner_id == owner_id)
            .cloned()
    }

    pub async fn create_task(
        &self,
        owner_id: &str,
        mut req: CreateTaskRequest,
    ) -> Result<TorrentTask> {
        let title = req.title.trim();
        if title.is_empty() {
            bail!("Title is required");
        }
        if title.chars().count() > 200 {
            bail!("Title must be 200 characters or fewer");
        }
        let download_source = validated_download_source(&req)?;
        if req
            .expected_size_bytes
            .is_some_and(|size| size > MAX_TASK_BYTES)
        {
            bail!("Torrent is larger than the 50 GiB task limit");
        }
        let expected_size = req.expected_size_bytes.unwrap_or(0);
        let available_space = fs2::available_space(&self.base_dir)
            .context("Failed to check available download storage")?;
        let required_space = expected_size
            .saturating_mul(2)
            .saturating_add(2 * 1024 * 1024 * 1024);
        if available_space < required_space {
            bail!("Not enough storage is available to download and prepare this video");
        }
        req.title = title.to_string();
        req.sub_pref = normalize_subtitle_preference(req.sub_pref.as_deref())?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let task_dir = self.base_dir.join(&task_id);
        fs::create_dir_all(&task_dir)
            .await
            .context("Failed to create torrent task workspace")?;

        let task = TorrentTask {
            id: task_id.clone(),
            title: req.title.clone(),
            magnet_url: download_source.clone(),
            status: TaskStatus::Queued,
            created_at: unix_now(),
            completed_at: None,
            sub_pref: req.sub_pref.clone(),
            owner_id: owner_id.to_string(),
        };

        {
            let mut tasks = self.tasks.write().await;
            let queued_for_user = tasks
                .values()
                .filter(|existing| {
                    existing.owner_id == owner_id
                        && matches!(
                            existing.status,
                            TaskStatus::Queued
                                | TaskStatus::Downloading { .. }
                                | TaskStatus::Remuxing { .. }
                        )
                })
                .count();
            if queued_for_user >= MAX_ACTIVE_OR_QUEUED_TASKS_PER_USER {
                drop(tasks);
                let _ = fs::remove_dir_all(&task_dir).await;
                bail!(
                    "Maximum queued and active downloads limit ({MAX_ACTIVE_OR_QUEUED_TASKS_PER_USER}) reached"
                );
            }
            tasks.insert(task_id.clone(), task.clone());
        }

        if let Err(error) = persist_task(&self.base_dir, &task).await {
            self.tasks.write().await.remove(&task_id);
            let _ = fs::remove_dir_all(&task_dir).await;
            return Err(error);
        }
        let _ = self.event_tx.send(task.clone());

        let control = TaskControl {
            cancel: CancellationToken::new(),
            done: Arc::new(Notify::new()),
        };
        self.controls
            .write()
            .await
            .insert(task_id.clone(), control.clone());

        let tasks = Arc::clone(&self.tasks);
        let controls = Arc::clone(&self.controls);
        let base_dir = self.base_dir.clone();
        let event_tx = self.event_tx.clone();
        let slots = Arc::clone(&self.download_slots);
        let tools = self.tools.clone();
        let title = req.title;
        let sub_pref = req.sub_pref;
        let expected_size = req.expected_size_bytes.unwrap_or(0);
        let task_id_for_worker = task_id.clone();
        tokio::spawn(async move {
            let result = Self::execute_task(
                Arc::clone(&tasks),
                base_dir.clone(),
                event_tx.clone(),
                slots,
                tools,
                control.cancel.clone(),
                task_id_for_worker.clone(),
                title,
                download_source,
                expected_size,
                sub_pref,
            )
            .await;
            if let Err(error) = result {
                if !control.cancel.is_cancelled() {
                    Self::fail_task(
                        &tasks,
                        &base_dir,
                        &event_tx,
                        &task_id_for_worker,
                        user_safe_task_error(&error),
                    )
                    .await;
                }
            }
            controls.write().await.remove(&task_id_for_worker);
            control.done.notify_one();
        });

        Ok(task)
    }

    pub async fn delete_task(&self, owner_id: &str, id: &str) -> Result<bool> {
        {
            let tasks = self.tasks.read().await;
            if !tasks.get(id).is_some_and(|task| task.owner_id == owner_id) {
                return Ok(false);
            }
        }

        if let Some(control) = self.controls.read().await.get(id).cloned() {
            control.cancel.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(5), control.done.notified()).await;
        }

        let removed = {
            let mut tasks = self.tasks.write().await;
            tasks
                .get(id)
                .is_some_and(|task| task.owner_id == owner_id)
                .then(|| tasks.remove(id))
                .flatten()
        };
        if removed.is_none() {
            return Ok(false);
        }

        let task_dir = self.base_dir.join(id);
        if task_dir.exists() {
            fs::remove_dir_all(&task_dir)
                .await
                .context("Failed to remove torrent task files")?;
        }
        info!(task_id = id, "deleted torrent task");
        Ok(true)
    }

    pub async fn get_task_file_path(&self, owner_id: &str, id: &str) -> Option<(PathBuf, String)> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(id).filter(|task| task.owner_id == owner_id)?;
        let TaskStatus::Ready { file_name, .. } = &task.status else {
            return None;
        };
        let path = self.base_dir.join(id).join(file_name);
        path.is_file().then(|| (path, file_name.clone()))
    }

    pub async fn get_subtitle_file_path(
        &self,
        owner_id: &str,
        id: &str,
        lang_code: &str,
    ) -> Option<(PathBuf, String)> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(id).filter(|task| task.owner_id == owner_id)?;
        let TaskStatus::Ready { subtitles, .. } = &task.status else {
            return None;
        };
        let subtitle = subtitles
            .iter()
            .find(|subtitle| subtitle.language_code == lang_code)?;
        let path = self.base_dir.join(id).join(&subtitle.file_name);
        path.is_file().then(|| (path, subtitle.file_name.clone()))
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_task(
        tasks: Arc<RwLock<HashMap<String, TorrentTask>>>,
        base_dir: PathBuf,
        event_tx: broadcast::Sender<TorrentTask>,
        slots: Arc<Semaphore>,
        tools: ToolPaths,
        cancel: CancellationToken,
        task_id: String,
        title: String,
        download_source: String,
        expected_size: u64,
        sub_pref: Option<String>,
    ) -> Result<()> {
        let _permit = tokio::select! {
            _ = cancel.cancelled() => bail!("Task was cancelled"),
            permit = slots.acquire_owned() => permit.context("Torrent worker queue closed")?,
        };
        ensure_task_exists(&tasks, &task_id).await?;

        let task_dir = base_dir.join(&task_id);
        let payload_dir = task_dir.join("payload");
        let extracted_dir = task_dir.join("extracted");
        fs::create_dir_all(&payload_dir)
            .await
            .context("Failed to create torrent payload directory")?;

        info!(task_id = %task_id, title = %title, "starting torrent download");
        update_status(
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            TaskStatus::Downloading {
                progress: 0.0,
                speed_bytes_per_sec: 0,
                eta_seconds: 0,
                downloaded_bytes: 0,
                total_bytes: expected_size,
            },
        )
        .await?;
        run_torrent_client(
            &tools.torrent_client,
            &payload_dir,
            &download_source,
            expected_size,
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            &cancel,
        )
        .await?;

        update_status(
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            TaskStatus::Remuxing {
                progress: 0.12,
                message: "Inspecting downloaded files and archives...".to_string(),
            },
        )
        .await?;

        let direct_video_available = !video_candidates(&payload_dir, None).is_empty();
        if let Err(error) = extract_archives(&tools, &payload_dir, &extracted_dir, &cancel).await {
            if !direct_video_available {
                return Err(error);
            }
            warn!(task_id = %task_id, %error, "archive extraction failed but direct video is available");
        }

        update_status(
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            TaskStatus::Remuxing {
                progress: 0.32,
                message: "Selecting the main playable video...".to_string(),
            },
        )
        .await?;

        let (input_video, probe) = select_main_video(&tools.ffprobe, &payload_dir, &extracted_dir)
            .await
            .context("No playable video file was found in the torrent")?;
        let output_name = format!("{}.mp4", sanitize_file_stem(&title));
        let output_path = task_dir.join(&output_name);

        update_status(
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            TaskStatus::Remuxing {
                progress: 0.5,
                message: if probe.video_codec.as_deref() == Some("h264") {
                    "Optimizing the video for fast-start browser playback...".to_string()
                } else {
                    "Converting the video to browser-compatible H.264...".to_string()
                },
            },
        )
        .await?;
        remux_video(
            &tools.ffmpeg,
            &tools.ffprobe,
            &input_video,
            &output_path,
            &probe,
            &cancel,
        )
        .await?;

        update_status(
            &tasks,
            &base_dir,
            &event_tx,
            &task_id,
            TaskStatus::Remuxing {
                progress: 0.86,
                message: "Extracting included browser subtitle tracks...".to_string(),
            },
        )
        .await?;
        let subtitles = extract_subtitles(
            &tools.ffmpeg,
            &input_video,
            &probe,
            &payload_dir,
            &extracted_dir,
            &task_dir,
            &sanitize_file_stem(&title),
            sub_pref.as_deref(),
            &cancel,
        )
        .await;

        let file_size = fs::metadata(&output_path)
            .await
            .context("Failed to inspect remuxed MP4")?
            .len();
        if file_size < 1024 {
            bail!("Remuxed MP4 is empty or invalid");
        }

        let _ = fs::remove_dir_all(&payload_dir).await;
        let _ = fs::remove_dir_all(&extracted_dir).await;
        let now = unix_now();
        let ready = {
            let mut guard = tasks.write().await;
            let task = guard
                .get_mut(&task_id)
                .context("Task was deleted before completion")?;
            task.status = TaskStatus::Ready {
                file_name: output_name,
                file_size,
                has_mp4: true,
                subtitles,
            };
            task.completed_at = Some(now);
            task.clone()
        };
        persist_task(&base_dir, &ready).await?;
        let _ = event_tx.send(ready);
        info!(task_id = %task_id, file_size, "torrent task is ready");
        Ok(())
    }

    async fn fail_task(
        tasks: &Arc<RwLock<HashMap<String, TorrentTask>>>,
        base_dir: &Path,
        event_tx: &broadcast::Sender<TorrentTask>,
        task_id: &str,
        reason: String,
    ) {
        error!(task_id, reason, "torrent task failed");
        let failed = {
            let mut guard = tasks.write().await;
            guard.get_mut(task_id).map(|task| {
                task.status = TaskStatus::Failed {
                    reason: reason.clone(),
                };
                task.completed_at = Some(unix_now());
                task.clone()
            })
        };
        if let Some(task) = failed {
            if let Err(error) = persist_task(base_dir, &task).await {
                warn!(task_id, %error, "failed to persist failed torrent task");
            }
            let _ = event_tx.send(task);
        }
    }

    async fn prune_stale_tasks(tasks: &Arc<RwLock<HashMap<String, TorrentTask>>>, base_dir: &Path) {
        let now = unix_now();
        let stale_ids: Vec<String> = {
            let guard = tasks.read().await;
            guard
                .values()
                .filter(|task| {
                    task.completed_at.is_some_and(|completed_at| {
                        now.saturating_sub(completed_at) > TASK_RETENTION.as_secs()
                    })
                })
                .map(|task| task.id.clone())
                .collect()
        };

        for id in stale_ids {
            info!(task_id = %id, "auto-pruning stale torrent task");
            let _ = fs::remove_dir_all(base_dir.join(&id)).await;
            tasks.write().await.remove(&id);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_torrent_client(
    client: &Path,
    payload_dir: &Path,
    source: &str,
    expected_size: u64,
    tasks: &Arc<RwLock<HashMap<String, TorrentTask>>>,
    base_dir: &Path,
    event_tx: &broadcast::Sender<TorrentTask>,
    task_id: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut command = Command::new(client);
    command
        .arg("--dir")
        .arg(payload_dir)
        .arg("--seed-time=0")
        .arg("--follow-torrent=mem")
        .arg("--file-allocation=none")
        .arg("--continue=true")
        .arg("--allow-overwrite=true")
        .arg("--auto-file-renaming=false")
        .arg("--check-integrity=true")
        .arg("--summary-interval=0")
        .arg("--console-log-level=warn")
        .arg("--download-result=hide")
        .arg("--connect-timeout=30")
        .arg("--timeout=60")
        .arg("--bt-stop-timeout=300")
        .arg(source)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = command.spawn().with_context(|| {
        format!(
            "Failed to start torrent client '{}'; install aria2",
            client.display()
        )
    })?;
    let mut previous_bytes = 0_u64;
    let mut previous_at = Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                bail!("Task was cancelled");
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }

        if !tasks.read().await.contains_key(task_id) {
            let _ = child.kill().await;
            bail!("Task was cancelled");
        }

        if let Some(status) = child
            .try_wait()
            .context("Failed to inspect torrent client")?
        {
            if !status.success() {
                bail!("Torrent client could not complete this download");
            }
            break;
        }

        let downloaded_bytes = directory_size(payload_dir).await;
        if downloaded_bytes > MAX_TASK_BYTES {
            let _ = child.kill().await;
            bail!("Torrent exceeded the 50 GiB task limit");
        }
        let elapsed = previous_at.elapsed().as_secs_f64().max(0.1);
        let speed_bytes_per_sec =
            (downloaded_bytes.saturating_sub(previous_bytes) as f64 / elapsed) as u64;
        previous_bytes = downloaded_bytes;
        previous_at = Instant::now();
        let total_bytes = expected_size.max(downloaded_bytes);
        let progress = if expected_size > 0 {
            (downloaded_bytes as f64 / expected_size as f64).clamp(0.0, 0.99) as f32
        } else if downloaded_bytes > 0 {
            0.01
        } else {
            0.0
        };
        let eta_seconds = if expected_size > downloaded_bytes && speed_bytes_per_sec > 0 {
            (expected_size - downloaded_bytes) / speed_bytes_per_sec
        } else {
            0
        };
        update_status(
            tasks,
            base_dir,
            event_tx,
            task_id,
            TaskStatus::Downloading {
                progress,
                speed_bytes_per_sec,
                eta_seconds,
                downloaded_bytes,
                total_bytes,
            },
        )
        .await?;
    }

    let downloaded_bytes = directory_size(payload_dir).await;
    if downloaded_bytes == 0 {
        bail!("Torrent client finished without downloading files");
    }
    Ok(())
}

async fn extract_archives(
    tools: &ToolPaths,
    payload_dir: &Path,
    extracted_dir: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut extracted_count = 0_usize;
    for _ in 0..2 {
        let mut archives = archive_candidates(payload_dir, Some(&seen));
        archives.extend(archive_candidates(extracted_dir, Some(&seen)));
        if archives.is_empty() {
            break;
        }
        for archive in archives {
            if extracted_count >= 12 {
                bail!("Torrent contains too many nested archives");
            }
            seen.insert(archive.clone());
            validate_archive_paths(&tools.archive, &archive, cancel).await?;
            let output_dir = extracted_dir.join(format!("archive-{extracted_count}"));
            fs::create_dir_all(&output_dir).await?;

            let mut command = Command::new(&tools.archive);
            command
                .arg("x")
                .arg("-y")
                .arg("-bd")
                .arg(format!("-o{}", output_dir.display()))
                .arg(&archive)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut status = run_command(command, cancel).await;
            if status.as_ref().is_ok_and(|status| !status.success())
                && archive
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("rar"))
            {
                let mut fallback = Command::new(&tools.unrar);
                fallback
                    .arg("x")
                    .arg("-o+")
                    .arg(&archive)
                    .arg(&output_dir)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                status = run_command(fallback, cancel).await;
            }
            match status {
                Ok(status) if status.success() => {
                    extracted_count += 1;
                    remove_symlinks(&output_dir);
                    if directory_size(extracted_dir).await > MAX_TASK_BYTES {
                        bail!("Extracted torrent exceeded the 50 GiB task limit");
                    }
                }
                Ok(_) => warn!(archive = %archive.display(), "archive extraction failed"),
                Err(error) => return Err(error.context("Archive extraction tool is unavailable")),
            }
        }
    }
    Ok(())
}

async fn validate_archive_paths(
    archive_tool: &Path,
    archive: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut command = Command::new(archive_tool);
    command
        .arg("l")
        .arg("-slt")
        .arg(archive)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = command.spawn().with_context(|| {
        format!(
            "Failed to inspect archive with '{}'",
            archive_tool.display()
        )
    })?;
    let output = tokio::select! {
        _ = cancel.cancelled() => bail!("Task was cancelled"),
        output = child.wait_with_output() => output?,
    };
    if !output.status.success() {
        bail!("Archive index could not be read safely");
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let entries = listing
        .split_once("----------")
        .map(|(_, entries)| entries)
        .unwrap_or_default();
    let mut unpacked_size = 0_u64;
    for line in entries.lines() {
        if let Some(path) = line.strip_prefix("Path = ") {
            let candidate = Path::new(path.trim());
            if candidate.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            }) {
                bail!("Archive contains an unsafe file path");
            }
        }
        if line.starts_with("Symbolic Link = ") || line.starts_with("Hard Link = ") {
            bail!("Archive contains unsupported file links");
        }
        if let Some(size) = line.strip_prefix("Size = ") {
            unpacked_size = unpacked_size.saturating_add(size.trim().parse::<u64>().unwrap_or(0));
            if unpacked_size > MAX_TASK_BYTES {
                bail!("Archive expands beyond the 50 GiB task limit");
            }
        }
    }
    Ok(())
}

async fn select_main_video(
    ffprobe: &Path,
    payload_dir: &Path,
    extracted_dir: &Path,
) -> Option<(PathBuf, ProbeInfo)> {
    let mut candidates = video_candidates(payload_dir, None);
    candidates.extend(video_candidates(extracted_dir, None));
    candidates.sort_by(|left, right| {
        left.is_sample
            .cmp(&right.is_sample)
            .then_with(|| right.size.cmp(&left.size))
    });

    for candidate in candidates {
        if candidate.size < 1024 * 1024 {
            continue;
        }
        if let Ok(probe) = probe_media(ffprobe, &candidate.path).await {
            if probe.video_codec.is_some() && probe.duration_seconds.unwrap_or(31.0) >= 30.0 {
                return Some((candidate.path, probe));
            }
        }
    }
    None
}

async fn remux_video(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &Path,
    output: &Path,
    probe: &ProbeInfo,
    cancel: &CancellationToken,
) -> Result<()> {
    let transcode_video = probe.video_codec.as_deref() != Some("h264");
    let transcode_audio = probe.audio_codecs.iter().any(|codec| codec != "aac");
    let _ = fs::remove_file(output).await;

    let first = run_ffmpeg(
        ffmpeg,
        input,
        output,
        transcode_video,
        transcode_audio,
        cancel,
    )
    .await?;
    if !first.success() {
        let _ = fs::remove_file(output).await;
        let retry = run_ffmpeg(ffmpeg, input, output, true, true, cancel).await?;
        if !retry.success() {
            bail!("FFmpeg could not convert the selected video to MP4");
        }
    }

    let output_probe = probe_media(ffprobe, output)
        .await
        .context("FFprobe could not validate the remuxed MP4")?;
    if output_probe.video_codec.as_deref() != Some("h264") {
        bail!("Remuxed output is not browser-compatible H.264 video");
    }
    Ok(())
}

async fn run_ffmpeg(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    transcode_video: bool,
    transcode_audio: bool,
    cancel: &CancellationToken,
) -> Result<ExitStatus> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a?")
        .arg("-sn")
        .arg("-map_metadata")
        .arg("0");
    if transcode_video {
        command
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("veryfast")
            .arg("-crf")
            .arg("21")
            .arg("-pix_fmt")
            .arg("yuv420p");
    } else {
        command.arg("-c:v").arg("copy");
    }
    if transcode_audio {
        command.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
    } else {
        command.arg("-c:a").arg("copy");
    }
    command
        .arg("-max_muxing_queue_size")
        .arg("4096")
        .arg("-movflags")
        .arg("+faststart")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run_command(command, cancel)
        .await
        .context("Failed to start FFmpeg")
}

#[allow(clippy::too_many_arguments)]
async fn extract_subtitles(
    ffmpeg: &Path,
    input_video: &Path,
    probe: &ProbeInfo,
    payload_dir: &Path,
    extracted_dir: &Path,
    task_dir: &Path,
    output_stem: &str,
    preference: Option<&str>,
    cancel: &CancellationToken,
) -> Vec<SubtitleFileMeta> {
    let desired = match preference.unwrap_or("all") {
        "vi" => vec!["vi"],
        "en" => vec!["en"],
        _ => vec!["vi", "en"],
    };
    let mut files = subtitle_candidates(payload_dir);
    files.extend(subtitle_candidates(extracted_dir));
    files.sort_by_key(|file| std::cmp::Reverse(file.size));
    let mut subtitles = Vec::new();

    for code in desired {
        let output_name = format!("{output_stem}.{code}.vtt");
        let output_path = task_dir.join(&output_name);
        let external = files
            .iter()
            .find(|candidate| detect_subtitle_language(&candidate.path) == Some(code));
        let status = if let Some(external) = external {
            convert_subtitle(ffmpeg, &external.path, None, &output_path, cancel).await
        } else if let Some(stream) = probe
            .subtitle_streams
            .iter()
            .find(|stream| subtitle_stream_language(stream) == Some(code))
        {
            convert_subtitle(
                ffmpeg,
                input_video,
                Some(stream.index),
                &output_path,
                cancel,
            )
            .await
        } else {
            Ok(failed_exit_status())
        };

        if status.is_ok_and(|status| status.success())
            && fs::metadata(&output_path)
                .await
                .is_ok_and(|metadata| metadata.len() > 0)
        {
            subtitles.push(SubtitleFileMeta {
                language: if code == "vi" {
                    "Vietnamese".to_string()
                } else {
                    "English".to_string()
                },
                language_code: code.to_string(),
                format: "vtt".to_string(),
                file_name: output_name,
            });
        } else {
            let _ = fs::remove_file(&output_path).await;
        }
    }
    subtitles
}

async fn convert_subtitle(
    ffmpeg: &Path,
    input: &Path,
    stream_index: Option<u32>,
    output: &Path,
    cancel: &CancellationToken,
) -> Result<ExitStatus> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input);
    if let Some(index) = stream_index {
        command.arg("-map").arg(format!("0:{index}"));
    }
    command
        .arg("-f")
        .arg("webvtt")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    run_command(command, cancel).await
}

async fn run_command(mut command: Command, cancel: &CancellationToken) -> Result<ExitStatus> {
    command.kill_on_drop(true);
    let mut child = command.spawn()?;
    tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            bail!("Task was cancelled")
        }
        status = child.wait() => Ok(status?),
    }
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProbeStream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Debug)]
struct ProbeInfo {
    duration_seconds: Option<f64>,
    video_codec: Option<String>,
    audio_codecs: Vec<String>,
    subtitle_streams: Vec<ProbeStream>,
}

async fn probe_media(ffprobe: &Path, path: &Path) -> Result<ProbeInfo> {
    let mut command = Command::new(ffprobe);
    command
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("stream=index,codec_type,codec_name:stream_tags=language,title:format=duration")
        .arg("-of")
        .arg("json")
        .arg(path)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .context("FFprobe timed out")??;
    if !output.status.success() {
        bail!("FFprobe rejected the media file");
    }
    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout)?;
    let video_codec = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .and_then(|stream| stream.codec_name.clone());
    let audio_codecs = parsed
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .filter_map(|stream| stream.codec_name.clone())
        .collect();
    let subtitle_streams = parsed
        .streams
        .iter()
        .filter(|stream| {
            stream.codec_type.as_deref() == Some("subtitle")
                && matches!(
                    stream.codec_name.as_deref(),
                    Some("subrip" | "ass" | "ssa" | "webvtt" | "mov_text")
                )
        })
        .cloned()
        .collect();
    Ok(ProbeInfo {
        duration_seconds: parsed
            .format
            .and_then(|format| format.duration)
            .and_then(|duration| duration.parse().ok()),
        video_codec,
        audio_codecs,
        subtitle_streams,
    })
}

#[derive(Debug)]
struct FileCandidate {
    path: PathBuf,
    size: u64,
    is_sample: bool,
}

fn video_candidates(root: &Path, exclude: Option<&HashSet<PathBuf>>) -> Vec<FileCandidate> {
    collect_files(root)
        .into_iter()
        .filter(|(path, _)| exclude.is_none_or(|excluded| !excluded.contains(path)))
        .filter(|(path, _)| has_extension(path, VIDEO_EXTENSIONS))
        .map(|(path, size)| {
            let lower = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            FileCandidate {
                path,
                size,
                is_sample: ["sample", "trailer", "preview", "opening", "ending"]
                    .iter()
                    .any(|marker| lower.contains(marker)),
            }
        })
        .collect()
}

fn subtitle_candidates(root: &Path) -> Vec<FileCandidate> {
    collect_files(root)
        .into_iter()
        .filter(|(path, _)| has_extension(path, SUBTITLE_EXTENSIONS))
        .map(|(path, size)| FileCandidate {
            path,
            size,
            is_sample: false,
        })
        .collect()
}

fn archive_candidates(root: &Path, seen: Option<&HashSet<PathBuf>>) -> Vec<PathBuf> {
    collect_files(root)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| seen.is_none_or(|seen| !seen.contains(path)))
        .filter(|path| is_primary_archive(path))
        .collect()
}

fn collect_files(root: &Path) -> Vec<(PathBuf, u64)> {
    fn visit(path: &Path, depth: usize, output: &mut Vec<(PathBuf, u64)>) {
        if depth > 16 || output.len() >= 20_000 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                visit(&path, depth + 1, output);
            } else if metadata.is_file() {
                output.push((path, metadata.len()));
            }
        }
    }

    let mut files = Vec::new();
    if root.is_dir() {
        visit(root, 0, &mut files);
    }
    files
}

async fn directory_size(root: &Path) -> u64 {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        collect_files(&root).into_iter().map(|(_, size)| size).sum()
    })
    .await
    .unwrap_or(0)
}

fn remove_symlinks(root: &Path) {
    fn visit(path: &Path, depth: usize) {
        if depth > 16 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                let _ = std::fs::remove_file(path);
            } else if metadata.is_dir() {
                visit(&path, depth + 1);
            }
        }
    }
    visit(root, 0);
}

fn is_primary_archive(path: &Path) -> bool {
    let lower = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower.ends_with(".zip")
        || lower.ends_with(".7z")
        || lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tgz")
    {
        return true;
    }
    if !lower.ends_with(".rar") {
        return false;
    }
    if let Some(part_start) = lower.rfind(".part") {
        let number = &lower[part_start + 5..lower.len() - 4];
        return number.parse::<u32>().ok() == Some(1);
    }
    true
}

fn has_extension(path: &Path, supported: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            supported
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

fn detect_subtitle_language(path: &Path) -> Option<&'static str> {
    let lower = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let normalized = lower
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    if lower.contains("vietsub")
        || tokens
            .iter()
            .any(|token| matches!(*token, "vi" | "vie" | "vietnamese" | "vietnam"))
    {
        Some("vi")
    } else if lower.contains("engsub")
        || tokens
            .iter()
            .any(|token| matches!(*token, "en" | "eng" | "english"))
    {
        Some("en")
    } else {
        None
    }
}

fn subtitle_stream_language(stream: &ProbeStream) -> Option<&'static str> {
    let value = stream
        .tags
        .get("language")
        .or_else(|| stream.tags.get("LANGUAGE"))?
        .to_ascii_lowercase();
    match value.as_str() {
        "vi" | "vie" | "vietnamese" => Some("vi"),
        "en" | "eng" | "english" => Some("en"),
        _ => None,
    }
}

fn sanitize_file_stem(title: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in title.chars() {
        let next = if character.is_alphanumeric() || matches!(character, '.' | '-' | '_') {
            previous_separator = false;
            character
        } else {
            if previous_separator {
                continue;
            }
            previous_separator = true;
            '_'
        };
        if output.len() + next.len_utf8() > 120 {
            break;
        }
        output.push(next);
    }
    let output = output.trim_matches(['.', '-', '_']).to_string();
    if output.is_empty() {
        "any-watch-video".to_string()
    } else {
        output
    }
}

fn normalize_subtitle_preference(value: Option<&str>) -> Result<Option<String>> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => Ok(Some("all".to_string())),
        Some("vi") => Ok(Some("vi".to_string())),
        Some("en") => Ok(Some("en".to_string())),
        Some(_) => bail!("Subtitle preference must be all, vi, or en"),
    }
}

fn validated_download_source(request: &CreateTaskRequest) -> Result<String> {
    for candidate in [
        Some(request.magnet_url.as_str()),
        request.torrent_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    {
        if valid_magnet(candidate) || valid_torrent_url(candidate) {
            return Ok(candidate.to_string());
        }
    }
    bail!("A valid magnet link or HTTP(S) .torrent URL is required")
}

fn valid_magnet(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "magnet"
        && url.query_pairs().any(|(key, value)| {
            key.eq_ignore_ascii_case("xt")
                && value
                    .to_ascii_lowercase()
                    .strip_prefix("urn:btih:")
                    .is_some_and(|hash| {
                        matches!(hash.len(), 32 | 40)
                            && hash
                                .chars()
                                .all(|character| character.is_ascii_alphanumeric())
                    })
        })
}

fn valid_torrent_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url
            .path()
            .to_ascii_lowercase()
            .trim_end_matches('/')
            .ends_with(".torrent")
}

async fn ensure_task_exists(
    tasks: &Arc<RwLock<HashMap<String, TorrentTask>>>,
    task_id: &str,
) -> Result<()> {
    if tasks.read().await.contains_key(task_id) {
        Ok(())
    } else {
        bail!("Task was cancelled")
    }
}

async fn update_status(
    tasks: &Arc<RwLock<HashMap<String, TorrentTask>>>,
    base_dir: &Path,
    event_tx: &broadcast::Sender<TorrentTask>,
    task_id: &str,
    status: TaskStatus,
) -> Result<TorrentTask> {
    let task = {
        let mut guard = tasks.write().await;
        let task = guard.get_mut(task_id).context("Task was deleted")?;
        task.status = status;
        task.clone()
    };
    persist_task(base_dir, &task).await?;
    let _ = event_tx.send(task.clone());
    Ok(task)
}

async fn persist_task(base_dir: &Path, task: &TorrentTask) -> Result<()> {
    let base_dir = base_dir.to_path_buf();
    let task = task.clone();
    tokio::task::spawn_blocking(move || persist_task_sync(&base_dir, &task)).await??;
    Ok(())
}

fn persist_task_sync(base_dir: &Path, task: &TorrentTask) -> Result<()> {
    let task_dir = base_dir.join(&task.id);
    std::fs::create_dir_all(&task_dir)?;
    let stored = PersistedTorrentTask {
        owner_id: task.owner_id.clone(),
        task: task.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&stored)?;
    let temporary = task_dir.join("task.json.tmp");
    let destination = task_dir.join("task.json");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, destination)?;
    Ok(())
}

fn load_persisted_tasks(base_dir: &Path) -> HashMap<String, TorrentTask> {
    let mut tasks = HashMap::new();
    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return tasks;
    };
    for entry in entries.flatten() {
        let task_file = entry.path().join("task.json");
        let Ok(bytes) = std::fs::read(&task_file) else {
            continue;
        };
        let Ok(mut stored) = serde_json::from_slice::<PersistedTorrentTask>(&bytes) else {
            continue;
        };
        stored.task.owner_id = stored.owner_id;
        if matches!(
            stored.task.status,
            TaskStatus::Queued | TaskStatus::Downloading { .. } | TaskStatus::Remuxing { .. }
        ) {
            stored.task.status = TaskStatus::Failed {
                reason: "Download was interrupted by a server restart. Delete it and start again."
                    .to_string(),
            };
            stored.task.completed_at = Some(unix_now());
            let _ = persist_task_sync(base_dir, &stored.task);
        }
        if let TaskStatus::Ready { ref file_name, .. } = stored.task.status {
            if !base_dir.join(&stored.task.id).join(file_name).is_file() {
                stored.task.status = TaskStatus::Failed {
                    reason: "The completed media file is missing.".to_string(),
                };
                stored.task.completed_at = Some(unix_now());
                let _ = persist_task_sync(base_dir, &stored.task);
            }
        }
        tasks.insert(stored.task.id.clone(), stored.task);
    }
    tasks
}

fn user_safe_task_error(error: &anyhow::Error) -> String {
    let text = format!("{error:#}");
    if text.contains("aria2") || text.contains("torrent client") {
        "The torrent client could not download this release. Check that aria2 is installed and the torrent still has reachable peers.".to_string()
    } else if text.contains("FFmpeg") || text.contains("FFprobe") || text.contains("H.264") {
        "The downloaded media could not be converted into a browser-playable MP4.".to_string()
    } else if text.contains("archive") || text.contains("Archive") {
        "The downloaded archive could not be extracted safely.".to_string()
    } else if text.contains("No playable video") {
        "No playable film or episode was found in the downloaded files.".to_string()
    } else {
        text.chars().take(280).collect()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(unix)]
fn failed_exit_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(1 << 8)
}

#[cfg(windows)]
fn failed_exit_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable_path(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn validates_magnets_and_torrent_urls() {
        assert!(valid_magnet(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(valid_torrent_url("https://example.test/file.torrent"));
        assert!(!valid_magnet("magnet:?dn=missing-hash"));
        assert!(!valid_torrent_url("file:///tmp/movie.torrent"));
        assert!(!valid_torrent_url("https://example.test/movie.mp4"));
    }

    #[test]
    fn sanitizes_long_and_unsafe_titles() {
        let value = sanitize_file_stem("../../A very long: film? title / episode 01");
        assert!(!value.contains('/'));
        assert!(!value.starts_with('.'));
        assert!(value.len() <= 120);
    }

    #[test]
    fn detects_external_subtitle_languages() {
        assert_eq!(
            detect_subtitle_language(Path::new("Movie.VietSub.srt")),
            Some("vi")
        );
        assert_eq!(
            detect_subtitle_language(Path::new("Movie.eng.vtt")),
            Some("en")
        );
        assert_eq!(detect_subtitle_language(Path::new("Movie.jpn.ass")), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn downloads_and_remuxes_a_real_video_fixture() {
        use std::os::unix::fs::PermissionsExt;

        let ffmpeg = match executable_path("ffmpeg") {
            Some(path) => path,
            None => return,
        };
        let ffprobe = match executable_path("ffprobe") {
            Some(path) => path,
            None => return,
        };
        let archive_tool = match executable_path("7z") {
            Some(path) => path,
            None => return,
        };
        let directory = tempfile::tempdir().unwrap();
        let source_video = directory.path().join("fixture-source.mp4");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=24",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=44100",
                "-t",
                "31",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-b:v",
                "600k",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
            ])
            .arg(&source_video)
            .status()
            .unwrap();
        assert!(generated.success());
        assert!(std::fs::metadata(&source_video).unwrap().len() > 1024 * 1024);
        let archive_file = directory.path().join("fixture-release.7z");
        let archived = std::process::Command::new(&archive_tool)
            .arg("a")
            .arg("-bd")
            .arg(&archive_file)
            .arg(&source_video)
            .status()
            .unwrap();
        assert!(archived.success());

        let fake_client = directory.path().join("fake-aria2c.sh");
        std::fs::write(
            &fake_client,
            format!(
                "#!/bin/sh\nset -eu\ndir=''\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = '--dir' ]; then dir=$2; shift 2; else shift; fi\ndone\ncp '{}' \"$dir/downloaded.7z\"\n",
                archive_file.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&fake_client).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fake_client, permissions).unwrap();

        let manager = TorrentTaskManager::with_tools(
            directory.path().join("tasks"),
            ToolPaths {
                torrent_client: fake_client,
                ffmpeg,
                ffprobe: ffprobe.clone(),
                archive: archive_tool,
                unrar: PathBuf::from("false"),
            },
        );
        let task = manager
            .create_task(
                "owner-a",
                CreateTaskRequest {
                    title: "Fixture Film".to_string(),
                    magnet_url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
                        .to_string(),
                    torrent_url: None,
                    expected_size_bytes: Some(std::fs::metadata(&archive_file).unwrap().len()),
                    sub_pref: Some("all".to_string()),
                },
            )
            .await
            .unwrap();

        let ready = tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                if let Some(task) = manager.get_task("owner-a", &task.id).await {
                    if matches!(
                        task.status,
                        TaskStatus::Ready { .. } | TaskStatus::Failed { .. }
                    ) {
                        break task;
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        let TaskStatus::Ready {
            file_name,
            file_size,
            ..
        } = ready.status
        else {
            panic!("real fixture pipeline failed: {:?}", ready.status);
        };
        assert!(file_size > 1024 * 1024);
        let output = directory
            .path()
            .join("tasks")
            .join(&task.id)
            .join(file_name);
        let probe = probe_media(&ffprobe, &output).await.unwrap();
        assert_eq!(probe.video_codec.as_deref(), Some("h264"));
        assert!(manager.get_task("owner-b", &task.id).await.is_none());
    }

    #[tokio::test]
    async fn persisted_tasks_remain_owner_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let task = TorrentTask {
            id: "task-1".to_string(),
            title: "Film".to_string(),
            magnet_url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567".to_string(),
            status: TaskStatus::Failed {
                reason: "test".to_string(),
            },
            created_at: unix_now(),
            completed_at: Some(unix_now()),
            sub_pref: Some("all".to_string()),
            owner_id: "owner-a".to_string(),
        };
        persist_task(directory.path(), &task).await.unwrap();
        let manager = TorrentTaskManager::new(directory.path().to_path_buf());

        assert_eq!(manager.list_tasks("owner-a").await.len(), 1);
        assert!(manager.list_tasks("owner-b").await.is_empty());
        assert!(manager.get_task("owner-b", "task-1").await.is_none());
    }
}
