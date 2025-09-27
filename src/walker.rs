use crate::{error::Result, types::*};
use freedesktop_desktop_entry::{DesktopEntry as FreedesktopEntry};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub struct WalkerIntegration {
    desktop_entries: HashMap<String, DesktopEntry>,
    desktop_dirs: Vec<PathBuf>,
    enabled: bool,
}

impl WalkerIntegration {
    pub fn new(config: &crate::config::Config) -> Self {
        let desktop_dirs = config.walker.desktop_entry_dirs
            .iter()
            .map(|dir| {
                if dir.starts_with("~/") {
                    if let Some(home) = dirs::home_dir() {
                        home.join(&dir[2..])
                    } else {
                        PathBuf::from(dir)
                    }
                } else {
                    PathBuf::from(dir)
                }
            })
            .collect();

        Self {
            desktop_entries: HashMap::new(),
            desktop_dirs,
            enabled: config.walker.enabled,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        if !self.enabled {
            debug!("Walker integration disabled");
            return Ok(());
        }

        info!("Initializing Walker integration");
        self.scan_desktop_entries().await?;
        info!("Found {} desktop entries", self.desktop_entries.len());
        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub async fn scan_desktop_entries(&mut self) -> Result<()> {
        self.desktop_entries.clear();

        let dirs = self.desktop_dirs.clone();
        for dir in &dirs {
            if !dir.exists() {
                debug!("Desktop entry directory not found: {:?}", dir);
                continue;
            }

            debug!("Scanning desktop entries in: {:?}", dir);
            self.scan_directory(dir).await?;
        }

        Ok(())
    }

    async fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                if let Ok(desktop_entry) = self.parse_desktop_entry(&path).await {
                    let key = self.get_entry_key(&desktop_entry);
                    self.desktop_entries.insert(key, desktop_entry);
                }
            }
        }

