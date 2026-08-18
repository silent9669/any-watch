use std::time::Duration;

use any_watch_core::torrents::engine::{CreateTaskRequest, TaskStatus, TorrentTaskManager};
use any_watch_core::torrents::hub::TorrentSearchHub;
use any_watch_core::torrents::subtitles::SubtitleFinder;
use any_watch_core::torrents::{detect_quality, detect_subtitles, format_bytes};

#[tokio::test]
async fn test_torrent_search_hub_initialization() {
    let hub = TorrentSearchHub::new();
    let providers = hub.available_providers();
    assert_eq!(providers.len(), 4);
    assert_eq!(providers[0], "AnimeTosho");
    assert_eq!(providers[1], "Nyaa");
    assert_eq!(providers[2], "YTS");
    assert_eq!(providers[3], "ThePirateBay");
}

#[tokio::test]
async fn test_torrent_task_lifecycle_and_sandboxing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    let manager = TorrentTaskManager::new(base_path.clone());
    let mut rx = manager.subscribe();

    let req = CreateTaskRequest {
        title: "Test Movie 2024 1080p".to_string(),
        magnet_url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Test"
            .to_string(),
        sub_pref: Some("all".to_string()),
    };

    let task = manager.create_task(req).await.expect("create task");
    assert_eq!(task.title, "Test Movie 2024 1080p");
    assert_eq!(task.status, TaskStatus::Queued);

    // List tasks
    let tasks = manager.list_tasks().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task.id);

    // Wait for task to progress through downloading -> remuxing -> ready
    let mut reached_ready = false;
    let timeout = tokio::time::sleep(Duration::from_secs(12));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            event = rx.recv() => {
                if let Ok(updated) = event {
                    if updated.id == task.id {
                        if let TaskStatus::Ready { ref file_name, has_mp4, ref subtitles, .. } = updated.status {
                            assert!(has_mp4);
                            assert!(file_name.ends_with(".mp4"));
                            assert_eq!(subtitles.len(), 2);
                            assert!(subtitles.iter().any(|s| s.language_code == "vi"));
                            assert!(subtitles.iter().any(|s| s.language_code == "en"));
                            reached_ready = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    assert!(
        reached_ready,
        "Task did not reach Ready status within timeout"
    );

    // Verify task file path
    let file_opt = manager.get_task_file_path(&task.id).await;
    assert!(file_opt.is_some());
    let (file_path, file_name) = file_opt.unwrap();
    assert!(file_path.exists());
    assert!(file_name.ends_with(".mp4"));

    // Verify subtitle path
    let vi_sub_opt = manager.get_subtitle_file_path(&task.id, "vi").await;
    assert!(vi_sub_opt.is_some());
    let (vi_path, _) = vi_sub_opt.unwrap();
    assert!(vi_path.exists());

    let en_sub_opt = manager.get_subtitle_file_path(&task.id, "en").await;
    assert!(en_sub_opt.is_some());
    let (en_path, _) = en_sub_opt.unwrap();
    assert!(en_path.exists());

    // Test deleting task cleans up directory
    let deleted = manager.delete_task(&task.id).await.expect("delete task");
    assert!(deleted);
    assert!(!file_path.exists());
    assert_eq!(manager.list_tasks().await.len(), 0);
}

#[tokio::test]
async fn test_subtitle_finder_initialization() {
    let finder = SubtitleFinder::new();
    // Test that searching subtitles runs without panics
    let results = finder.search_subtitles("Attack on Titan", Some("vi")).await;
    assert!(results.is_ok());
}

#[test]
fn test_detect_quality_and_subtitles_edge_cases() {
    let q = detect_quality("Cyberpunk.Edgerunners.S01.2160p.HDR.x265");
    assert_eq!(q, Some("4K".to_string()));

    let (eng, vi) = detect_subtitles("[AnimeVietsub] Solo Leveling - Episode 05 [1080p]");
    assert!(vi);
    assert!(eng);

    let (eng_only, vi_none) =
        detect_subtitles("Stranger Things S04 1080p NF WEB-DL DDP5.1 x264-NTb");
    assert!(eng_only);
    assert!(!vi_none);
}

#[test]
fn test_format_bytes_scales() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(2048), "2 KB");
    assert_eq!(format_bytes(1024 * 1024 * 150), "150.0 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 4), "4.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 * 3), "3.00 TB");
}
