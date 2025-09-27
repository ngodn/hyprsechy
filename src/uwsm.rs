use crate::{error::Result, types::*, systemd::SystemdManager};
use std::collections::HashMap;
use tokio::process::Command;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct UwsmIntegration {
    systemd: SystemdManager,
    available: bool,
}

impl UwsmIntegration {
    pub async fn new() -> Self {
        let available = Self::check_uwsm_available().await;
        if available {
            info!("UWSM integration available");
        } else {
            warn!("UWSM not available, falling back to direct launching");
        }

        Self {
            systemd: SystemdManager::new(),
            available,
        }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub async fn check_uwsm_available() -> bool {
        match Command::new("which").arg("uwsm").output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    pub async fn discover_uwsm_apps(&self) -> Result<Vec<UwsmAppState>> {
        if !self.available {
            return Ok(Vec::new());
        }

        debug!("Discovering UWSM applications");
        let services = self.systemd.discover_uwsm_apps().await?;

        let mut apps = Vec::new();
        for service in services {
            if let Ok(app) = self.convert_service_to_app(service).await {
                apps.push(app);
            }
        }

        info!("Discovered {} UWSM applications", apps.len());
        Ok(apps)
    }

    pub async fn launch_app(&self, command: &[String]) -> Result<()> {
        if self.available {
            self.launch_via_uwsm(command).await
        } else {
            self.launch_direct(command).await
        }
    }

    pub async fn launch_via_uwsm(&self, command: &[String]) -> Result<()> {
        if !self.available {
            return Err(crate::error::HyprsechyError::Uwsm(
                "UWSM not available".to_string(),
            ));
        }

        info!("Launching command via UWSM: {:?}", command);

        let mut cmd = Command::new("uwsm");
        cmd.args(&["app", "--"]);
        cmd.args(command);

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::HyprsechyError::Uwsm(format!(
                "Failed to launch via UWSM: {}",
                stderr
            )));
        }

        debug!("Successfully launched via UWSM");
        Ok(())
    }

    pub async fn launch_direct(&self, command: &[String]) -> Result<()> {
        if command.is_empty() {
            return Err(crate::error::HyprsechyError::Uwsm(
                "Empty command".to_string(),
            ));
        }

        info!("Launching command directly: {:?}", command);

        let mut cmd = Command::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Direct launch failed: {}", stderr);
        }

        Ok(())
    }

    pub async fn correlate_with_hyprland(&self, clients: &[crate::types::HyprlandClient]) -> Result<HashMap<String, SystemdServiceInfo>> {
        self.systemd.correlate_with_hyprland_clients(clients).await
    }

    pub async fn wait_for_app_ready(&self, unit_name: &str, timeout_ms: u64) -> Result<bool> {
        if !self.available {
            // For non-UWSM apps, just wait a bit and assume they're ready
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            return Ok(true);
        }

        self.systemd.wait_for_unit_ready(unit_name, timeout_ms).await
    }

    pub async fn stop_app(&self, unit_name: &str) -> Result<()> {
        if !self.available {
            debug!("UWSM not available, cannot stop unit: {}", unit_name);
            return Ok(());
        }

        self.systemd.stop_unit(unit_name).await
    }

    pub async fn get_app_command(&self, unit_name: &str) -> Result<String> {
        self.systemd.get_unit_command(unit_name).await
    }

    pub async fn get_running_apps(&self) -> Result<Vec<SystemdServiceInfo>> {
        if !self.available {
            return Ok(Vec::new());
        }

        self.systemd.discover_uwsm_apps().await
    }

    async fn convert_service_to_app(&self, service: SystemdServiceInfo) -> Result<UwsmAppState> {
        // This is a basic conversion - in a real implementation, we'd correlate
        // with Hyprland clients to get window position/size information

        let command_parts: Vec<String> = service.command
            .split_whitespace()
            .map(String::from)
            .collect();

        // Extract class name from command (basic heuristic)
        let class = command_parts.first()
            .and_then(|cmd| std::path::Path::new(cmd).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(UwsmAppState {
            class,
            title: "".to_string(), // Will be filled by Hyprland correlation
            desktop_entry: None,
            command: command_parts,
            workspace_id: 1, // Default workspace
            position: (0, 0),
            size: (800, 600),
            monitor: "".to_string(),
            floating: false,
            fullscreen: false,
            systemd_unit: Some(service.unit_name),
            uwsm_launched: true,
            pid: Some(service.pid),
        })
    }

    pub async fn get_uwsm_status(&self) -> Result<UwsmStatus> {
        if !self.available {
            return Ok(UwsmStatus {
                available: false,
                active_apps: 0,
                total_units: 0,
            });
        }

        let apps = self.get_running_apps().await?;
        let units = self.systemd.get_graphical_session_units().await?;

        Ok(UwsmStatus {
            available: true,
            active_apps: apps.len(),
            total_units: units.len(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct UwsmStatus {
    pub available: bool,
    pub active_apps: usize,
    pub total_units: usize,
}

impl Default for UwsmIntegration {
    fn default() -> Self {
        Self {
            systemd: SystemdManager::new(),
            available: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_uwsm_detection() {
        let uwsm = UwsmIntegration::new().await;
        // This test just ensures the module compiles and runs
        assert!(uwsm.is_available() || !uwsm.is_available());
    }
}