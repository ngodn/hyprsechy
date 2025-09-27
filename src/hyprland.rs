use crate::{error::Result, types::*};
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

pub struct HyprlandIpc {
    #[allow(dead_code)]
    socket_path: PathBuf,
    event_socket_path: PathBuf,
}

impl HyprlandIpc {
    pub fn new() -> Result<Self> {
        // Try to get HYPRLAND_INSTANCE_SIGNATURE from environment first
        if let Ok(signature) = env::var("HYPRLAND_INSTANCE_SIGNATURE") {
            // Try multiple possible locations
            let mut possible_paths = vec![
                format!("/tmp/hypr/{}/.socket.sock", signature),
                format!("/run/user/{}/hypr/{}/.socket.sock",
                    std::process::id(), signature),
            ];

            if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
                possible_paths.push(format!("{}/hypr/{}/.socket.sock", runtime_dir, signature));
            }

            for socket_path in possible_paths {
                if std::path::Path::new(&socket_path).exists() {
                    let event_socket_path = socket_path.replace(".socket.sock", ".socket2.sock");
                    debug!("Found Hyprland sockets: {} and {}", socket_path, event_socket_path);
                    return Ok(Self {
                        socket_path: PathBuf::from(socket_path),
                        event_socket_path: PathBuf::from(event_socket_path),
                    });
                }
            }
        }

