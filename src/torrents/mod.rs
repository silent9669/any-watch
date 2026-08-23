use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod animetosho;
pub mod engine;
pub mod eztv;
pub mod hub;
pub mod nyaa;
pub mod piratebay;
pub mod solidtorrents;
pub mod subtitles;
pub mod tokyotoshokan;
pub mod yts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TorrentCategory {
    #[default]
    All,
    Anime,
    Movies,
    Tv,
    Documentaries,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSearchResult {
    pub id: String,
    pub title: String,
    pub magnet_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent_url: Option<String>,
    pub source: String,
    pub category: String,
    pub size_bytes: u64,
    pub formatted_size: String,
    pub seeds: u32,
    pub peers: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_date: Option<String>,
    pub has_engsub: bool,
    pub has_vietsub: bool,
}

#[async_trait]
pub trait TorrentSearchProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn supported_categories(&self) -> &'static [TorrentCategory];
    async fn search(
        &self,
        query: &str,
        category: TorrentCategory,
        page: u32,
    ) -> Result<Vec<TorrentSearchResult>>;
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn detect_quality(title: &str) -> Option<String> {
    let lower = title.to_lowercase();
    if lower.contains("2160p") || lower.contains("4k") || lower.contains("uhd") {
        Some("4K".to_string())
    } else if lower.contains("1080p") || lower.contains("fhd") {
        Some("1080p".to_string())
    } else if lower.contains("720p") || lower.contains("hd") {
        Some("720p".to_string())
    } else if lower.contains("480p") || lower.contains("sd") {
        Some("480p".to_string())
    } else {
        None
    }
}

pub fn detect_subtitles(title: &str) -> (bool, bool) {
    let lower = title.to_lowercase();
    let has_vietsub = lower.contains("vietsub")
        || lower.contains("viet sub")
        || lower.contains("vietnamese")
        || lower.contains("thuyết minh")
        || lower.contains("thuyet minh")
        || lower.contains("long tieng")
        || lower.contains("lồng tiếng");

    let has_foreign_only = lower.contains("italian")
        || lower.contains("german")
        || lower.contains("french")
        || lower.contains("spanish")
        || lower.contains(".ita.")
        || lower.contains(".ger.")
        || lower.contains(".fre.")
        || lower.contains(".spa.")
        || lower.contains("[ita]")
        || lower.contains("[ger]")
        || lower.contains("[fre]")
        || lower.contains("[spa]")
        || lower.contains(" raw ")
        || lower.starts_with("raw ")
        || lower.starts_with("raw.")
        || lower.ends_with(" raw")
        || lower.contains(".raw.")
        || lower.contains("[raw]");

    let has_engsub = !has_foreign_only
        && (lower.contains("engsub")
            || lower.contains("eng sub")
            || lower.contains("english")
            || lower.contains("multi-sub")
            || lower.contains("multisub")
            || lower.contains("dual audio")
            || lower.contains("subbed")
            || lower.contains("[sub]"));

    (has_engsub, has_vietsub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1024 * 1024 * 50), "50.0 MB");
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 * 2 + 500 * 1024 * 1024),
            "2.49 GB"
        );
    }

    #[test]
    fn test_detect_quality() {
        assert_eq!(
            detect_quality("[SubsPlease] Frieren - 28 (1080p) [ABCD1234].mkv"),
            Some("1080p".to_string())
        );
        assert_eq!(
            detect_quality("Oppenheimer.2023.2160p.UHD.HDR.x265.mp4"),
            Some("4K".to_string())
        );
        assert_eq!(
            detect_quality("One.Piece.E1100.720p.HD.mkv"),
            Some("720p".to_string())
        );
        assert_eq!(
            detect_quality("Sample.Video.480p.avi"),
            Some("480p".to_string())
        );
        assert_eq!(detect_quality("Unknown.Video.mkv"), None);
    }

    #[test]
    fn test_detect_subtitles() {
        let (eng, viet) = detect_subtitles("[Vietsub] Kimetsu no Yaiba - Ep 01 [1080p].mkv");
        assert!(viet);
        assert!(!eng);

        let (eng2, viet2) = detect_subtitles("Attack.on.Titan.S04E28.EngSub.1080p.mkv");
        assert!(!viet2);
        assert!(eng2);

        let (eng3, viet3) = detect_subtitles("Raw.Anime.Movie.2024.1080p.mkv");
        assert!(!viet3);
        assert!(!eng3);
    }

    #[test]
    fn test_torrent_category_serde() {
        let json = serde_json::to_string(&TorrentCategory::Anime).unwrap();
        assert_eq!(json, "\"anime\"");
        let cat: TorrentCategory = serde_json::from_str("\"movies\"").unwrap();
        assert_eq!(cat, TorrentCategory::Movies);
    }
}
