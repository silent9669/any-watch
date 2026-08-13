use anyhow::{Context, Result};
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

const ANILIST_API: &str = "https://graphql.anilist.co";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogAnime {
    pub catalog_id: i64,
    pub title: String,
    pub native_title: Option<String>,
    #[serde(default)]
    pub synonyms: Vec<String>,
    pub description: Option<String>,
    pub cover_url: String,
    pub banner_url: Option<String>,
    pub genres: Vec<String>,
    pub total_episodes: Option<u32>,
    pub score: Option<u32>,
    pub format: Option<String>,
    pub season_year: Option<u32>,
    pub season: Option<String>,
    pub status: Option<String>,
    pub popularity: Option<u64>,
    pub trending: Option<u64>,
    pub personal_match: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFilters {
    pub genre: Option<String>,
    pub season: Option<String>,
    pub year: Option<u32>,
    pub format: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub items: Vec<CatalogAnime>,
    pub page: u32,
    pub has_next_page: bool,
}

#[derive(Debug, Clone)]
pub struct TastePreference {
    pub genres: Vec<String>,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryCatalog {
    pub trending: Vec<CatalogAnime>,
    pub popular_this_season: Vec<CatalogAnime>,
    pub genres: Vec<String>,
}

#[derive(Clone)]
pub struct CatalogClient {
    client: reqwest::Client,
}

impl Default for CatalogClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .build()
                .expect("failed to build AniList catalog client"),
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CatalogAnime>> {
        self.query_page(
            CATALOG_PAGE_QUERY,
            serde_json::json!({
                "search": query,
                "perPage": limit.clamp(1, 30),
                "genre": null,
                "sort": ["SEARCH_MATCH"]
            }),
        )
        .await
    }

    pub async fn discovery(&self) -> Result<DiscoveryCatalog> {
        let now = Utc::now();
        let season = match now.month() {
            12 | 1 | 2 => "WINTER",
            3..=5 => "SPRING",
            6..=8 => "SUMMER",
            _ => "FALL",
        };
        let response = self
            .post(
                DISCOVERY_QUERY,
                serde_json::json!({"season": season, "year": now.year()}),
            )
            .await?;
        Ok(DiscoveryCatalog {
            trending: parse_media_list(&response["data"]["trending"]["media"]),
            popular_this_season: parse_media_list(&response["data"]["seasonal"]["media"]),
            genres: vec![
                "Action".into(),
                "Adventure".into(),
                "Comedy".into(),
                "Drama".into(),
                "Fantasy".into(),
                "Mystery".into(),
                "Romance".into(),
                "Sci-Fi".into(),
                "Sports".into(),
                "Supernatural".into(),
            ],
        })
    }

    pub async fn by_genre(&self, genre: &str, limit: usize) -> Result<Vec<CatalogAnime>> {
        self.query_page(
            CATALOG_PAGE_QUERY,
            serde_json::json!({
                "search": null,
                "genre": genre,
                "perPage": limit.clamp(1, 30),
                "sort": ["POPULARITY_DESC"]
            }),
        )
        .await
    }

    pub async fn by_ids(&self, ids: &[i64]) -> Result<Vec<CatalogAnime>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.query_page(
            CATALOG_IDS_QUERY,
            serde_json::json!({ "ids": ids, "perPage": ids.len().clamp(1, 50) }),
        )
        .await
    }

    pub async fn catalog(
        &self,
        filters: &CatalogFilters,
        sort: &str,
        page: u32,
        per_page: usize,
    ) -> Result<CatalogPage> {
        let sort = match sort {
            "personalMatch" | "trending" => "TRENDING_DESC",
            "popularity" => "POPULARITY_DESC",
            "score" => "SCORE_DESC",
            "newest" => "START_DATE_DESC",
            "title" => "TITLE_ROMAJI",
            _ => "TRENDING_DESC",
        };
        let (query, variables) =
            catalog_browser_request(filters, sort, page.max(1), per_page.clamp(1, 30));
        let response = self.post(&query, variables).await?;
        Ok(CatalogPage {
            items: parse_media_list(&response["data"]["Page"]["media"]),
            page: page.max(1),
            has_next_page: response["data"]["Page"]["pageInfo"]["hasNextPage"]
                .as_bool()
                .unwrap_or(false),
        })
    }

    async fn query_page(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<Vec<CatalogAnime>> {
        let response = self.post(query, variables).await?;
        Ok(parse_media_list(&response["data"]["Page"]["media"]))
    }

    async fn post(&self, query: &str, variables: serde_json::Value) -> Result<serde_json::Value> {
        let response = self
            .client
            .post(ANILIST_API)
            .json(&serde_json::json!({"query": query, "variables": variables}))
            .send()
            .await
            .context("AniList request failed")?;
        let status = response.status();
        let body: serde_json::Value = response.json().await.context("Invalid AniList response")?;
        if !status.is_success() || body.get("errors").is_some() {
            anyhow::bail!("AniList catalog error ({status}): {}", body["errors"]);
        }
        Ok(body)
    }
}

const CATALOG_PAGE_QUERY: &str = r#"
  query ($search: String, $genre: String, $perPage: Int, $sort: [MediaSort]) {
    Page(page: 1, perPage: $perPage) {
      media(search: $search, genre: $genre, type: ANIME, sort: $sort, isAdult: false) {
        ...CatalogFields
      }
    }
  }
  fragment CatalogFields on Media {
    id title { romaji english native } synonyms description(asHtml: false)
    coverImage { extraLarge large } bannerImage genres episodes averageScore format seasonYear season status popularity trending
  }
"#;

const CATALOG_IDS_QUERY: &str = r#"
  query ($ids: [Int], $perPage: Int) {
    Page(page: 1, perPage: $perPage) {
      media(id_in: $ids, type: ANIME, isAdult: false) { ...CatalogFields }
    }
  }
  fragment CatalogFields on Media {
    id title { romaji english native } synonyms description(asHtml: false)
    coverImage { extraLarge large } bannerImage genres episodes averageScore format seasonYear season status popularity trending
  }
"#;

fn catalog_browser_request(
    filters: &CatalogFilters,
    sort: &str,
    page: u32,
    per_page: usize,
) -> (String, serde_json::Value) {
    let mut declarations = vec![
        "$page: Int".to_string(),
        "$perPage: Int".to_string(),
        "$sort: [MediaSort]".to_string(),
    ];
    let mut arguments = vec![
        "type: ANIME".to_string(),
        "sort: $sort".to_string(),
        "isAdult: false".to_string(),
    ];
    let mut variables = serde_json::Map::from_iter([
        ("page".to_string(), serde_json::json!(page)),
        ("perPage".to_string(), serde_json::json!(per_page)),
        ("sort".to_string(), serde_json::json!([sort])),
    ]);
    let optional_filters = [
        ("genre", "String", "genre", filters.genre.clone()),
        ("season", "MediaSeason", "season", filters.season.clone()),
        ("format", "MediaFormat", "format", filters.format.clone()),
        ("status", "MediaStatus", "status", filters.status.clone()),
    ];
    for (variable, gql_type, argument, value) in optional_filters {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            declarations.push(format!("${variable}: {gql_type}"));
            arguments.push(format!("{argument}: ${variable}"));
            variables.insert(variable.to_string(), serde_json::json!(value));
        }
    }
    if let Some(year) = filters.year {
        declarations.push("$year: Int".to_string());
        arguments.push("seasonYear: $year".to_string());
        variables.insert("year".to_string(), serde_json::json!(year));
    }
    let query = format!(
        r#"
        query ({}) {{
          Page(page: $page, perPage: $perPage) {{
            pageInfo {{ hasNextPage }}
            media({}) {{
               id title {{ romaji english native }} synonyms description(asHtml: false)
              coverImage {{ extraLarge large }} bannerImage genres episodes averageScore
              format seasonYear season status popularity trending
            }}
          }}
        }}
        "#,
        declarations.join(", "),
        arguments.join(", ")
    );
    (query, serde_json::Value::Object(variables))
}