        // If signature not available or sockets not found, try to find automatically
        // First try /tmp/hypr (traditional location)
        if let Ok(entries) = std::fs::read_dir("/tmp/hypr") {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let socket_path = entry.path().join(".socket.sock");
                        if socket_path.exists() {
                            let event_socket_path = entry.path().join(".socket2.sock");
                            debug!("Auto-discovered Hyprland sockets at: {:?}", entry.path());
                            return Ok(Self {
                                socket_path,
                                event_socket_path,
                            });
                        }
                    }
                }
            }
        }

        // Then try /run/user/*/hypr/ (user session location)
        if let Ok(run_user_entries) = std::fs::read_dir("/run/user") {
            for user_entry in run_user_entries.flatten() {
                if let Ok(file_type) = user_entry.file_type() {
                    if file_type.is_dir() {
                        let hypr_path = user_entry.path().join("hypr");
                        if hypr_path.exists() {
                            if let Ok(hypr_entries) = std::fs::read_dir(&hypr_path) {
                                for hypr_entry in hypr_entries.flatten() {
                                    if let Ok(hypr_file_type) = hypr_entry.file_type() {
                                        if hypr_file_type.is_dir() {
                                            let socket_path = hypr_entry.path().join(".socket.sock");
                                            if socket_path.exists() {
                                                let event_socket_path = hypr_entry.path().join(".socket2.sock");
                                                debug!("Auto-discovered Hyprland sockets in: {:?}", hypr_entry.path());
                                                return Ok(Self {
                                                    socket_path,
                                                    event_socket_path,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(crate::error::HyprsechyError::HyprlandIpc(
            "Could not find Hyprland socket. Make sure Hyprland is running.".to_string(),
        ))
    }

    pub async fn get_clients(&self) -> Result<Vec<HyprlandClient>> {
        let json = self.run_hyprctl(&["clients", "-j"]).await?;
        let clients: Vec<Value> = serde_json::from_str(&json)?;

        let mut hypr_clients = Vec::new();
        for client in clients {
            if let Ok(hypr_client) = self.parse_client(client) {
                hypr_clients.push(hypr_client);
            }
        }

        Ok(hypr_clients)
    }

    pub async fn get_workspaces(&self) -> Result<Vec<HyprlandWorkspace>> {
        let json = self.run_hyprctl(&["workspaces", "-j"]).await?;
        let workspaces: Vec<Value> = serde_json::from_str(&json)?;

        let mut hypr_workspaces = Vec::new();
        for workspace in workspaces {
            if let Ok(hypr_workspace) = self.parse_workspace(workspace) {
                hypr_workspaces.push(hypr_workspace);
            }
        }

        Ok(hypr_workspaces)
    }

    pub async fn get_monitors(&self) -> Result<Vec<HyprlandMonitor>> {
        let json = self.run_hyprctl(&["monitors", "-j"]).await?;
        let monitors: Vec<Value> = serde_json::from_str(&json)?;

        let mut hypr_monitors = Vec::new();
        for monitor in monitors {
            if let Ok(hypr_monitor) = self.parse_monitor(monitor) {
                hypr_monitors.push(hypr_monitor);
            }
        }

        Ok(hypr_monitors)
    }

    pub async fn get_active_workspace(&self) -> Result<i32> {
        let json = self.run_hyprctl(&["activeworkspace", "-j"]).await?;
        let workspace: Value = serde_json::from_str(&json)?;

        workspace["id"].as_i64().map(|id| id as i32).ok_or_else(|| {
            crate::error::HyprsechyError::HyprlandIpc(
                "Failed to get active workspace ID".to_string(),
            )
        })
    }

    pub async fn dispatch(&self, command: &str) -> Result<()> {
        let result = self.run_hyprctl(&["dispatch", command]).await?;
        if result.trim() != "ok" {
            warn!("Dispatch command '{}' returned: {}", command, result);
        }
        Ok(())
    }

    pub async fn listen_events<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(HyprlandEvent) + Send,
    {
        info!(
            "Connecting to Hyprland event socket: {:?}",
            self.event_socket_path
        );
        let stream = UnixStream::connect(&self.event_socket_path).await?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    warn!("Hyprland event socket closed");
                    break;
                }
                Ok(_) => {
                    if let Ok(event) = self.parse_event(&line) {
                        callback(event);
                    }
                }
                Err(e) => {
                    error!("Error reading from event socket: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }

        Ok(())
    }

    async fn run_hyprctl(&self, args: &[&str]) -> Result<String> {
        debug!("Running hyprctl with args: {:?}", args);

        let output = Command::new("hyprctl")
            .args(args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::HyprsechyError::HyprlandIpc(format!(
                "hyprctl command failed: {}",
                stderr
            )));
        }

        let response = String::from_utf8_lossy(&output.stdout).to_string();
        debug!("hyprctl response: {}", response);
        Ok(response)
    }

    #[allow(dead_code)]
    async fn send_command(&self, command: &str) -> Result<String> {
        debug!("Sending command to Hyprland: {}", command);

        let mut stream = UnixStream::connect(&self.socket_path).await?;
        stream.write_all(command.as_bytes()).await?;

        let mut response = String::new();
        stream.read_to_string(&mut response).await?;

        debug!("Hyprland response: {}", response);
        Ok(response)
    }

    fn parse_client(&self, client: Value) -> Result<HyprlandClient> {
        let address = client["address"]
            .as_str()
            .ok_or_else(|| {
                crate::error::HyprsechyError::HyprlandIpc("Missing address".to_string())
            })?
            .to_string();

        let class = client["class"].as_str().unwrap_or("").to_string();

        let title = client["title"].as_str().unwrap_or("").to_string();

        let workspace_id = client["workspace"]["id"].as_i64().unwrap_or(-1) as i32;

        let workspace_name = client["workspace"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let at_array = client["at"].as_array().ok_or_else(|| {
            crate::error::HyprsechyError::HyprlandIpc("Missing position".to_string())
        })?;
        let at = (
            at_array[0].as_i64().unwrap_or(0) as i32,
            at_array[1].as_i64().unwrap_or(0) as i32,
        );

        let size_array = client["size"]
            .as_array()
            .ok_or_else(|| crate::error::HyprsechyError::HyprlandIpc("Missing size".to_string()))?;
        let size = (
            size_array[0].as_i64().unwrap_or(0) as u32,
            size_array[1].as_i64().unwrap_or(0) as u32,
        );

        let monitor = client["monitor"].as_i64().unwrap_or(0) as i32;

        let floating = client["floating"].as_bool().unwrap_or(false);

        let fullscreen = client["fullscreen"].as_bool().unwrap_or(false);

        let pid = client["pid"].as_i64().unwrap_or(0) as u32;

        Ok(HyprlandClient {
            address,
            class,
            title,
            workspace: HyprlandWorkspace {
                id: workspace_id,
                name: workspace_name,
                monitor: "".to_string(), // Will be filled from workspace query
                windows: 0,              // Will be filled from workspace query
            },
            at,
            size,
            monitor,
            floating,
            fullscreen,
            pid,
        })
    }

    fn parse_workspace(&self, workspace: Value) -> Result<HyprlandWorkspace> {
        let id = workspace["id"].as_i64().unwrap_or(-1) as i32;

        let name = workspace["name"].as_str().unwrap_or("").to_string();

        let monitor = workspace["monitor"].as_str().unwrap_or("").to_string();

        let windows = workspace["windows"].as_i64().unwrap_or(0) as i32;

        Ok(HyprlandWorkspace {
            id,
            name,
            monitor,
            windows,
        })
    }

    fn parse_monitor(&self, monitor: Value) -> Result<HyprlandMonitor> {
        let id = monitor["id"].as_i64().unwrap_or(-1) as i32;

        let name = monitor["name"].as_str().unwrap_or("").to_string();

        let description = monitor["description"].as_str().unwrap_or("").to_string();

        let make = monitor["make"].as_str().unwrap_or("").to_string();

        let model = monitor["model"].as_str().unwrap_or("").to_string();

        let serial = monitor["serial"].as_str().unwrap_or("").to_string();

        let width = monitor["width"].as_i64().unwrap_or(0) as i32;

        let height = monitor["height"].as_i64().unwrap_or(0) as i32;

        let refresh_rate = monitor["refreshRate"].as_f64().unwrap_or(60.0) as f32;

        let x = monitor["x"].as_i64().unwrap_or(0) as i32;

        let y = monitor["y"].as_i64().unwrap_or(0) as i32;

        let scale = monitor["scale"].as_f64().unwrap_or(1.0) as f32;

        let transform = monitor["transform"].as_i64().unwrap_or(0) as i32;

        let focused = monitor["focused"].as_bool().unwrap_or(false);

        let dpms_status = monitor["dpmsStatus"].as_bool().unwrap_or(true);

        let vrr = monitor["vrr"].as_bool().unwrap_or(false);

        // Parse active workspace
        let active_workspace_id = monitor["activeWorkspace"]["id"].as_i64().unwrap_or(-1) as i32;

        let active_workspace_name = monitor["activeWorkspace"]["name"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let active_workspace = HyprlandWorkspace {
            id: active_workspace_id,
            name: active_workspace_name,
            monitor: name.clone(),
            windows: 0,
        };

        // Parse special workspace if it exists
        let special_workspace = if monitor["specialWorkspace"]["id"].is_null() {
            None
        } else {
            Some(HyprlandWorkspace {
                id: monitor["specialWorkspace"]["id"].as_i64().unwrap_or(-1) as i32,
                name: monitor["specialWorkspace"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                monitor: name.clone(),
                windows: 0,
            })
        };

        // Parse reserved space
        let reserved = [
            monitor["reserved"][0].as_i64().unwrap_or(0) as i32,
            monitor["reserved"][1].as_i64().unwrap_or(0) as i32,
            monitor["reserved"][2].as_i64().unwrap_or(0) as i32,
            monitor["reserved"][3].as_i64().unwrap_or(0) as i32,
        ];

        Ok(HyprlandMonitor {
            id,
            name,
            description,
            make,
            model,
            serial,
            width,
            height,
            refresh_rate,
            x,
            y,
            active_workspace,
            special_workspace,
            reserved,
            scale,
            transform,
            focused,
            dpms_status,
            vrr,
        })
    }

    fn parse_event(&self, line: &str) -> Result<HyprlandEvent> {
        let parts: Vec<&str> = line.trim().split(">>").collect();
        if parts.len() < 2 {
            return Err(crate::error::HyprsechyError::HyprlandIpc(format!(
                "Invalid event format: {}",
                line
            )));
        }

        let event_type = parts[0];
        let data = parts[1];

        match event_type {
            "openwindow" => {
                let parts: Vec<&str> = data.split(',').collect();
                if parts.len() >= 4 {
                    Ok(HyprlandEvent::WindowOpen {
                        window_address: parts[0].to_string(),
                        workspace: parts[1].to_string(),
                        class: parts[2].to_string(),
                        title: parts[3..].join(","),
                    })
                } else {
                    Err(crate::error::HyprsechyError::HyprlandIpc(
                        "Invalid openwindow event".to_string(),
                    ))
                }
            }
            "closewindow" => Ok(HyprlandEvent::WindowClose {
                window_address: data.to_string(),
            }),
            "movewindow" => {
                let parts: Vec<&str> = data.split(',').collect();
                if parts.len() >= 2 {
                    Ok(HyprlandEvent::WindowMove {
                        window_address: parts[0].to_string(),
                        workspace: parts[1].to_string(),
                    })
                } else {
                    Err(crate::error::HyprsechyError::HyprlandIpc(
                        "Invalid movewindow event".to_string(),
                    ))
                }
            }
            "createworkspace" => Ok(HyprlandEvent::WorkspaceCreate {
                workspace: data.to_string(),
            }),
            "destroyworkspace" => Ok(HyprlandEvent::WorkspaceDestroy {
                workspace: data.to_string(),
            }),
            "workspace" => Ok(HyprlandEvent::WorkspaceFocus {
                workspace: data.to_string(),
            }),
            "monitoradded" => Ok(HyprlandEvent::MonitorAdd {
                monitor: data.to_string(),
            }),
            "monitorremoved" => Ok(HyprlandEvent::MonitorRemove {
                monitor: data.to_string(),
            }),
            "windowtitle" => {
                let parts: Vec<&str> = data.splitn(2, ',').collect();
                if parts.len() >= 2 {
                    Ok(HyprlandEvent::WindowTitle {
                        window_address: parts[0].to_string(),
                        title: parts[1].to_string(),
                    })
                } else {
                    Err(crate::error::HyprsechyError::HyprlandIpc(
                        "Invalid windowtitle event".to_string(),
                    ))
                }
            }
            _ => {
                debug!("Unknown event type: {}", event_type);
                Err(crate::error::HyprsechyError::HyprlandIpc(format!(
                    "Unknown event type: {}",
                    event_type
                )))
            }
        }
    }
}

pub async fn exec_command(command: &str) -> Result<()> {
    debug!("Executing command via hyprctl: {}", command);

    let output = Command::new("hyprctl")
        .arg("dispatch")
        .arg("exec")
        .arg(command)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::error::HyprsechyError::HyprlandIpc(format!(
            "Failed to execute command '{}': {}",
            command, stderr
        )));
    }

    Ok(())
}
