use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::OnceLock, time::Duration};
use tokio::sync::RwLock;

const ANILIST_API: &str = "https://graphql.anilist.co";
const ANISKIP_API: &str = "https://api.aniskip.com/v1/skip-times";
static MAL_ID_CACHE: OnceLock<RwLock<HashMap<i64, u64>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkipTime {
    pub skip_type: String,
    pub start_time: f64,
    pub end_time: f64,
    pub episode_length: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AniListResponse {
    data: Option<AniListData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AniListData {
    media: Option<AniListMedia>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AniListMedia {
    id_mal: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AniSkipResponse {
    found: bool,
    #[serde(default)]
    results: Vec<AniSkipResult>,
}

#[derive(Debug, Deserialize)]
struct AniSkipResult {
    skip_type: String,
    interval: AniSkipInterval,
    episode_length: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AniSkipInterval {
    start_time: f64,
    end_time: f64,
}

pub async fn fetch_skip_times(catalog_id: i64, episode_number: u32) -> Result<Vec<SkipTime>> {
    anyhow::ensure!(catalog_id > 0, "catalog id is required for AniSkip");
    anyhow::ensure!(episode_number > 0, "episode number is required for AniSkip");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .use_rustls_tls()
        .http1_only()
        .user_agent("any-watch/1.0")
        .build()
        .context("failed to build AniSkip client")?;
    let Some(id_mal) = resolve_mal_id(&client, catalog_id).await? else {
        return Ok(Vec::new());
    };
    let response = client
        .get(format!("{ANISKIP_API}/{id_mal}/{episode_number}"))
        .query(&[("types[]", "op"), ("types[]", "ed")])
        .send()
        .await
        .context("AniSkip request failed")?
        .error_for_status()
        .context("AniSkip returned an error")?
        .json::<AniSkipResponse>()
        .await
        .context("AniSkip returned an invalid response")?;

    if !response.found {
        return Ok(Vec::new());
    }

    Ok(normalize_results(response.results))
}

async fn resolve_mal_id(client: &Client, catalog_id: i64) -> Result<Option<u64>> {
    let cache = MAL_ID_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(id) = cache.read().await.get(&catalog_id).copied() {
        return Ok(Some(id));
    }
    let response = client
        .post(ANILIST_API)
        .json(&serde_json::json!({
            "query": "query ($id: Int) { Media(id: $id, type: ANIME) { idMal } }",
            "variables": { "id": catalog_id }
        }))
        .send()
        .await
        .context("AniList id mapping request failed")?
        .error_for_status()
        .context("AniList id mapping returned an error")?
        .json::<AniListResponse>()
        .await
        .context("AniList id mapping returned an invalid response")?;
    let id = response
        .data
        .and_then(|data| data.media)
        .and_then(|media| media.id_mal);
    if let Some(id) = id {
        cache.write().await.insert(catalog_id, id);
    }
    Ok(id)
}

fn normalize_results(results: Vec<AniSkipResult>) -> Vec<SkipTime> {
    let mut ranges = results
        .into_iter()
        .filter(|item| matches!(item.skip_type.as_str(), "op" | "ed" | "recap"))
        .filter(|item| {
            item.interval.start_time.is_finite()
                && item.interval.end_time.is_finite()
                && item.interval.start_time >= 0.0
                && item.interval.end_time > item.interval.start_time
                && item.interval.end_time <= 6.0 * 60.0 * 60.0
        })
        .map(|item| SkipTime {
            skip_type: item.skip_type,
            start_time: item.interval.start_time,
            end_time: item.interval.end_time,
            episode_length: item
                .episode_length
                .filter(|length| length.is_finite() && *length > 0.0),
        })
        .collect::<Vec<_>>();
    ranges.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_or_unknown_skip_ranges() {
        let ranges = normalize_results(vec![
            AniSkipResult {
                skip_type: "op".into(),
                interval: AniSkipInterval {
                    start_time: 90.0,
                    end_time: 150.0,
                },
                episode_length: Some(1420.0),
            },
            AniSkipResult {
                skip_type: "preview".into(),
                interval: AniSkipInterval {
                    start_time: 1400.0,
                    end_time: 1450.0,
                },
                episode_length: None,
            },
            AniSkipResult {
                skip_type: "ed".into(),
                interval: AniSkipInterval {
                    start_time: 1500.0,
                    end_time: 1490.0,
                },
                episode_length: None,
            },
        ]);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].skip_type, "op");
        assert_eq!(ranges[0].episode_length, Some(1420.0));
    }

    #[tokio::test]
    #[ignore = "live AniList and AniSkip smoke test"]
    async fn live_one_piece_skip_times_smoke() {
        let ranges = fetch_skip_times(21, 1)
            .await
            .expect("One Piece skip times should load");
        assert!(ranges.iter().any(|range| range.skip_type == "op"));
    }
}
