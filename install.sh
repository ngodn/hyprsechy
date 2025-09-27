#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Hyprsechy Installation Script${NC}"
echo "=============================="

# Check if running in Hyprland
if [ -z "$HYPRLAND_INSTANCE_SIGNATURE" ]; then
    echo -e "${YELLOW}Warning: Not running in Hyprland. Some features may not work.${NC}"
fi

# Create directories
echo -e "${BLUE}Creating directories...${NC}"
mkdir -p ~/.local/bin
mkdir -p ~/.config/systemd/user
mkdir -p ~/.local/share/hyprsechy
mkdir -p ~/.local/share/hyprsechy/bin

# Build the project
echo -e "${BLUE}Building hyprsechy...${NC}"
if ! cargo build --release; then
    echo -e "${RED}Failed to build hyprsechy${NC}"
    exit 1
fi

# Install binaries
echo -e "${BLUE}Installing binaries...${NC}"
cp target/release/hyprsechy ~/.local/bin/
cp target/release/hyprsechy-restore ~/.local/bin/
chmod +x ~/.local/bin/hyprsechy
chmod +x ~/.local/bin/hyprsechy-restore

# Install TUI management scripts
echo -e "${BLUE}Installing TUI management scripts...${NC}"
cp scripts/hyprsechy-tui-install ~/.local/share/hyprsechy/bin/
cp scripts/hyprsechy-tui-remove ~/.local/share/hyprsechy/bin/
cp scripts/launch-restore-tui.sh ~/.local/share/hyprsechy/bin/
chmod +x ~/.local/share/hyprsechy/bin/hyprsechy-tui-*
chmod +x ~/.local/share/hyprsechy/bin/launch-restore-tui.sh

# Install systemd service
echo -e "${BLUE}Installing systemd service...${NC}"
cp hyprsechy.service ~/.config/systemd/user/

# Reload systemd user daemon
echo -e "${BLUE}Reloading systemd user daemon...${NC}"
systemctl --user daemon-reload

# Enable and start the service
echo -e "${BLUE}Enabling and starting hyprsechy service...${NC}"
systemctl --user enable hyprsechy.service
systemctl --user restart hyprsechy.service

# Handle configuration - preserve existing or create default
CONFIG_FILE="~/.config/hyprsechy/config.toml"
if [ -f ~/.config/hyprsechy/config.toml ]; then
    echo -e "${YELLOW}Existing configuration found, preserving user settings...${NC}"
    # Create backup for safety
    cp ~/.config/hyprsechy/config.toml ~/.config/hyprsechy/config.toml.backup.$(date +%s)
    echo -e "${GREEN}Configuration backup created: config.toml.backup.$(date +%s)${NC}"
else
    echo -e "${BLUE}Creating default configuration...${NC}"
    mkdir -p ~/.config/hyprsechy
    ~/.local/bin/hyprsechy init-config > /dev/null 2>&1 || true
fi

# Detect user environment dynamically like ivetouchbarbtw
echo -e "${BLUE}Detecting user environment...${NC}"
CURRENT_USER=$(whoami)
USER_HOME="$HOME"
USER_UID=$(id -u)
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$USER_UID}"

# Detect Wayland display
WAYLAND_DISPLAY_VALUE="${WAYLAND_DISPLAY:-wayland-1}"
if [ -d "$RUNTIME_DIR" ]; then
    for socket in "$RUNTIME_DIR"/wayland-*; do
        if [ -S "$socket" ] && [[ ! "$socket" == *.lock ]]; then
            WAYLAND_DISPLAY_VALUE=$(basename "$socket")
            break
        fi
    done
fi

# Note: Hyprland signature changes on each restart, so we use dynamic detection in the code

# Detect terminal preference
TERMINAL_PREF="${TERMINAL:-}"
if [ -z "$TERMINAL_PREF" ]; then
    for term in alacritty kitty foot xterm; do
        if command -v "$term" >/dev/null 2>&1; then
            TERMINAL_PREF="$term"
            break
        fi
    done
fi

echo "Detected user: $CURRENT_USER"
echo "Detected UID: $USER_UID"
echo "Detected runtime dir: $RUNTIME_DIR"
echo "Detected Wayland display: $WAYLAND_DISPLAY_VALUE"
echo "Detected terminal: $TERMINAL_PREF"
echo "Note: Hyprland signature will be detected dynamically at runtime"

# Handle user environment configuration - preserve existing or update
USER_ENV_FILE="~/.config/hyprsechy/user-env.toml"
if [ -f ~/.config/hyprsechy/user-env.toml ]; then
    echo -e "${YELLOW}Existing user environment found, backing up and updating...${NC}"
    cp ~/.config/hyprsechy/user-env.toml ~/.config/hyprsechy/user-env.toml.backup.$(date +%s)
    echo -e "${GREEN}User environment backup created${NC}"
else
    echo -e "${BLUE}Creating user environment configuration...${NC}"
fi

mkdir -p ~/.config/hyprsechy
cat > ~/.config/hyprsechy/user-env.toml <<EOF
# Auto-generated user environment configuration
# Note: Hyprland signature is detected dynamically at runtime
[user_environment]
username = "$CURRENT_USER"
uid = $USER_UID
home_dir = "$USER_HOME"
runtime_dir = "$RUNTIME_DIR"
wayland_display = "$WAYLAND_DISPLAY_VALUE"
terminal = "$TERMINAL_PREF"
EOF

# Register TUI applications
echo -e "${BLUE}Registering TUI applications...${NC}"
~/.local/share/hyprsechy/bin/hyprsechy-tui-install

# Check for existing session data
SESSION_DATA_DIR="~/.local/share/hyprsechy"
if [ -f ~/.local/share/hyprsechy/session.json ]; then
    echo -e "${YELLOW}Existing session data found and will be preserved${NC}"
    echo "Session data location: ~/.local/share/hyprsechy/"
fi

echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Service status:"
systemctl --user status hyprsechy.service --no-pager || true
echo ""
echo "Check status anytime: ${YELLOW}systemctl --user status hyprsechy.service${NC}"
echo ""
echo "Commands available:"
echo "- Save session: ${YELLOW}hyprsechy save${NC}"
echo "- Restore session: ${YELLOW}hyprsechy restore${NC}"
echo "- List sessions: ${YELLOW}hyprsechy list${NC}"
echo "- Interactive restore: ${YELLOW}hyprsechy-restore${NC}"
echo ""
echo "TUI Applications registered:"
echo "- ${YELLOW}Hyprsechy Session Restore${NC} - Available in app launcher (SUPER+SPACE)"
echo "- ${YELLOW}Hyprsechy Config${NC} - Configuration manager (if available)"
echo ""
echo "TUI Management:"
echo "- Install TUIs: ${YELLOW}~/.local/share/hyprsechy/bin/hyprsechy-tui-install${NC}"
echo "- Remove TUIs: ${YELLOW}~/.local/share/hyprsechy/bin/hyprsechy-tui-remove${NC}"
echo ""
echo -e "${BLUE}Make sure ~/.local/bin is in your PATH!${NC}"