const DISCOVERY_QUERY: &str = r#"
  query ($season: MediaSeason, $year: Int) {
    trending: Page(page: 1, perPage: 18) {
      media(type: ANIME, sort: TRENDING_DESC, isAdult: false) { ...CatalogFields }
    }
    seasonal: Page(page: 1, perPage: 18) {
      media(type: ANIME, season: $season, seasonYear: $year, sort: POPULARITY_DESC, isAdult: false) {
        ...CatalogFields
      }
    }
  }
  fragment CatalogFields on Media {
    id title { romaji english native } synonyms description(asHtml: false)
    coverImage { extraLarge large } bannerImage genres episodes averageScore format seasonYear season status popularity trending
  }
"#;

fn parse_media_list(value: &serde_json::Value) -> Vec<CatalogAnime> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_media)
        .collect()
}

fn parse_media(media: &serde_json::Value) -> Option<CatalogAnime> {
    let catalog_id = media["id"].as_i64()?;
    let title = media["title"]["english"]
        .as_str()
        .or_else(|| media["title"]["romaji"].as_str())?
        .to_string();
    let cover_url = media["coverImage"]["extraLarge"]
        .as_str()
        .or_else(|| media["coverImage"]["large"].as_str())?
        .to_string();
    Some(CatalogAnime {
        catalog_id,
        title,
        native_title: media["title"]["native"].as_str().map(str::to_string),
        synonyms: media["synonyms"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        description: media["description"].as_str().map(str::to_string),
        cover_url,
        banner_url: media["bannerImage"].as_str().map(str::to_string),
        genres: media["genres"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        total_episodes: media["episodes"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok()),
        score: media["averageScore"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok()),
        format: media["format"].as_str().map(str::to_string),
        season_year: media["seasonYear"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok()),
        season: media["season"].as_str().map(str::to_string),
        status: media["status"].as_str().map(str::to_string),
        popularity: media["popularity"].as_u64(),
        trending: media["trending"].as_u64(),
        personal_match: None,
    })
}

pub fn apply_personal_matches(items: &mut [CatalogAnime], preferences: &[TastePreference]) {
    if preferences.is_empty() {
        for item in items {
            item.personal_match = item.score;
        }
        return;
    }

    let mut genre_weights: HashMap<String, f64> = HashMap::new();
    for preference in preferences {
        for genre in &preference.genres {
            *genre_weights.entry(genre.to_lowercase()).or_default() += preference.weight;
        }
    }
    let max_genre_weight = genre_weights
        .values()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let max_popularity = items
        .iter()
        .filter_map(|item| item.popularity)
        .max()
        .unwrap_or(1) as f64;

    for item in items {
        let affinity = item
            .genres
            .iter()
            .filter_map(|genre| genre_weights.get(&genre.to_lowercase()))
            .copied()
            .sum::<f64>()
            / (item.genres.len().max(1) as f64 * max_genre_weight);
        let score = item.score.unwrap_or(0) as f64 / 100.0;
        let popularity = item.popularity.unwrap_or(0) as f64 / max_popularity;
        item.personal_match = Some(
            ((0.65 * affinity + 0.25 * score + 0.10 * popularity) * 100.0)
                .round()
                .clamp(0.0, 99.0) as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_match_prefers_affinity_and_falls_back_to_score() {
        let mut items = vec![CatalogAnime {
            catalog_id: 1,
            title: "Example".into(),
            native_title: None,
            synonyms: Vec::new(),
            description: None,
            cover_url: "cover".into(),
            banner_url: None,
            genres: vec!["Action".into()],
            total_episodes: None,
            score: Some(80),
            format: Some("TV".into()),
            season_year: Some(2026),
            season: None,
            status: None,
            popularity: Some(100),
            trending: Some(10),
            personal_match: None,
        }];
        apply_personal_matches(&mut items, &[]);
        assert_eq!(items[0].personal_match, Some(80));
        apply_personal_matches(
            &mut items,
            &[TastePreference {
                genres: vec!["Action".into()],
                weight: 3.0,
            }],
        );
        assert_eq!(items[0].personal_match, Some(95));
    }

    #[test]
    fn parses_catalog_media_without_optional_fields() {
        let item = serde_json::json!({
            "id": 21,
            "title": {"english": "One Piece", "romaji": "One Piece", "native": null},
            "coverImage": {"extraLarge": "https://example.com/cover.jpg", "large": null},
            "bannerImage": null,
            "genres": ["Action"],
            "episodes": null,
            "averageScore": 88,
            "format": "TV",
            "seasonYear": 1999
        });
        let parsed = parse_media(&item).expect("catalog item");
        assert_eq!(parsed.catalog_id, 21);
        assert_eq!(parsed.title, "One Piece");
        assert_eq!(parsed.score, Some(88));
    }

    #[test]
    fn catalog_request_omits_inactive_filters() {
        let (query, variables) =
            catalog_browser_request(&CatalogFilters::default(), "TRENDING_DESC", 1, 24);
        assert!(!query.contains("genre: $genre"));
        assert!(!query.contains("seasonYear: $year"));
        assert!(variables.get("genre").is_none());

        let filters = CatalogFilters {
            genre: Some("Action".to_string()),
            year: Some(2026),
            ..CatalogFilters::default()
        };
        let (query, variables) = catalog_browser_request(&filters, "SCORE_DESC", 2, 12);
        assert!(query.contains("genre: $genre"));
        assert!(query.contains("seasonYear: $year"));
        assert_eq!(variables["genre"], "Action");
        assert_eq!(variables["year"], 2026);
    }
}
