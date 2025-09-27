use thiserror::Error;

pub type Result<T> = std::result::Result<T, HyprsechyError>;

#[derive(Error, Debug)]
pub enum HyprsechyError {
    #[error("Hyprland IPC error: {0}")]
    HyprlandIpc(String),

    #[error("Systemd error: {0}")]
    Systemd(String),

    #[error("UWSM integration error: {0}")]
    Uwsm(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Walker integration error: {0}")]
    Walker(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Desktop entry error: {0}")]
    DesktopEntry(String),
}
