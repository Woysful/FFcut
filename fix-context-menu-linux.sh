#!/bin/bash

# FFcut Context Menu Fix Script for Linux
# This script fixes permission issues with context menu integration on KDE Plasma 6 and other DEs
# Also handles AppImage path issues

echo "========================================="
echo "FFcut Context Menu Fix Script"
echo "========================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to check and fix file permissions
fix_file_permissions() {
    local file="$1"
    local name="$2"
    
    if [ -f "$file" ]; then
        echo -n "Checking $name... "
        
        if [ -x "$file" ]; then
            echo -e "${GREEN}✓ Already executable${NC}"
        else
            chmod +x "$file" 2>/dev/null
            if [ $? -eq 0 ]; then
                echo -e "${GREEN}✓ Fixed - made executable${NC}"
            else
                echo -e "${RED}✗ Failed to set permissions${NC}"
                return 1
            fi
        fi
    else
        echo -e "${YELLOW}⚠ $name not found (may not be installed)${NC}"
        return 2
    fi
    
    return 0
}

# Function to check if path in desktop file is valid
check_desktop_file_path() {
    local file="$1"
    
    if [ ! -f "$file" ]; then
        return 1
    fi
    
    # Extract Exec path from desktop file
    local exec_path=$(grep "^Exec=" "$file" | sed 's/^Exec="\?\([^"]*\)"\?.*/\1/' | awk '{print $1}')
    
    if [ -z "$exec_path" ]; then
        return 1
    fi
    
    # Check if it's a temp mount path (AppImage issue)
    if [[ "$exec_path" == /tmp/.mount_* ]]; then
        echo -e "${RED}✗ Invalid path detected: $exec_path${NC}"
        echo -e "${YELLOW}  This is a temporary AppImage mount path!${NC}"
        return 2
    fi
    
    # Check if file exists and is executable
    if [ ! -x "$exec_path" ]; then
        echo -e "${RED}✗ Executable not found or not executable: $exec_path${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✓ Valid executable path: $exec_path${NC}"
    return 0
}

# Detect desktop environment
detect_de() {
    if [ -n "$XDG_CURRENT_DESKTOP" ]; then
        echo "$XDG_CURRENT_DESKTOP"
    elif [ -n "$DESKTOP_SESSION" ]; then
        echo "$DESKTOP_SESSION"
    else
        echo "unknown"
    fi
}

DE=$(detect_de)
echo "Detected Desktop Environment: $DE"
echo ""

# Check for AppImage
echo "Checking for AppImage issues..."
DESKTOP_FILE="$HOME/.local/share/applications/ffcut-open.desktop"

if [ -f "$DESKTOP_FILE" ]; then
    if ! check_desktop_file_path "$DESKTOP_FILE"; then
        echo ""
        echo -e "${BLUE}=========================================${NC}"
        echo -e "${BLUE}AppImage Path Issue Detected!${NC}"
        echo -e "${BLUE}=========================================${NC}"
        echo ""
        echo "The desktop file contains a temporary mount path."
        echo "This happens when:"
        echo "  1. You enabled context menu while running FFcut from a temp location"
        echo "  2. You're running FFcut as AppImage without proper APPIMAGE env var"
        echo ""
        echo "To fix this:"
        echo "  1. Move FFcut AppImage to a permanent location (e.g., ~/Applications/)"
        echo "  2. Run FFcut from that location"
        echo "  3. Disable context menu integration"
        echo "  4. Re-enable context menu integration"
        echo ""
        echo "Or run this command manually:"
        echo "  rm '$DESKTOP_FILE'"
        echo "  Then re-enable context menu from within FFcut"
        echo ""
        echo -e "${BLUE}=========================================${NC}"
        echo ""
    fi
else
    echo -e "${YELLOW}⚠ Desktop file not found${NC}"
fi

# Check icon installation
echo ""
echo "Checking icon installation..."
ICON_FILE="$HOME/.local/share/icons/hicolor/256x256/apps/ffcut.png"

