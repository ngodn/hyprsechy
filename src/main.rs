use anyhow::Result;
use clap::{Parser, Subcommand};
use hyprsechy::config::Config;
use hyprsechy::session::SessionManager;
use hyprsechy::uwsm::UwsmIntegration;
use std::process;
use tracing::{debug, error, info, Level};
use tracing_subscriber;

#[derive(Parser)]
#[command(
    name = "hyprsechy",
    about = "Hyprland Session Management Service with UWSM Integration",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(short, long, global = true)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run as daemon
    Daemon {
        #[arg(long)]
        uwsm_integration: bool,
    },
    /// Save current session
    Save {
        /// Optional session name
        name: Option<String>,
        #[arg(long)]
        include_systemd_units: bool,
    },
    /// Restore specific session
    Restore {
        /// Optional session name (defaults to latest)
        name: Option<String>,
        #[arg(long)]
        use_uwsm_launcher: bool,
    },
    /// List saved sessions
    List,
    /// Show daemon status
    Status {
        #[arg(long)]
        show_systemd_units: bool,
    },
    /// Sync walker database
    SyncWalker,
    /// Export walker config
    ExportWalker,
    /// Install systemd service
    Install {
        #[arg(long)]
        uwsm: bool,
    },
    /// Create default configuration
    InitConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    // Load configuration
    let config = match load_config(cli.config.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load config: {}", e);
            process::exit(1);
        }
    };

    // Execute command
    match run_command(cli.command, config).await {
        Ok(_) => {}
        Err(e) => {
            error!("Command failed: {}", e);
            process::exit(1);
        }
    }

    Ok(())
}

async fn run_command(command: Commands, config: Config) -> Result<()> {
    match command {
        Commands::Daemon { uwsm_integration } => {
            info!("Starting hyprsechy daemon");
            if uwsm_integration {
                info!("UWSM integration enabled");
            }
            run_daemon(config).await
        }
        Commands::Save { name, include_systemd_units } => {
            info!("Saving session");
            save_session(config, name, include_systemd_units).await
        }
        Commands::Restore { name, use_uwsm_launcher } => {
            info!("Restoring session");
            restore_session(config, name, use_uwsm_launcher).await
        }
        Commands::List => {
            list_sessions(config).await
        }
        Commands::Status { show_systemd_units } => {
            show_status(config, show_systemd_units).await
        }
        Commands::SyncWalker => {
            sync_walker(config).await
        }
        Commands::ExportWalker => {
            export_walker_config(config).await
        }
        Commands::Install { uwsm } => {
            install_service(uwsm).await
        }
        Commands::InitConfig => {
            hyprsechy::config::create_default_config()?;
            Ok(())
        }
    }
}

fn load_config(config_path: Option<&str>) -> Result<Config> {
    if let Some(path) = config_path {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    } else {
        Config::load().map_err(Into::into)
    }
}

