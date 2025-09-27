use crate::{error::Result, types::*};
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct SystemdManager {
    #[allow(dead_code)]
    user_mode: bool,
}

impl SystemdManager {
    pub fn new() -> Self {
        Self { user_mode: true }
    }

    pub async fn discover_uwsm_apps(&self) -> Result<Vec<SystemdServiceInfo>> {
        debug!("Discovering UWSM apps via systemd");

        let output = Command::new("systemctl")
            .args(&[
                "--user",
                "list-units",
                "--type=scope",
                "app-graphical.slice",
                "--output=json",
                "--no-pager",
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to list systemd units: {}", stderr);
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let units: Vec<Value> = serde_json::from_str(&stdout).unwrap_or_default();

        let mut services = Vec::new();
        for unit in units {
            if let Ok(service) = self.parse_systemd_unit(unit).await {
                services.push(service);
            }
        }

        info!("Discovered {} UWSM apps via systemd", services.len());
        Ok(services)
    }

    pub async fn get_unit_property(&self, unit_name: &str, property: &str) -> Result<String> {
        let output = Command::new("systemctl")
            .args(&[
                "--user",
                "show",
                unit_name,
                "--property",
                property,
                "--value",
                "--no-pager",
            ])
            .output()
            .await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(crate::error::HyprsechyError::Systemd(format!(
                "Failed to get property {} for unit {}",
                property, unit_name
            )))
        }
    }

    pub async fn get_unit_pid(&self, unit_name: &str) -> Result<u32> {
        let pid_str = self.get_unit_property(unit_name, "MainPID").await?;
        pid_str.parse().map_err(|_| {
            crate::error::HyprsechyError::Systemd(format!(
                "Invalid PID for unit {}: {}",
                unit_name, pid_str
            ))
        })
    }

    pub async fn get_unit_command(&self, unit_name: &str) -> Result<String> {
        // Try to get the ExecStart property
        if let Ok(exec_start) = self.get_unit_property(unit_name, "ExecStart").await {
            if !exec_start.is_empty() {
                return Ok(exec_start);
            }
        }

        // For scope units, try to get the command from cgroup
        let pid = self.get_unit_pid(unit_name).await?;
        self.get_command_from_pid(pid).await
    }

    pub async fn get_command_from_pid(&self, pid: u32) -> Result<String> {
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        match tokio::fs::read_to_string(cmdline_path).await {
            Ok(cmdline) => {
                // Convert null-separated arguments to space-separated
                let command = cmdline
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(command)
            }
            Err(e) => Err(crate::error::HyprsechyError::Systemd(format!(
                "Failed to read cmdline for PID {}: {}",
                pid, e
            ))),
        }
    }

    pub async fn wait_for_unit_ready(&self, unit_name: &str, timeout_ms: u64) -> Result<bool> {
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start_time.elapsed() < timeout {
            match self.get_unit_status(unit_name).await {
                Ok(ServiceStatus::Active) => return Ok(true),
                Ok(ServiceStatus::Failed) => return Ok(false),
                _ => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        warn!("Timeout waiting for unit {} to be ready", unit_name);
        Ok(false)
    }

    pub async fn get_unit_status(&self, unit_name: &str) -> Result<ServiceStatus> {
        let output = Command::new("systemctl")
            .args(&["--user", "is-active", unit_name, "--no-pager"])
            .output()
            .await?;

        let status_output = String::from_utf8_lossy(&output.stdout);
        let status_str = status_output.trim();

        match status_str {
            "active" => Ok(ServiceStatus::Active),
            "inactive" => Ok(ServiceStatus::Inactive),
            "failed" => Ok(ServiceStatus::Failed),
            _ => Ok(ServiceStatus::Unknown),
        }
    }

    pub async fn start_unit(&self, unit_name: &str) -> Result<()> {
        debug!("Starting systemd unit: {}", unit_name);

        let output = Command::new("systemctl")
            .args(&["--user", "start", unit_name, "--no-pager"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::HyprsechyError::Systemd(format!(
                "Failed to start unit {}: {}",
                unit_name, stderr
            )));
        }

        Ok(())
    }

    pub async fn stop_unit(&self, unit_name: &str) -> Result<()> {
        debug!("Stopping systemd unit: {}", unit_name);

        let output = Command::new("systemctl")
            .args(&["--user", "stop", unit_name, "--no-pager"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to stop unit {}: {}", unit_name, stderr);
        }

        Ok(())
    }

    pub async fn get_graphical_session_units(&self) -> Result<Vec<String>> {
        let output = Command::new("systemctl")
            .args(&[
                "--user",
                "list-units",
                "--type=scope,service",
                "app-graphical.slice",
                "--output=json",
                "--no-pager",
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let units: Vec<Value> = serde_json::from_str(&stdout).unwrap_or_default();

        let unit_names: Vec<String> = units
            .into_iter()
            .filter_map(|unit| unit["unit"].as_str().map(String::from))
            .filter(|name| name.starts_with("app-") && !name.contains("hyprsechy"))
            .collect();

        Ok(unit_names)
    }

    pub async fn correlate_with_hyprland_clients(
        &self,
        clients: &[crate::types::HyprlandClient],
    ) -> Result<HashMap<String, SystemdServiceInfo>> {
        let systemd_units = self.discover_uwsm_apps().await?;
        let mut correlation = HashMap::new();

        for unit in systemd_units {
            // Try to match by PID
            if let Some(client) = clients.iter().find(|c| c.pid == unit.pid) {
                correlation.insert(client.address.clone(), unit);
            }
        }

        debug!(
            "Correlated {} clients with systemd units",
            correlation.len()
        );
        Ok(correlation)
    }

    async fn parse_systemd_unit(&self, unit: Value) -> Result<SystemdServiceInfo> {
        let unit_name = unit["unit"]
            .as_str()
            .ok_or_else(|| crate::error::HyprsechyError::Systemd("Missing unit name".to_string()))?
            .to_string();

        // Get PID and command for the unit
        let pid = match self.get_unit_pid(&unit_name).await {
            Ok(pid) => pid,
            Err(_) => {
                // If we can't get PID, this might not be an active unit
                return Err(crate::error::HyprsechyError::Systemd(format!(
                    "Could not get PID for unit {}",
                    unit_name
                )));
            }
        };

        let command = self
            .get_unit_command(&unit_name)
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        let status = self
            .get_unit_status(&unit_name)
            .await
            .unwrap_or(ServiceStatus::Unknown);

        Ok(SystemdServiceInfo {
            unit_name,
            pid,
            command,
            status,
        })
    }
}

impl Default for SystemdManager {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn check_uwsm_available() -> bool {
    Command::new("which")
        .arg("uwsm")
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub async fn launch_via_uwsm(command: &[String]) -> Result<()> {
    if !check_uwsm_available().await {
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

    Ok(())
}