if [ -f "$ICON_FILE" ]; then
    echo -e "${GREEN}✓ Icon installed at: $ICON_FILE${NC}"
    
    # Check if icon is referenced in desktop files
    if [ -f "$DESKTOP_FILE" ]; then
        ICON_REF=$(grep "^Icon=" "$DESKTOP_FILE" | cut -d= -f2)
        if [ "$ICON_REF" = "ffcut" ]; then
            echo -e "${GREEN}✓ Desktop file uses icon name: $ICON_REF${NC}"
        elif [ "$ICON_REF" = "video-x-generic" ]; then
            echo -e "${YELLOW}⚠ Using fallback icon (ffcut icon not found during setup)${NC}"
            echo -e "  To fix: disable and re-enable context menu integration in FFcut"
        else
            echo -e "${YELLOW}⚠ Desktop file uses icon: $ICON_REF${NC}"
        fi
    fi
    
    # Check KDE service menu icon
    KDE_SERVICE_FILE="$HOME/.local/share/kio/servicemenus/ffcut-open.desktop"
    if [ -f "$KDE_SERVICE_FILE" ]; then
        KDE_ICON_REF=$(grep "^Icon=" "$KDE_SERVICE_FILE" | cut -d= -f2)
        if [ "$KDE_ICON_REF" = "ffcut" ]; then
            echo -e "${GREEN}✓ KDE service menu uses icon name: $KDE_ICON_REF${NC}"
        else
            echo -e "${YELLOW}⚠ KDE service menu uses icon: $KDE_ICON_REF${NC}"
            echo -e "  To fix: disable and re-enable context menu integration in FFcut"
        fi
    fi
else
    echo -e "${YELLOW}⚠ Icon not installed${NC}"
    echo -e "  Icon will be installed automatically when you enable context menu"
    echo -e "  If icon doesn't appear, try: disable and re-enable context menu integration"
fi

echo ""
echo "Fixing permissions..."
echo ""

# Fix main .desktop file
echo "1. Main application desktop file:"
fix_file_permissions "$HOME/.local/share/applications/ffcut-open.desktop" "ffcut-open.desktop"
echo ""

# Fix KDE service menu (if KDE)
if [[ "$DE" == *"KDE"* ]] || [[ "$DE" == *"kde"* ]] || [[ "$DE" == *"Plasma"* ]] || [[ "$DE" == *"plasma"* ]]; then
    echo "2. KDE Service Menu:"
    fix_file_permissions "$HOME/.local/share/kio/servicemenus/ffcut-open.desktop" "KDE service menu"
    echo ""
    
    echo "3. Updating KDE cache..."
    if command -v kbuildsycoca6 &> /dev/null; then
        kbuildsycoca6 --noincremental &>/dev/null
        echo -e "${GREEN}✓ KDE cache updated (Plasma 6)${NC}"
    elif command -v kbuildsycoca5 &> /dev/null; then
        kbuildsycoca5 --noincremental &>/dev/null
        echo -e "${GREEN}✓ KDE cache updated (Plasma 5)${NC}"
    else
        echo -e "${YELLOW}⚠ kbuildsycoca not found - please restart Dolphin manually${NC}"
    fi
    echo ""
fi

# Fix GNOME/Nautilus script (if GNOME)
if [[ "$DE" == *"GNOME"* ]] || [[ "$DE" == *"gnome"* ]] || [[ "$DE" == *"Ubuntu"* ]] || [[ "$DE" == *"ubuntu"* ]]; then
    echo "2. GNOME/Nautilus script:"
    fix_file_permissions "$HOME/.local/share/nautilus/scripts/Edit with FFcut" "Nautilus script"
    echo ""
fi

# Update desktop database
echo "4. Updating desktop database..."
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$HOME/.local/share/applications" &>/dev/null
    echo -e "${GREEN}✓ Desktop database updated${NC}"
else
    echo -e "${YELLOW}⚠ update-desktop-database not found${NC}"
fi
echo ""

# Final recommendations
echo "========================================="
echo "Recommendations:"
echo "========================================="

if [[ "$DE" == *"KDE"* ]] || [[ "$DE" == *"kde"* ]] || [[ "$DE" == *"Plasma"* ]] || [[ "$DE" == *"plasma"* ]]; then
    echo "For KDE Plasma:"
    echo "  1. Restart Dolphin:"
    echo "     killall dolphin && dolphin &"
    echo ""
    echo "  2. Or logout and login again"
elif [[ "$DE" == *"GNOME"* ]] || [[ "$DE" == *"gnome"* ]]; then
    echo "For GNOME:"
    echo "  1. Restart Nautilus:"
    echo "     nautilus -q"
    echo ""
    echo "  2. Or logout and login again"
else
    echo "  Restart your file manager or logout and login again"
fi

echo ""
echo "========================================="
echo "To verify the fix worked:"
echo "========================================="
echo "Right-click on any video file (.mp4, .mkv, etc.)"
echo "and check if 'Edit with FFcut' appears in the menu."
echo ""
echo -e "${GREEN}Done!${NC}"
