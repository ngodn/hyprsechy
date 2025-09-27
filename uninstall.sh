#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Hyprsechy Uninstallation Script${NC}"
echo "==============================="

# Stop and disable service if running
if systemctl --user is-active --quiet hyprsechy.service 2>/dev/null; then
    echo -e "${BLUE}Stopping hyprsechy service...${NC}"
    systemctl --user stop hyprsechy.service
fi

if systemctl --user is-enabled --quiet hyprsechy.service 2>/dev/null; then
    echo -e "${BLUE}Disabling hyprsechy service...${NC}"
    systemctl --user disable hyprsechy.service
fi

# Remove systemd service file
if [ -f ~/.config/systemd/user/hyprsechy.service ]; then
    echo -e "${BLUE}Removing systemd service file...${NC}"
    rm ~/.config/systemd/user/hyprsechy.service
fi

# Remove binaries
if [ -f ~/.local/bin/hyprsechy ]; then
    echo -e "${BLUE}Removing binaries...${NC}"
    rm ~/.local/bin/hyprsechy
fi

if [ -f ~/.local/bin/hyprsechy-restore ]; then
    rm ~/.local/bin/hyprsechy-restore
fi

# Remove TUI applications
if [ -f ~/.local/share/hyprsechy/bin/hyprsechy-tui-remove ]; then
    echo -e "${BLUE}Removing TUI applications...${NC}"
    ~/.local/share/hyprsechy/bin/hyprsechy-tui-remove
fi

# Reload systemd user daemon
echo -e "${BLUE}Reloading systemd user daemon...${NC}"
systemctl --user daemon-reload

# Ask about configuration and data
echo ""
echo -e "${YELLOW}What would you like to do with your configuration and session data?${NC}"
echo "Options:"
echo "1) Keep everything (recommended for updates)"
echo "2) Backup and remove (creates a compressed backup)"
echo "3) Remove completely (no backup)"
echo ""
echo "Data includes:"
echo "- ~/.config/hyprsechy/ (configuration files)"
echo "- ~/.local/share/hyprsechy/ (session data and scripts)"
echo ""
read -p "Choose option [1-3]: " -n 1 -r
echo

case $REPLY in
    2)
        BACKUP_DIR="$HOME/hyprsechy-backup-$(date +%Y%m%d-%H%M%S)"
        echo -e "${BLUE}Creating backup at: $BACKUP_DIR${NC}"
        mkdir -p "$BACKUP_DIR"

        if [ -d ~/.config/hyprsechy ]; then
            echo "Backing up configuration..."
            cp -r ~/.config/hyprsechy "$BACKUP_DIR/config"
        fi

        if [ -d ~/.local/share/hyprsechy ]; then
            echo "Backing up session data..."
            cp -r ~/.local/share/hyprsechy "$BACKUP_DIR/data"
        fi

        # Create compressed backup
        echo "Compressing backup..."
        cd "$HOME"
        tar -czf "$(basename "$BACKUP_DIR").tar.gz" "$(basename "$BACKUP_DIR")"
        rm -rf "$BACKUP_DIR"

        echo -e "${GREEN}Backup created: $(basename "$BACKUP_DIR").tar.gz${NC}"

        # Now remove the data
        if [ -d ~/.config/hyprsechy ]; then
            echo -e "${BLUE}Removing configuration...${NC}"
            rm -rf ~/.config/hyprsechy
        fi

        if [ -d ~/.local/share/hyprsechy ]; then
            echo -e "${BLUE}Removing session data and scripts...${NC}"
            rm -rf ~/.local/share/hyprsechy
        fi

        echo -e "${GREEN}Complete uninstallation with backup finished!${NC}"
        echo -e "${YELLOW}To restore later: tar -xzf $(basename "$BACKUP_DIR").tar.gz && cp -r $(basename "$BACKUP_DIR")/* ~/${NC}"
        ;;
    3)
        if [ -d ~/.config/hyprsechy ]; then
            echo -e "${BLUE}Removing configuration...${NC}"
            rm -rf ~/.config/hyprsechy
        fi

        if [ -d ~/.local/share/hyprsechy ]; then
            echo -e "${BLUE}Removing session data and scripts...${NC}"
            rm -rf ~/.local/share/hyprsechy
        fi

        echo -e "${GREEN}Complete uninstallation finished!${NC}"
        ;;
    *)
        echo -e "${GREEN}Uninstallation complete! Configuration and data preserved.${NC}"
        echo -e "${YELLOW}Your data is kept for future reinstallation.${NC}"
        ;;
esac

echo ""
echo "Hyprsechy has been removed from your system."