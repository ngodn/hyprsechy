#!/bin/bash

# Launch script for hyprsechy-restore TUI
# Uses dynamic detection like ivetouchbarbtw

# Load user environment config if available
USER_ENV_FILE="$HOME/.config/hyprsechy/user-env.toml"

if [ -f "$USER_ENV_FILE" ]; then
    # Extract terminal from config
    TERMINAL=$(grep "terminal" "$USER_ENV_FILE" | cut -d'"' -f2)
    RUNTIME_DIR=$(grep "runtime_dir" "$USER_ENV_FILE" | cut -d'"' -f2)
    WAYLAND_DISPLAY=$(grep "wayland_display" "$USER_ENV_FILE" | cut -d'"' -f2)
else
    # Fallback detection
    TERMINAL="${TERMINAL:-alacritty}"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
fi

# Dynamic Hyprland signature detection (changes on each restart)
HYPR_SIG=""
if [ -d "$RUNTIME_DIR/hypr" ]; then
    for hypr_dir in "$RUNTIME_DIR"/hypr/*; do
        if [ -d "$hypr_dir" ] && [ -S "$hypr_dir/.socket.sock" ]; then
            HYPR_SIG=$(basename "$hypr_dir")
            break
        fi
    done
fi

# Set environment variables
export XDG_RUNTIME_DIR="$RUNTIME_DIR"
export WAYLAND_DISPLAY="$WAYLAND_DISPLAY"
if [ -n "$HYPR_SIG" ]; then
    export HYPRLAND_INSTANCE_SIGNATURE="$HYPR_SIG"
fi

# Launch the TUI via hyprctl with proper environment
exec hyprctl dispatch exec "uwsm app -- $TERMINAL --class TUI.float -e bash -c 'export PATH=\$HOME/.local/bin:\$PATH; export HYPRLAND_INSTANCE_SIGNATURE=$HYPR_SIG; export XDG_RUNTIME_DIR=$RUNTIME_DIR; export WAYLAND_DISPLAY=$WAYLAND_DISPLAY; hyprsechy-restore; read -p \"Press Enter to close\"'"