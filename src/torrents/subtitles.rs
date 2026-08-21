use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleMatch {
    pub id: String,
    pub title: String,
    pub language: String,
    pub language_code: String,
    pub download_url: String,
    pub format: String,
    pub source: String,
}

pub struct SubtitleFinder {
    client: Client,
}

impl Default for SubtitleFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtitleFinder {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Search for subtitles across providers by movie/show/anime title
    pub async fn search_subtitles(
        &self,
        query: &str,
        lang_code: Option<&str>,
    ) -> Result<Vec<SubtitleMatch>> {
        let mut matches = Vec::new();

        // 1. Search OpenSubtitles / SubDL / YTS-subs mirrors
        if let Ok(yts_matches) = self.search_yts_subs(query, lang_code).await {
            matches.extend(yts_matches);
        }

        if let Ok(opensub_matches) = self.search_opensubtitles_v3(query, lang_code).await {
            matches.extend(opensub_matches);
        }

        // Deduplicate matches by language and title
        matches.sort_by(|a, b| {
            if a.language_code == b.language_code {
                a.title.cmp(&b.title)
            } else {
                a.language_code.cmp(&b.language_code)
            }
        });

        matches.dedup_by(|a, b| {
            a.id == b.id || (a.language_code == b.language_code && a.title == b.title)
        });

        Ok(matches)
    }

    async fn search_yts_subs(
        &self,
        query: &str,
        lang_filter: Option<&str>,
    ) -> Result<Vec<SubtitleMatch>> {
        let mut url = Url::parse("https://yts-subs.com/ajax/search")?;
        url.query_pairs_mut().append_pair("q", query);

        let response = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct YtsSearchResponse {
            #[serde(default)]
            movies: Option<Vec<YtsSearchMovie>>,
        }

        #[derive(Deserialize)]
        struct YtsSearchMovie {
            title: Option<String>,
            link: Option<String>,
        }

        let body: YtsSearchResponse = response
            .json()
            .await
            .unwrap_or(YtsSearchResponse { movies: None });
        let mut results = Vec::new();

        if let Some(movies) = body.movies {
            for movie in movies.into_iter().take(2) {
                if let (Some(title), Some(link)) = (movie.title, movie.link) {
                    let page_url = format!("https://yts-subs.com{}", link);
                    if let Ok(subs) = self
                        .scrape_yts_subs_page(&page_url, &title, lang_filter)
                        .await
                    {
                        results.extend(subs);
                    }
                }
            }
        }

        Ok(results)
    }

    async fn scrape_yts_subs_page(
        &self,
        page_url: &str,
        movie_title: &str,
        lang_filter: Option<&str>,
    ) -> Result<Vec<SubtitleMatch>> {
        let response = self
            .client
            .get(page_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let html = response.text().await.unwrap_or_default();
        let mut results = Vec::new();

        // Parse HTML table rows for subtitles
        let re = regex::Regex::new(r#"(?s)<tr[^>]*>.*?<span class="sub-lang">([^<]+)</span>.*?<a href="(/subtitles/[^"]+)"[^>]*>([^<]+)</a>"#).unwrap();

        for cap in re.captures_iter(&html) {
            let lang = cap.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            let sub_link = cap.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
            let sub_title = cap.get(3).map(|m| m.as_str().trim()).unwrap_or_default();

            let (lang_name, lang_code) = match lang.to_lowercase().as_str() {
                "english" | "en" => ("English".to_string(), "en".to_string()),
                "vietnamese" | "vi" | "tiếng việt" | "tieng viet" => {
                    ("Vietnamese".to_string(), "vi".to_string())
                }
                other => (
                    other.to_string(),
                    other.chars().take(2).collect::<String>().to_lowercase(),
                ),
            };

            if let Some(target_code) = lang_filter {
                if lang_code != target_code {
                    continue;
                }
            } else if lang_code != "en" && lang_code != "vi" {
                continue;
            }

            let download_url = format!(
                "https://yts-subs.com/download{}",
                sub_link.trim_start_matches("/subtitles")
            );

            results.push(SubtitleMatch {
                id: format!("yts-sub-{}-{}", lang_code, results.len()),
                title: format!("{} [{}] - {}", movie_title, lang_name, sub_title),
                language: lang_name,
                language_code: lang_code,
                download_url,
                format: "srt".to_string(),
                source: "YTS-Subs".to_string(),
            });
        }

        Ok(results)
    }

    async fn search_opensubtitles_v3(
        &self,
        query: &str,
        lang_filter: Option<&str>,
    ) -> Result<Vec<SubtitleMatch>> {
        // OpenSubtitles REST search
        let encoded_q = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let url = Url::parse(&format!(
            "https://rest.opensubtitles.org/search/query-{}",
            encoded_q
        ))?;

        let response = self
            .client
            .get(url)
            .header("User-Agent", "TemporaryUserAgent")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct OpenSubItem {
            #[serde(rename = "SubFileName")]
            sub_file_name: Option<String>,
            #[serde(rename = "ISO639")]
            iso639: Option<String>,
            #[serde(rename = "LanguageName")]
            language_name: Option<String>,
            #[serde(rename = "SubDownloadLink")]
            sub_download_link: Option<String>,
            #[serde(rename = "SubFormat")]
            sub_format: Option<String>,
            #[serde(rename = "IDSubtitle")]
            id_subtitle: Option<String>,
        }

        let items: Vec<OpenSubItem> = response.json().await.unwrap_or_default();
        let mut results = Vec::new();

        for item in items {
            let code = item.iso639.unwrap_or_default().to_lowercase();
            if let Some(target) = lang_filter {
                if code != target {
                    continue;
                }
            } else if code != "en" && code != "vi" {
                continue;
            }

            let download_url = match item.sub_download_link {
                Some(url) if !url.is_empty() => url,
                _ => continue,
            };

            let lang_name = item.language_name.unwrap_or_else(|| {
                if code == "vi" {
                    "Vietnamese".to_string()
                } else {
                    "English".to_string()
                }
            });
            let format = item.sub_format.unwrap_or_else(|| "srt".to_string());
            let title = item
                .sub_file_name
                .unwrap_or_else(|| format!("{} [{}]", query, lang_name));
            let id = format!(
                "opensub-{}",
                item.id_subtitle
                    .unwrap_or_else(|| format!("{}-{}", code, results.len()))
            );

            results.push(SubtitleMatch {
                id,
                title,
                language: lang_name,
                language_code: code,
                download_url,
                format,
                source: "OpenSubtitles".to_string(),
            });
        }

        Ok(results)
    }

    /// Fetch raw subtitle content as string (converts or normalizes to SRT / VTT)
    pub async fn fetch_subtitle_text(&self, download_url: &str) -> Result<String> {
        let resp = self
            .client
            .get(download_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await
            .context("Failed to fetch subtitle file")?;

        let resp = resp
            .error_for_status()
            .context("Subtitle download returned an error")?;
        let bytes = resp.bytes().await?;
        anyhow::ensure!(
            !bytes.starts_with(&[b'P', b'K', 0x03, 0x04]) && !bytes.starts_with(&[0x1f, 0x8b]),
            "Subtitle provider returned an archive; use the task extractor instead"
        );
        String::from_utf8(bytes.to_vec()).context("Subtitle file was not UTF-8 text")
    }
}
