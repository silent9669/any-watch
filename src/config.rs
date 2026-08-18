use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub sources: SourcesConfig,

    #[serde(default)]
    pub prowlarr: Option<ProwlarrConfig>,

    #[serde(default)]
    pub invidious: Option<InvidiousConfig>,

    #[serde(default)]
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default)]
    pub invidious: bool,

    #[serde(default = "default_true")]
    pub anidb: bool,

    #[serde(default)]
    pub anizone: bool,

    #[serde(default)]
    pub allanime: bool,

    #[serde(default = "default_true")]
    pub animegg: bool,

    #[serde(default)]
    pub moviebox: bool,

    #[serde(default = "default_true")]
    pub kkphim: bool,

    #[serde(default = "default_true")]
    pub ophim: bool,

    #[serde(default)]
    pub animevietsub: bool,

    #[serde(default)]
    pub animetvn: bool,

    #[serde(default = "default_true")]
    pub niniyo: bool,

    #[serde(default = "default_true")]
    pub k20: bool,

    #[serde(default)]
    pub hianime: bool,
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn omitted_uncertified_sources_remain_disabled() {
        let config: Config = toml::from_str(
            r#"
                [sources]
                moviebox = true
            "#,
        )
        .expect("config should parse");

        assert!(!config.sources.allanime);
        assert!(config.sources.anidb);
        assert!(config.sources.animegg);
        assert!(!config.sources.animevietsub);
        assert!(!config.sources.animetvn);
        assert!(!config.sources.hianime);
        assert!(!config.sources.anizone);
        assert!(!config.sources.invidious);
    }

    #[test]
    fn defaults_enable_only_browser_certified_sources() {
        let sources = super::SourcesConfig::default();

        assert!(sources.kkphim);
        assert!(sources.ophim);
        assert!(sources.niniyo);
        assert!(sources.k20);
        assert!(!sources.anizone);
        assert!(sources.anidb);
        assert!(!sources.moviebox);
        assert!(sources.animegg);
        assert!(!sources.allanime);
        assert!(!sources.animevietsub);
        assert!(!sources.animetvn);
        assert!(!sources.hianime);
        assert!(!sources.invidious);
    }

    #[test]
    fn enabled_invidious_requires_an_instance_url() {
        let mut config = Config::default();
        config.sources.invidious = true;
        assert!(config.validate().is_err());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProwlarrConfig {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvidiousConfig {
    pub instance_url: String,

    #[serde(default = "default_true")]
    pub local_proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub primary_color: String,
    pub secondary_color: String,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> ThemeConfig {
    ThemeConfig {
        primary_color: "#E50914".to_string(), // Netflix Red
        secondary_color: "#ffffff".to_string(),
    }
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            invidious: false,
            anidb: true,
            anizone: false,
            // AllAnime remains disabled: its current source API is
            // challenge-gated and cannot pass playback certification.
            allanime: false,
            // AnimeGG passes live playback certification and is enabled.
            animegg: true,
            // MovieBox currently returns HEVC-only DASH streams, which are not
            // browser-safe across the supported Chrome and Firefox clients.
            moviebox: false,
            kkphim: true,
            ophim: true,
            // AnimeVietSub remains an opt-in OPhim-backed compatibility
            // adapter until a distinct web-safe integration is certified.
            animevietsub: false,
            animetvn: false,
            niniyo: true,
            k20: true,
            hianime: false,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        default_theme()
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path()?;
        Self::migrate_legacy_config(&config_path)?;
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(config_path).context("Failed to read config file")?;
        toml::from_str(&content).context("Failed to parse config file")
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        let parent = config_path.parent().context("Invalid config path")?;
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(config_path, content).context("Failed to write config file")
    }

    pub fn get_config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "silent9669", "any-watch")
            .context("Failed to get config directory")?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    fn get_legacy_config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "silent9669", "ani-tui")
            .context("Failed to get legacy config directory")?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    fn migrate_legacy_config(config_path: &std::path::Path) -> Result<()> {
        if config_path.exists() {
            return Ok(());
        }

        let legacy_path = Self::get_legacy_config_path()?;
        if !legacy_path.exists() {
            return Ok(());
        }

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create any-watch config directory")?;
        }

        std::fs::copy(&legacy_path, config_path).with_context(|| {
            format!(
                "Failed to migrate config from {} to {}",
                legacy_path.display(),
                config_path.display()
            )
        })?;

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.sources.invidious {
            let config = self
                .invidious
                .as_ref()
                .context("Invidious is enabled but [invidious] is not configured")?;
            let url = reqwest::Url::parse(config.instance_url.trim())
                .context("Invidious instance_url is not a valid URL")?;
            anyhow::ensure!(
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
                "Invidious instance_url must be an HTTP or HTTPS URL"
            );
        }
        Ok(())
    }
}
