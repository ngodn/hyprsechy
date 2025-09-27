use crate::{
    config::Config,
    error::Result,
    hyprland::HyprlandIpc,
    types::*,
    uwsm::UwsmIntegration,
    walker::WalkerIntegration,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct SessionManager {
    config: Config,
    hyprland: Arc<HyprlandIpc>,
    uwsm: UwsmIntegration,
    walker: Arc<RwLock<WalkerIntegration>>,
    current_session: Arc<RwLock<Option<SessionState>>>,
}

impl SessionManager {
    pub async fn new(config: Config) -> Result<Self> {
        let hyprland = Arc::new(HyprlandIpc::new()?);
        let uwsm = UwsmIntegration::new().await;
        let mut walker = WalkerIntegration::new(&config);

        if walker.is_enabled() {
            walker.initialize().await?;
        }

        Ok(Self {
            config,
            hyprland,
            uwsm,
            walker: Arc::new(RwLock::new(walker)),
            current_session: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn capture_current_session(&self) -> Result<SessionState> {
        info!("Capturing current session state");

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get Hyprland state
        let clients = self.hyprland.get_clients().await?;
        let workspaces = self.hyprland.get_workspaces().await?;
        let monitors = self.hyprland.get_monitors().await?;
        let active_workspace = self.hyprland.get_active_workspace().await?;

        // Convert monitors to our format
        let monitor_states: Vec<MonitorState> = monitors.into_iter().map(|m| MonitorState {
            name: m.name,
            width: m.width as u32,
            height: m.height as u32,
            x: m.x,
            y: m.y,
            scale: m.scale,
            active_workspace: m.active_workspace.id,
        }).collect();

        // Get UWSM apps and correlate with systemd
        let uwsm_apps = self.collect_uwsm_apps(&clients).await?;
        let systemd_units = if self.uwsm.is_available() {
            self.uwsm.get_running_apps().await?
        } else {
            Vec::new()
        };

        // Convert workspaces and include window information
        let mut workspace_states: Vec<WorkspaceState> = Vec::new();
        for workspace in workspaces {
            let workspace_windows: Vec<UwsmAppState> = uwsm_apps.iter()
                .filter(|app| app.workspace_id == workspace.id)
                .cloned()
                .collect();

            workspace_states.push(WorkspaceState {
                id: workspace.id,
                name: workspace.name,
                monitor: workspace.monitor,
                windows: workspace_windows,
            });
        }

        let session = SessionState {
            timestamp,
            workspaces: workspace_states,
            monitors: monitor_states,
            active_workspace,
            uwsm_apps,
            systemd_units,
        };

        // Update current session
        *self.current_session.write().await = Some(session.clone());

        info!(
            "Captured session with {} workspaces, {} windows, {} monitors",
            session.workspaces.len(),
            session.uwsm_apps.len(),
            session.monitors.len()
        );

        Ok(session)
    }

    async fn collect_uwsm_apps(&self, clients: &[HyprlandClient]) -> Result<Vec<UwsmAppState>> {
        let mut apps = Vec::new();

        // Get systemd correlation if UWSM is available
        let systemd_correlation = if self.uwsm.is_available() {
            self.uwsm.correlate_with_hyprland(clients).await?
        } else {
            HashMap::new()
        };

        for client in clients {
            // Skip if class should be ignored
            if self.config.should_ignore_class(&client.class) {
                debug!("Ignoring window with class: {}", client.class);
                continue;
            }

            // Skip if workspace should be ignored
            if self.config.should_ignore_workspace(client.workspace.id) {
                debug!("Ignoring window on workspace: {}", client.workspace.id);
                continue;
            }

            let systemd_unit = systemd_correlation.get(&client.address).map(|unit| unit.unit_name.clone());
            let uwsm_launched = systemd_unit.is_some();

            // Get command for the application
            let command = self.get_app_command(client, &systemd_correlation).await;

            let app = UwsmAppState {
                class: client.class.clone(),
                title: client.title.clone(),
                desktop_entry: None, // Will be filled by Walker if available
                command,
                workspace_id: client.workspace.id,
                position: client.at,
                size: client.size,
                monitor: client.monitor.to_string(),
                floating: client.floating,
                fullscreen: client.fullscreen,
                systemd_unit,
                uwsm_launched,
                pid: Some(client.pid),
            };

            apps.push(app);
        }

        // Enhance apps with Walker desktop entry information
        if self.walker.read().await.is_enabled() {
            for app in &mut apps {
                if let Some(entry) = self.walker.read().await.find_desktop_entry_by_class(&app.class) {
                    app.desktop_entry = Some(entry.path.clone());
                }
            }
        }

        Ok(apps)
    }

    async fn get_app_command(&self, client: &HyprlandClient, systemd_correlation: &HashMap<String, SystemdServiceInfo>) -> Vec<String> {
        // Try custom command mapping first
        if let Some(custom_cmd) = self.config.get_custom_command(&client.class) {
            return custom_cmd.split_whitespace().map(String::from).collect();
        }

        // Try systemd unit command
        if let Some(unit) = systemd_correlation.get(&client.address) {
            if let Ok(cmd) = self.uwsm.get_app_command(&unit.unit_name).await {
                return cmd.split_whitespace().map(String::from).collect();
            }
        }

        // Try to get command from PID if available
        if let Ok(pid_cmd) = self.get_command_from_pid(client.pid).await {
            if !pid_cmd.is_empty() && pid_cmd != client.class.to_lowercase() {
                return self.clean_pid_command(&pid_cmd);
            }
        }

        // Try Walker integration
        if self.walker.read().await.is_enabled() {
            let walker_cmd = self.walker.read().await.get_launch_command(&client.class, None);
            if !walker_cmd.is_empty() && walker_cmd[0] != client.class.to_lowercase() {
                return walker_cmd;
            }
        }

        // Fallback: try common executable names for known classes
        match client.class.as_str() {
            "Spotify" => vec!["spotify".to_string()],
            "org.gnome.Nautilus" => vec!["nautilus".to_string()],
            "chromium" => vec!["chromium".to_string()],
            "firefox" => vec!["firefox".to_string()],
            _ => vec![client.class.to_lowercase()]
        }
    }

    async fn get_command_from_pid(&self, pid: u32) -> Result<String> {
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        match tokio::fs::read_to_string(cmdline_path).await {
            Ok(cmdline) => {
                let command = cmdline
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(command)
            }
            Err(_) => Ok(String::new())
        }
    }

    fn clean_pid_command(&self, cmd: &str) -> Vec<String> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return vec![];
        }

        // For complex commands with env vars, pipes, etc., try to extract the main executable
        let main_executable = if parts[0] == "env" && parts.len() > 1 {
            // Handle "env VAR=value executable args"
            parts.iter()
                .skip(1)
                .find(|part| !part.contains('='))
                .unwrap_or(&parts[0])
        } else {
            parts[0]
        };

        // Return just the executable name for complex commands that might not work when restored
        if cmd.contains('|') || cmd.contains("sudo") || cmd.contains("tee") {
            vec![main_executable.to_string()]
        } else {
            parts.iter().map(|s| s.to_string()).collect()
        }
    }

    pub async fn save_session(&self, session: &SessionState) -> Result<()> {
        let session_path = self.config.session_file_path()?;

        // Create backup if session file exists
        if session_path.exists() {
            self.create_backup(&session_path).await?;
        }

        // Ensure directory exists
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Serialize and save
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(&session_path, json).await?;

        info!("Session saved to: {:?}", session_path);
        Ok(())
    }

    async fn create_backup(&self, session_path: &PathBuf) -> Result<()> {
        // Rotate existing backups
        for i in (1..self.config.general.backup_count).rev() {
            let old_backup = self.config.backup_session_path(i)?;
            let new_backup = self.config.backup_session_path(i + 1)?;

            if old_backup.exists() {
                tokio::fs::rename(&old_backup, &new_backup).await?;
            }
        }

        // Create new backup
        let backup_path = self.config.backup_session_path(1)?;
        tokio::fs::copy(session_path, &backup_path).await?;

        debug!("Created session backup: {:?}", backup_path);
        Ok(())
    }

    pub async fn load_session(&self) -> Result<Option<SessionState>> {
        let session_path = self.config.session_file_path()?;

        if !session_path.exists() {
            info!("No saved session found");
            return Ok(None);
        }

        info!("Loading session from: {:?}", session_path);

        let content = tokio::fs::read_to_string(&session_path).await?;
        let session: SessionState = serde_json::from_str(&content)?;

        info!(
            "Loaded session with {} workspaces, {} windows",
            session.workspaces.len(),
            session.uwsm_apps.len()
        );

        Ok(Some(session))
    }

    pub async fn restore_session(&self, session: &SessionState) -> Result<()> {
        info!("Starting session restoration");

        // First, restore workspaces and monitor configuration
        self.restore_workspaces(session).await?;

        // Then restore applications
        let restored_count = self.restore_applications(session).await?;

        // Finally, set active workspace
        self.hyprland.dispatch(&format!("workspace {}", session.active_workspace)).await?;

        info!("Session restoration completed. Restored {} applications", restored_count);
        Ok(())
    }

    async fn restore_workspaces(&self, session: &SessionState) -> Result<()> {
        debug!("Restoring workspaces - ensuring all workspaces exist");

        // First, collect all unique workspace IDs that have windows
        let mut workspace_ids: Vec<i32> = session.workspaces
            .iter()
            .filter(|ws| !ws.windows.is_empty())
            .map(|ws| ws.id)
            .collect();
        workspace_ids.sort();
        workspace_ids.dedup();

        debug!("Need to ensure workspaces exist: {:?}", workspace_ids);

        // Visit each workspace to ensure it exists in Hyprland's database
        for workspace_id in &workspace_ids {
            debug!("Ensuring workspace {} exists", workspace_id);
            let workspace_cmd = format!("workspace {}", workspace_id);
            if let Err(e) = self.hyprland.dispatch(&workspace_cmd).await {
                warn!("Failed to create workspace {}: {}", workspace_id, e);
            }

            // Small delay to let Hyprland process the workspace creation
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        debug!("All workspaces should now exist in Hyprland's database");
        Ok(())
    }

    async fn restore_applications(&self, session: &SessionState) -> Result<usize> {
        debug!("Restoring applications");

        let mut restored_count = 0;

        for app in &session.uwsm_apps {
            match self.restore_single_app(app).await {
                Ok(_) => {
                    restored_count += 1;
                    debug!("Restored app: {} on workspace {}", app.class, app.workspace_id);
                }
                Err(e) => {
                    warn!("Failed to restore app {}: {}", app.class, e);
                }
            }

            // Small delay between app launches to avoid overwhelming the system
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        Ok(restored_count)
    }

    async fn restore_single_app(&self, app: &UwsmAppState) -> Result<()> {
        // Launch the application
        if app.uwsm_launched && self.uwsm.is_available() {
            self.uwsm.launch_app(&app.command).await?;
        } else {
            // Launch via hyprctl
            let command_str = app.command.join(" ");
            crate::hyprland::exec_command(&command_str).await?;
        }

        // Wait for the application to start
        if let Some(unit_name) = &app.systemd_unit {
            if self.config.restoration.wait_for_systemd_ready {
                self.uwsm.wait_for_app_ready(unit_name, self.config.restoration.systemd_timeout_ms).await?;
            }
        } else {
            // For non-systemd apps, wait a bit
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }

        // Position and configure the window
        self.configure_restored_window(app).await?;

        Ok(())
    }

    async fn configure_restored_window(&self, app: &UwsmAppState) -> Result<()> {
        // Wait a bit for the window to fully appear
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Find the window that was just launched
        let target_client = if let Ok(clients) = self.hyprland.get_clients().await {
            clients.iter()
                .filter(|c| c.class == app.class)
                .max_by_key(|c| c.pid) // Get the newest window (highest PID)
                .cloned()
        } else {
            None
        };

        // Move to correct workspace
        if let Some(client) = &target_client {
            debug!("Moving window {} (class: {}) to workspace {}", client.address, client.class, app.workspace_id);

            // Use address-based command with proper address format
            let address = client.address.clone();
            let workspace_cmd = format!("movetoworkspacesilent {},address:{}", app.workspace_id, address);

            if let Err(e) = self.hyprland.dispatch(&workspace_cmd).await {
                warn!("Address-based move failed: {}, trying fallback", e);
                // Fallback: focus the window first, then move it
                let focus_cmd = format!("focuswindow address:{}", address);
                let _ = self.hyprland.dispatch(&focus_cmd).await;
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                let fallback_cmd = format!("movetoworkspacesilent {}", app.workspace_id);
                let _ = self.hyprland.dispatch(&fallback_cmd).await;
            }
        } else {
            warn!("Could not find window for class: {}", app.class);
            // Last resort fallback
            let fallback_cmd = format!("movetoworkspacesilent {}", app.workspace_id);
            let _ = self.hyprland.dispatch(&fallback_cmd).await;
        }

        // Small delay after moving
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Set floating state
        if app.floating {
            if let Some(client) = &target_client {
                let float_cmd = format!("togglefloating address:{}", client.address);
                let _ = self.hyprland.dispatch(&float_cmd).await;
            }
        }

        // Set fullscreen state
        if app.fullscreen {
            if let Some(client) = &target_client {
                let fullscreen_cmd = format!("fullscreen address:{}", client.address);
                let _ = self.hyprland.dispatch(&fullscreen_cmd).await;
            }
        }

        // Position and resize
        if app.position != (0, 0) && app.size != (0, 0) {
            if let Some(client) = &target_client {
                // Resize the specific window
                let resize_cmd = format!("resizewindowpixel exact {},{},address:{}",
                    app.size.0, app.size.1, client.address);
                let _ = self.hyprland.dispatch(&resize_cmd).await;

                // Move if floating
                if app.floating {
                    let move_cmd = format!("movewindowpixel exact {},{},address:{}",
                        app.position.0, app.position.1, client.address);
                    let _ = self.hyprland.dispatch(&move_cmd).await;
                }
            }
        }

        Ok(())
    }

    pub async fn auto_save_loop(&self) -> Result<()> {
        let interval = std::time::Duration::from_secs(self.config.general.save_interval);

        info!("Starting auto-save loop with interval: {}s", self.config.general.save_interval);

        loop {
            tokio::time::sleep(interval).await;

            match self.capture_current_session().await {
                Ok(session) => {
                    if let Err(e) = self.save_session(&session).await {
                        error!("Auto-save failed: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to capture session for auto-save: {}", e);
                }
            }
        }
    }

    pub async fn get_current_session(&self) -> Option<SessionState> {
        self.current_session.read().await.clone()
    }

    pub async fn refresh_walker(&self) -> Result<()> {
        if self.walker.read().await.is_enabled() {
            self.walker.write().await.refresh().await?;
            info!("Walker integration refreshed");
        }
        Ok(())
    }

    pub async fn get_session_info(&self) -> SessionInfo {
        let current = self.current_session.read().await.clone();
        let session_path = self.config.session_file_path().unwrap_or_default();

        SessionInfo {
            has_saved_session: session_path.exists(),
            current_session_timestamp: current.as_ref().map(|s| s.timestamp),
            current_workspaces: current.as_ref().map(|s| s.workspaces.len()).unwrap_or(0),
            current_windows: current.as_ref().map(|s| s.uwsm_apps.len()).unwrap_or(0),
            uwsm_available: self.uwsm.is_available(),
            walker_enabled: self.walker.read().await.is_enabled(),
        }
    }

    pub async fn start_event_listener(&self) -> Result<()> {
        info!("Starting Hyprland event listener for real-time session tracking");

        let (tx, mut rx) = mpsc::unbounded_channel::<()>();
        let session_manager = self.clone();

        // Spawn task to handle save requests
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                // Debounce saves - wait a bit for multiple events
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                // Drain any additional events that came in during the wait
                while rx.try_recv().is_ok() {}

                match session_manager.capture_current_session().await {
                    Ok(session) => {
                        if let Err(e) = session_manager.save_session(&session).await {
                            error!("Event-triggered auto-save failed: {}", e);
                        } else {
                            debug!("Event-triggered auto-save completed");
                        }
                    }
                    Err(e) => {
                        error!("Failed to capture session for event-triggered save: {}", e);
                    }
                }
            }
        });

        let hyprland = self.hyprland.clone();

        // Listen to Hyprland events and trigger saves on relevant changes
        hyprland.listen_events(move |event| {
            use crate::types::HyprlandEvent;

            match &event {
                HyprlandEvent::WindowOpen { class, .. } => {
                    info!("Window opened: {} - triggering auto-save", class);
                }
                HyprlandEvent::WindowClose { .. } => {
                    info!("Window closed - triggering auto-save");
                }
                HyprlandEvent::WindowMove { window_address: _, workspace } => {
                    info!("Window moved to workspace {} - triggering auto-save", workspace);
                }
                HyprlandEvent::WorkspaceCreate { workspace } => {
                    info!("Workspace created: {} - triggering auto-save", workspace);
                }
                HyprlandEvent::WorkspaceDestroy { workspace } => {
                    info!("Workspace destroyed: {} - triggering auto-save", workspace);
                }
                HyprlandEvent::WorkspaceFocus { workspace } => {
                    debug!("Workspace focused: {} - light auto-save", workspace);
                }
                _ => {
                    debug!("Other event received: {:?}", event);
                }
            }

            // Trigger save for important events
            let should_save = matches!(event,
                HyprlandEvent::WindowOpen { .. } |
                HyprlandEvent::WindowClose { .. } |
                HyprlandEvent::WindowMove { .. } |
                HyprlandEvent::WorkspaceCreate { .. } |
                HyprlandEvent::WorkspaceDestroy { .. }
            );

            if should_save {
                let _ = tx.send(());
            }
        }).await?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub has_saved_session: bool,
    pub current_session_timestamp: Option<u64>,
    pub current_workspaces: usize,
    pub current_windows: usize,
    pub uwsm_available: bool,
    pub walker_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_serialization() {
        let session = SessionState {
            timestamp: 1234567890,
            workspaces: vec![],
            monitors: vec![],
            active_workspace: 1,
            uwsm_apps: vec![],
            systemd_units: vec![],
        };

        let json = serde_json::to_string(&session).unwrap();
        let _deserialized: SessionState = serde_json::from_str(&json).unwrap();
    }
}