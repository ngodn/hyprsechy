use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UwsmAppState {
    pub class: String,
    pub title: String,
    pub desktop_entry: Option<String>,
    pub command: Vec<String>,
    pub workspace_id: i32,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub monitor: String,
    pub floating: bool,
    pub fullscreen: bool,
    pub systemd_unit: Option<String>,
    pub uwsm_launched: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: Vec<UwsmAppState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorState {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub active_workspace: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub timestamp: u64,
    pub workspaces: Vec<WorkspaceState>,
    pub monitors: Vec<MonitorState>,
    pub active_workspace: i32,
    pub uwsm_apps: Vec<UwsmAppState>,
    pub systemd_units: Vec<SystemdServiceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdServiceInfo {
    pub unit_name: String,
    pub pid: u32,
    pub command: String,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceStatus {
    Active,
    Inactive,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub categories: Vec<String>,
    pub no_display: bool,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct HyprlandClient {
    pub address: String,
    pub class: String,
    pub title: String,
    pub workspace: HyprlandWorkspace,
    pub at: (i32, i32),
    pub size: (u32, u32),
    pub monitor: i32,
    pub floating: bool,
    pub fullscreen: bool,
    pub pid: u32,
}

#[derive(Debug, Clone)]
pub struct HyprlandWorkspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: i32,
}

#[derive(Debug, Clone)]
pub struct HyprlandMonitor {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub x: i32,
    pub y: i32,
    pub active_workspace: HyprlandWorkspace,
    pub special_workspace: Option<HyprlandWorkspace>,
    pub reserved: [i32; 4], // top, right, bottom, left
    pub scale: f32,
    pub transform: i32,
    pub focused: bool,
    pub dpms_status: bool,
    pub vrr: bool,
}

#[derive(Debug, Clone)]
pub struct WalkerIntegration {
    pub desktop_entries: HashMap<String, DesktopEntry>,
    pub frequent_apps: Vec<String>,
    pub custom_commands: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum HyprlandEvent {
    WindowOpen {
        window_address: String,
        workspace: String,
        class: String,
        title: String,
    },
    WindowClose {
        window_address: String,
    },
    WindowMove {
        window_address: String,
        workspace: String,
    },
    WorkspaceCreate {
        workspace: String,
    },
    WorkspaceDestroy {
        workspace: String,
    },
    WorkspaceFocus {
        workspace: String,
    },
    MonitorAdd {
        monitor: String,
    },
    MonitorRemove {
        monitor: String,
    },
    WindowTitle {
        window_address: String,
        title: String,
    },
    WindowClass {
        window_address: String,
        class: String,
    },
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub general: GeneralConfig,
    pub uwsm: UwsmConfig,
    pub walker: WalkerConfig,
    pub restoration: RestorationConfig,
    pub filters: FilterConfig,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub save_interval: u64,
    pub backup_count: u32,
    pub auto_restore: bool,
    pub verbose_logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UwsmConfig {
    pub enabled: bool,
    pub track_systemd_units: bool,
    pub preserve_unit_names: bool,
    pub launch_via_uwsm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkerConfig {
    pub enabled: bool,
    pub desktop_entry_dirs: Vec<String>,
    pub sync_frequent_apps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorationConfig {
    pub wait_for_systemd_ready: bool,
    pub systemd_timeout_ms: u64,
    pub prefer_desktop_entries: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    pub ignore_classes: Vec<String>,
    pub ignore_workspaces: Vec<i32>,
}
