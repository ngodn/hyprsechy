use crate::{error::Result, types::*};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub uwsm: UwsmConfig,
    #[serde(default)]
    pub walker: WalkerConfig,
    #[serde(default)]
    pub restoration: RestorationConfig,
    #[serde(default)]
    pub filters: FilterConfig,
    #[serde(default)]
    pub commands: HashMap<String, String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            debug!("Config file not found, creating default");
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        debug!("Loading config from: {:?}", config_path);
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::HyprsechyError::Config(e.to_string()))?;

        std::fs::write(&config_path, content)?;
        debug!("Config saved to: {:?}", config_path);
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| crate::error::HyprsechyError::Config(
                "Could not determine config directory".to_string()
            ))?;
        Ok(config_dir.join("hyprsechy").join("config.toml"))
    }

    pub fn data_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .ok_or_else(|| crate::error::HyprsechyError::Config(
                "Could not determine data directory".to_string()
            ))?;
        Ok(data_dir.join("hyprsechy"))
    }

    pub fn session_file_path(&self) -> Result<PathBuf> {
        let data_dir = Self::data_dir()?;
        Ok(data_dir.join("session.json"))
    }

    pub fn backup_session_path(&self, backup_index: u32) -> Result<PathBuf> {
        let data_dir = Self::data_dir()?;
        Ok(data_dir.join(format!("session.backup.{}.json", backup_index)))
    }

    pub fn should_ignore_class(&self, class: &str) -> bool {
        self.filters.ignore_classes.iter().any(|ignored| {
            class.to_lowercase().contains(&ignored.to_lowercase())
        })
    }

    pub fn should_ignore_workspace(&self, workspace_id: i32) -> bool {
        self.filters.ignore_workspaces.contains(&workspace_id)
    }

    pub fn get_custom_command(&self, class: &str) -> Option<&String> {
        self.commands.get(class)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            uwsm: UwsmConfig::default(),
            walker: WalkerConfig::default(),
            restoration: RestorationConfig::default(),
            filters: FilterConfig::default(),
            commands: HashMap::new(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            save_interval: 60,
            backup_count: 5,
            auto_restore: true,
            verbose_logging: false,
        }
    }
}

impl Default for UwsmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            track_systemd_units: true,
            preserve_unit_names: false,
            launch_via_uwsm: true,
        }
    }
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            desktop_entry_dirs: vec![
                "/usr/share/applications".to_string(),
                "~/.local/share/applications".to_string(),
            ],
            sync_frequent_apps: true,
        }
    }
}

impl Default for RestorationConfig {
    fn default() -> Self {
        Self {
            wait_for_systemd_ready: true,
            systemd_timeout_ms: 5000,
            prefer_desktop_entries: true,
        }
    }
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            ignore_classes: vec![
                "steam".to_string(),
                "discord".to_string(),
                "hyprsechy".to_string(),
            ],
            ignore_workspaces: vec![],
        }
    }
}

pub fn create_default_config() -> Result<()> {
    let config = Config::default();
    config.save()?;
    println!("Created default config at: {:?}", Config::config_path()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let _deserialized: Config = toml::from_str(&toml_str).unwrap();
    }

    #[test]
    fn test_filter_class() {
        let config = Config::default();
        assert!(config.should_ignore_class("Steam"));
        assert!(config.should_ignore_class("STEAM"));
        assert!(!config.should_ignore_class("Firefox"));
    }
}