async fn run_daemon(config: Config) -> Result<()> {
    info!("Waiting for Hyprland to be ready...");

    // Wait for Hyprland to be available with retries
    let mut retries = 30; // Wait up to 30 seconds
    loop {
        match SessionManager::new(config.clone()).await {
            Ok(session_manager) => {
                info!("Hyprland is ready, initializing session manager");
                return run_session_manager(session_manager, config).await;
            }
            Err(e) if retries > 0 => {
                debug!("Hyprland not ready yet: {}. Retrying in 1 second...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                retries -= 1;
            }
            Err(e) => {
                error!("Failed to connect to Hyprland after 30 seconds: {}", e);
                return Err(e.into());
            }
        }
    }
}

async fn run_session_manager(session_manager: SessionManager, config: Config) -> Result<()> {

    // Launch restore TUI if auto_restore is enabled and session exists
    let tui_launched = if config.general.auto_restore {
        if let Ok(Some(_)) = session_manager.load_session().await {
            info!("Previous session found, launching restore TUI using proven method");
            if let Err(e) = launch_restore_tui_proven().await {
                error!("Failed to launch restore TUI: {}", e);
                false
            } else {
                info!("TUI launched successfully - waiting for user decision");
                true
            }
        } else {
            info!("No previous session found");
            false
        }
    } else {
        false
    };

    // If TUI was launched, wait for user decision before starting event listener
    if tui_launched {
        info!("Waiting for TUI completion before starting session tracking...");
        // Wait for TUI process to finish (user made decision)
        let mut wait_count = 0;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Check if TUI process is still running
            let output = tokio::process::Command::new("pgrep")
                .arg("-f")
                .arg("hyprsechy-restore")
                .output()
                .await;

            if let Ok(output) = output {
                if output.stdout.is_empty() {
                    info!("TUI completed - user made their decision, starting session tracking");
                    break;
                }
            }

            wait_count += 1;
            if wait_count > 300 { // 5 minutes max wait
                info!("TUI wait timeout - starting session tracking anyway");
                break;
            }
        }
    } else {
        // Wait a moment for Hyprland to be fully ready
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // Now safe to start Hyprland event listener for real-time session tracking
    let session_manager_clone = session_manager.clone();
    tokio::spawn(async move {
        info!("Starting Hyprland event listener");
        if let Err(e) = session_manager_clone.start_event_listener().await {
            error!("Event listener failed: {}", e);
        }
    });

    // Start periodic auto-save loop as backup
    tokio::spawn({
        let session_manager = session_manager;
        async move {
            if let Err(e) = session_manager.auto_save_loop().await {
                error!("Auto-save loop failed: {}", e);
            }
        }
    });

    // Keep daemon running
    info!("Daemon started, waiting for shutdown signal");
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received, stopping daemon");

    Ok(())
}

async fn save_session(config: Config, _name: Option<String>, _include_systemd_units: bool) -> Result<()> {
    let session_manager = SessionManager::new(config).await?;
    let session = session_manager.capture_current_session().await?;
    session_manager.save_session(&session).await?;
    println!("Session saved successfully");
    Ok(())
}

async fn restore_session(config: Config, _name: Option<String>, _use_uwsm_launcher: bool) -> Result<()> {
    let session_manager = SessionManager::new(config).await?;

    if let Some(session) = session_manager.load_session().await? {
        session_manager.restore_session(&session).await?;
        println!("Session restored successfully");
    } else {
        println!("No saved session found");
    }

    Ok(())
}

async fn list_sessions(config: Config) -> Result<()> {
    let session_path = config.session_file_path()?;

    if session_path.exists() {
        let session_manager = SessionManager::new(config).await?;
        if let Some(session) = session_manager.load_session().await? {
            println!("Main session:");
            println!("  Timestamp: {}", session.timestamp);
            println!("  Workspaces: {}", session.workspaces.len());
            println!("  Windows: {}", session.uwsm_apps.len());
            println!("  Monitors: {}", session.monitors.len());
        }
    } else {
        println!("No saved sessions found");
    }

    Ok(())
}

async fn show_status(config: Config, show_systemd_units: bool) -> Result<()> {
    let session_manager = SessionManager::new(config.clone()).await?;
    let info = session_manager.get_session_info().await;

    println!("Hyprsechy Status:");
    println!("  UWSM Available: {}", info.uwsm_available);
    println!("  Walker Enabled: {}", info.walker_enabled);
    println!("  Has Saved Session: {}", info.has_saved_session);
    println!("  Current Workspaces: {}", info.current_workspaces);
    println!("  Current Windows: {}", info.current_windows);

    if let Some(timestamp) = info.current_session_timestamp {
        println!("  Last Capture: {}", timestamp);
    }

    if show_systemd_units {
        let uwsm = UwsmIntegration::new().await;
        if let Ok(status) = uwsm.get_uwsm_status().await {
            println!("  UWSM Active Apps: {}", status.active_apps);
            println!("  UWSM Total Units: {}", status.total_units);
        }
    }

    Ok(())
}

async fn sync_walker(config: Config) -> Result<()> {
    let session_manager = SessionManager::new(config).await?;
    session_manager.refresh_walker().await?;
    println!("Walker database synced");
    Ok(())
}

async fn export_walker_config(_config: Config) -> Result<()> {
    // TODO: Implement walker config export
    println!("Walker config export not yet implemented");
    Ok(())
}

async fn install_service(uwsm: bool) -> Result<()> {
    if uwsm {
        println!("Installing as UWSM service...");
        // TODO: Implement UWSM service installation
    } else {
        println!("Installing as regular systemd service...");
        // TODO: Implement regular service installation
    }
    println!("Service installation not yet implemented");
    Ok(())
}

async fn launch_restore_tui_proven() -> Result<()> {
    info!("Launching TUI using ivetouchbarbtw proven method");

    // Use the proven launch script based on ivetouchbarbtw patterns
    let script_path = std::env::var("HOME")
        .map(|home| format!("{}/.local/share/hyprsechy/bin/launch-restore-tui.sh", home))
        .unwrap_or_else(|_| "/tmp/launch-restore-tui.sh".to_string());

    if !std::path::Path::new(&script_path).exists() {
        return Err(anyhow::anyhow!("TUI launch script not found: {}", script_path));
    }

    info!("Using proven launch script: {}", script_path);

    // Spawn the script in background to avoid blocking
    match tokio::process::Command::new("bash")
        .arg(&script_path)
        .spawn()
    {
        Ok(_) => {
            info!("TUI launch script started successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to start TUI launch script: {}", e);
            Err(anyhow::anyhow!("Failed to start TUI launch script: {}", e))
        }
    }
}