        Ok(())
    }

    async fn parse_desktop_entry(&self, path: &Path) -> Result<DesktopEntry> {
        let content = tokio::fs::read_to_string(path).await?;

        match FreedesktopEntry::decode(path, &content) {
            Ok(entry) => {
                let name = entry.name(None).unwrap_or(std::borrow::Cow::Borrowed("Unknown")).to_string();
                let exec = entry.exec().unwrap_or("").to_string();
                let icon = entry.icon().map(String::from);
                let no_display = entry.no_display();

                let categories = entry.categories()
                    .map(|cats| cats.split(';').map(String::from).collect())
                    .unwrap_or_default();

                Ok(DesktopEntry {
                    name,
                    exec: self.clean_exec_command(&exec),
                    icon,
                    categories,
                    no_display,
                    path: path.to_string_lossy().to_string(),
                })
            }
            Err(e) => {
                debug!("Failed to parse desktop entry {:?}: {}", path, e);
                Err(crate::error::HyprsechyError::DesktopEntry(
                    format!("Failed to parse desktop entry: {}", e)
                ))
            }
        }
    }

    fn clean_exec_command(&self, exec: &str) -> String {
        // Remove field codes like %f, %F, %u, %U
        let cleaned = exec
            .replace("%f", "")
            .replace("%F", "")
            .replace("%u", "")
            .replace("%U", "")
            .replace("%i", "")
            .replace("%c", "")
            .replace("%k", "");

        // Remove extra spaces
        cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn get_entry_key(&self, entry: &DesktopEntry) -> String {
        // Use the executable name as the key
        entry.exec
            .split_whitespace()
            .next()
            .and_then(|cmd| Path::new(cmd).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or(&entry.name)
            .to_lowercase()
    }

    pub fn find_desktop_entry_by_class(&self, class: &str) -> Option<&DesktopEntry> {
        // Skip hidden entries completely
        let visible_entries: Vec<_> = self.desktop_entries.values()
            .filter(|entry| !entry.no_display && !entry.exec.is_empty())
            .collect();

        // Try exact class name match first
        if let Some(entry) = visible_entries.iter().find(|entry| {
            entry.name.to_lowercase() == class.to_lowercase()
        }) {
            return Some(entry);
        }

        // Try StartupWMClass matching (best for apps like Spotify)
        if let Some(entry) = visible_entries.iter().find(|entry| {
            // For Spotify, class is "Spotify" and we want to match with spotify.desktop
            entry.name.to_lowercase().contains(&class.to_lowercase()) ||
            class.to_lowercase().contains(&entry.name.to_lowercase())
        }) {
            return Some(entry);
        }

        // Try executable name matching
        if let Some(entry) = visible_entries.iter().find(|entry| {
            let exec_name = entry.exec
                .split_whitespace()
                .next()
                .and_then(|cmd| Path::new(cmd).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("");

            !exec_name.is_empty() && (
                class.to_lowercase().contains(&exec_name.to_lowercase()) ||
                exec_name.to_lowercase().contains(&class.to_lowercase())
            )
        }) {
            return Some(entry);
        }

        // Fallback to key-based lookup for exact matches
        self.desktop_entries.get(&class.to_lowercase())
            .filter(|entry| !entry.no_display && !entry.exec.is_empty())
    }

    pub fn find_desktop_entry_by_command(&self, command: &str) -> Option<&DesktopEntry> {
        self.desktop_entries.values().find(|entry| {
            entry.exec.contains(command) || command.contains(&entry.exec)
        })
    }

    pub fn get_launch_command(&self, class: &str, fallback_command: Option<&str>) -> Vec<String> {
        if let Some(entry) = self.find_desktop_entry_by_class(class) {
            debug!("Found desktop entry for class '{}': {}", class, entry.exec);
            return entry.exec.split_whitespace().map(String::from).collect();
        }

        if let Some(cmd) = fallback_command {
            debug!("Using fallback command for class '{}': {}", class, cmd);
            return cmd.split_whitespace().map(String::from).collect();
        }

        // Last resort: try to use the class name as a command
        warn!("No desktop entry or fallback command found for class '{}'", class);
        vec![class.to_lowercase()]
    }

    pub fn get_all_applications(&self) -> Vec<&DesktopEntry> {
        self.desktop_entries.values()
            .filter(|entry| !entry.no_display)
            .collect()
    }

    pub fn get_frequent_applications(&self, limit: usize) -> Vec<&DesktopEntry> {
        // For now, just return the first N applications
        // In a real implementation, this would track usage frequency
        self.get_all_applications()
            .into_iter()
            .take(limit)
            .collect()
    }

    pub fn suggest_command_for_class(&self, class: &str) -> Option<String> {
        self.find_desktop_entry_by_class(class)
            .map(|entry| entry.exec.clone())
    }

    pub fn get_categories(&self) -> Vec<String> {
        let mut categories = Vec::new();
        for entry in self.desktop_entries.values() {
            for category in &entry.categories {
                if !categories.contains(category) && !category.is_empty() {
                    categories.push(category.clone());
                }
            }
        }
        categories.sort();
        categories
    }

    pub fn filter_by_category(&self, category: &str) -> Vec<&DesktopEntry> {
        self.desktop_entries.values()
            .filter(|entry| {
                entry.categories.iter().any(|cat|
                    cat.to_lowercase() == category.to_lowercase()
                )
            })
            .collect()
    }

    pub async fn refresh(&mut self) -> Result<()> {
        info!("Refreshing Walker integration");
        self.scan_desktop_entries().await
    }

    pub fn get_statistics(&self) -> WalkerStats {
        let total_entries = self.desktop_entries.len();
        let visible_entries = self.desktop_entries.values()
            .filter(|entry| !entry.no_display)
            .count();
        let categories = self.get_categories().len();

        WalkerStats {
            total_entries,
            visible_entries,
            categories,
            enabled: self.enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalkerStats {
    pub total_entries: usize,
    pub visible_entries: usize,
    pub categories: usize,
    pub enabled: bool,
}

impl Default for WalkerIntegration {
    fn default() -> Self {
        Self {
            desktop_entries: HashMap::new(),
            desktop_dirs: vec![
                PathBuf::from("/usr/share/applications"),
                PathBuf::from("/usr/local/share/applications"),
            ],
            enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_walker_initialization() {
        let config = Config::default();
        let mut walker = WalkerIntegration::new(&config);
        let result = walker.initialize().await;
        // Should not fail even if directories don't exist
        assert!(result.is_ok());
    }

    #[test]
    fn test_exec_cleaning() {
        let config = Config::default();
        let walker = WalkerIntegration::new(&config);

        let cleaned = walker.clean_exec_command("firefox %u");
        assert_eq!(cleaned, "firefox");

        let cleaned = walker.clean_exec_command("code %f %F");
        assert_eq!(cleaned, "code");
    }
}