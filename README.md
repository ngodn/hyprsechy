# Hyprsechy

A comprehensive session management service for Hyprland with UWSM integration, designed for the Omarchy ecosystem.

## Overview

Hyprsechy is a Rust-based daemon that automatically captures and restores Hyprland window sessions. It tracks workspace layouts, window positions, and application states, providing seamless session restoration across system restarts or Hyprland reloads.

## Features

### Core Functionality
- **Automatic Session Capture**: Real-time monitoring of workspace and window changes
- **Session Restoration**: Interactive TUI for restoring previous sessions on startup
- **UWSM Integration**: Full compatibility with Universal Wayland Session Manager
- **Walker Integration**: Desktop entry discovery and application launching
- **Systemd Service**: Runs as a user service with proper lifecycle management

### Session Management
- Workspace layout preservation
- Window positioning and sizing
- Application command tracking
- Monitor configuration support
- Multi-workspace session handling

### User Interface
- **Daemon Commands**: CLI interface for session operations
- **Restore TUI**: Interactive terminal interface for session restoration
- **Desktop Integration**: Launcher-accessible TUI applications

## Installation

### Prerequisites
- Hyprland window manager
- Rust toolchain (for building from source)
- UWSM (Universal Wayland Session Manager)
- systemd user services support

### Quick Install
```bash
git clone https://github.com/ngodn/hyprsechy
cd hyprsechy
./install.sh
```

The install script will:
1. Build the project with Cargo
2. Install binaries to `~/.local/bin/`
3. Set up systemd user service
4. Create configuration directories
5. Register TUI applications
6. Auto-detect user environment settings

### Manual Build
```bash
cargo build --release
cp target/release/hyprsechy ~/.local/bin/
cp target/release/hyprsechy-restore ~/.local/bin/
```

## Usage

### Service Management
```bash
# Check service status
systemctl --user status hyprsechy.service

# Start/stop service
systemctl --user start hyprsechy.service
systemctl --user stop hyprsechy.service

# View logs
journalctl --user -u hyprsechy.service -f
```

### Session Operations
```bash
# Save current session manually
hyprsechy save

# List saved sessions
hyprsechy list

# Restore session (CLI)
hyprsechy restore

# Interactive restore TUI
hyprsechy-restore
```

### Configuration
Configuration file: `~/.config/hyprsechy/config.toml`

User environment: `~/.config/hyprsechy/user-env.toml`

Session data: `~/.local/share/hyprsechy/session.json`

## Architecture

### Core Components

**Main Daemon** (`src/main.rs`)
- Service lifecycle management
- Session loading and TUI launching
- Command-line interface

**Session Manager** (`src/session.rs`)
- Workspace and window state capture
- Application restoration logic
- UWSM app integration

**Hyprland Integration** (`src/hyprland.rs`)
- IPC socket communication
- Event listening and command dispatch
- Window and workspace queries

**UWSM Integration** (`src/uwsm.rs`)
- Application launching and tracking
- Systemd unit management
- Desktop entry processing

**Restore TUI** (`src/restore_tui.rs`)
- Interactive session restoration
- Terminal-based user interface
- Session preview and selection

### Supporting Modules

**Configuration** (`src/config.rs`)
- TOML-based configuration management
- Default settings and validation

**Walker Integration** (`src/walker.rs`)
- Desktop entry discovery
- Application metadata extraction

**Types** (`src/types.rs`)
- Core data structures
- Session state definitions

**Error Handling** (`src/error.rs`)
- Structured error types
- Error context and reporting

### Scripts

**Installation Scripts**
- `install.sh`: Complete installation and setup
- `uninstall.sh`: Clean removal with backup options

**TUI Management**
- `scripts/hyprsechy-tui-install`: Register TUI applications
- `scripts/hyprsechy-tui-remove`: Unregister TUI applications
- `scripts/launch-restore-tui.sh`: Environment-aware TUI launcher

### Service Configuration
SystemD user service (`hyprsechy.service`) provides:
- Automatic startup with Hyprland session
- Proper environment variable handling
- Restart on failure with backoff
- Integration with Wayland session lifecycle

## Session Data Format

Sessions are stored in JSON format containing:
- **Workspaces**: Layout, windows, and monitor assignments
- **Windows**: Position, size, class, title, and commands
- **UWSM Apps**: Application state for restoration
- **Monitor Configuration**: Display setup and scaling
- **Metadata**: Timestamps and session identification

## Integration Features

### Omarchy Ecosystem
- Desktop entry pattern compatibility
- TUI window class conventions (TUI.float, TUI.tile)
- Launcher integration via desktop files
- Consistent user experience patterns

### UWSM Compatibility
- Automatic app detection via systemd
- Proper service lifecycle management
- Desktop entry-based launching
- Environment variable inheritance

### Hyprland Features
- Real-time event monitoring
- Workspace and window management
- Monitor configuration tracking
- Dynamic environment adaptation

## TUI Applications

Registered desktop applications for launcher access:

**Hyprsechy Session Restore**
- Interactive session restoration
- Session preview and selection
- Floating window interface

**Hyprsechy Config** (Future)
- Configuration management interface
- Settings adjustment and validation

## Development

### Building
```bash
cargo build --release
```

### Testing
```bash
cargo test
```

### Code Structure
- `src/`: Rust source code
- `scripts/`: Shell scripts for installation and TUI management
- `target/`: Build artifacts (gitignored)

## Troubleshooting

### Common Issues

**Service Not Starting**
- Check Hyprland is running: `echo $HYPRLAND_INSTANCE_SIGNATURE`
- Verify binary installation: `which hyprsechy`
- Check service logs: `journalctl --user -u hyprsechy.service`

**TUI Not Launching**
- Ensure proper environment variables in launch script
- Check terminal availability in user-env.toml
- Verify hyprsechy-restore binary exists

**Session Restoration Issues**
- Validate session.json format
- Check application availability
- Review UWSM integration logs

### Log Analysis
Service logs provide detailed information about:
- Session capture and save operations
- TUI launch attempts and completion
- Application restoration status
- Error conditions and recovery

## Uninstallation

```bash
./uninstall.sh
```

The uninstall script provides options for:
1. **Keep Everything**: Preserve configuration and data
2. **Backup and Remove**: Create compressed backup before removal
3. **Complete Removal**: Remove all files and data

## License

MIT License - see LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## Roadmap

- Configuration TUI interface
- Session management improvements
- Additional application integration
- Performance optimizations
- Extended monitor support