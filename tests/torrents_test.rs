use any_watch_core::torrents::engine::{CreateTaskRequest, TorrentTaskManager};
use any_watch_core::torrents::hub::TorrentSearchHub;
use any_watch_core::torrents::{detect_quality, detect_subtitles, format_bytes};

#[test]
fn test_torrent_search_hub_initialization() {
    let hub = TorrentSearchHub::new();
    let providers = hub.available_providers();
    assert_eq!(providers, ["AnimeTosho", "Nyaa", "YTS", "ThePirateBay"]);
}

#[tokio::test]
#[ignore = "requires live torrent indexer network access"]
async fn test_live_torrent_search_pagination() {
    let hub = TorrentSearchHub::new();
    let first = hub
        .search(
            "Big Buck Bunny",
            any_watch_core::torrents::TorrentCategory::All,
            None,
            1,
        )
        .await
        .expect("live torrent search page one");
    assert!(
        !first.is_empty(),
        "legal sample title should return a torrent result"
    );

    let second = hub
        .search(
            "Big Buck Bunny",
            any_watch_core::torrents::TorrentCategory::All,
            None,
            2,
        )
        .await
        .expect("live torrent search page two");
    assert!(first.iter().all(|item| {
        !second
            .iter()
            .any(|candidate| candidate.id == item.id && candidate.source == item.source)
    }));
}

#[tokio::test]
async fn test_torrent_task_rejects_invalid_download_sources() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let manager = TorrentTaskManager::new(temp_dir.path().to_path_buf());

    let result = manager
        .create_task(
            "owner-a",
            CreateTaskRequest {
                title: "Invalid download".to_string(),
                magnet_url: "https://example.test/movie.mp4".to_string(),
                torrent_url: None,
                expected_size_bytes: Some(1024),
                sub_pref: Some("all".to_string()),
            },
        )
        .await;

    assert!(result.is_err());
    assert!(manager.list_tasks("owner-a").await.is_empty());
}

#[test]
fn test_detect_quality_and_subtitles_edge_cases() {
    let quality = detect_quality("Cyberpunk.Edgerunners.S01.2160p.HDR.x265");
    assert_eq!(quality, Some("4K".to_string()));

    let (eng, vi) = detect_subtitles("[AnimeVietsub] Solo Leveling - Episode 05 [1080p]");
    assert!(vi);
    assert!(!eng);

    let (eng_only, vi_none) =
        detect_subtitles("Stranger Things S04 1080p NF WEB-DL DDP5.1 x264-NTb");
    assert!(!eng_only);
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
