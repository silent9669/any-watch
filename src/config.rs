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
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default)]
    pub allanime: bool,

    #[serde(default)]
    pub animegg: bool,

    #[serde(default = "default_true")]
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

    #[serde(default)]
    pub hianime: bool,
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn omitted_opt_in_sources_remain_disabled() {
        let config: Config = toml::from_str(
            r#"
                [sources]
                moviebox = true
            "#,
        )
        .expect("config should parse");

        assert!(!config.sources.allanime);
        assert!(!config.sources.animegg);
        assert!(!config.sources.animevietsub);
        assert!(!config.sources.animetvn);
        assert!(!config.sources.hianime);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProwlarrConfig {
    pub url: String,
    pub api_key: String,
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
            // AllAnime remains available for manual verification, but its
            // current episode sources do not resolve to a playable stream.
            allanime: false,
            // AnimeGG is opt-in while its public origin is timing out. Keeping
            // it enabled would advertise a provider that cannot pass playback
            // certification.
            animegg: false,
            moviebox: true,
            kkphim: true,
            ophim: true,
            animevietsub: false,
            animetvn: false,
            niniyo: true,
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
        let proj_dirs = ProjectDirs::from("com", "silent9669", "ani-desk")
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
                .context("Failed to create ani-desk config directory")?;
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
        // Config validation - all providers now work without external dependencies
        Ok(())
    }
}
