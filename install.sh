#!/bin/bash
# ============================================================
# DarkDM — Installer for Vivaldi/Chrome on Linux
# ============================================================
set -e

BOLD='\033[1m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[✓]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[✗]${NC} $1"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo -e "\n${BOLD}━━━ DarkDM Installer ━━━${NC}\n"

# ============================================================
# 1. Detect browser
# ============================================================
BROWSER=""
BROWSER_NAME=""
NMH_DIR=""

if [ -d "$HOME/.config/vivaldi" ]; then
    BROWSER="vivaldi"
    BROWSER_NAME="Vivaldi"
    NMH_DIR="$HOME/.config/vivaldi/NativeMessagingHosts"
elif [ -d "$HOME/.config/chromium" ]; then
    BROWSER="chromium"
    BROWSER_NAME="Chromium"
    NMH_DIR="$HOME/.config/chromium/NativeMessagingHosts"
elif [ -d "$HOME/.config/google-chrome" ]; then
    BROWSER="chrome"
    BROWSER_NAME="Google Chrome"
    NMH_DIR="$HOME/.config/google-chrome/NativeMessagingHosts"
elif [ -d "$HOME/.config/brave-browser" ]; then
    BROWSER="brave"
    BROWSER_NAME="Brave"
    NMH_DIR="$HOME/.config/brave-browser/NativeMessagingHosts"
else
    warn "No Chromium-based browser detected. Defaulting to Vivaldi."
    BROWSER="vivaldi"
    BROWSER_NAME="Vivaldi"
    NMH_DIR="$HOME/.config/vivaldi/NativeMessagingHosts"
fi

info "Detected browser: ${BROWSER_NAME}"

# ============================================================
# 2. Install native messaging host
# ============================================================
echo -e "\n${BOLD}Installing Native Messaging Host...${NC}"

mkdir -p "$NMH_DIR"

# Copy binary
HOST_BIN="$HOME/.local/bin/darkdm-host"
mkdir -p "$HOME/.local/bin"
cp "$SCRIPT_DIR/native-host/target/release/darkdm-host" "$HOST_BIN"
chmod +x "$HOST_BIN"
info "Binary installed: $HOST_BIN"

# Generate manifest with correct extension ID
EXTENSION_ID="${EXTENSION_ID:-lghgegmbhoinnmcbaipdcieenakfocen}"
MANIFEST_PATH="$NMH_DIR/com.darkdm.manager.json"

cat > "$MANIFEST_PATH" << EOF
{
  "name": "com.darkdm.manager",
  "description": "DarkDM Native Messaging Host - Video Download Manager",
  "path": "$HOST_BIN",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${EXTENSION_ID}/"]
}
EOF
info "Native manifest installed: $MANIFEST_PATH"

# ============================================================
# 3. Install extension
# ============================================================
echo -e "\n${BOLD}Extension setup...${NC}"

EXT_DIR="$HOME/.local/share/darkdm/extension"
mkdir -p "$EXT_DIR"
cp -r "$SCRIPT_DIR/extension/"* "$EXT_DIR/"
info "Extension files copied to: $EXT_DIR"

echo -e "\n${BOLD}━━━ Installation Complete ━━━${NC}\n"
echo -e "To load the extension in ${BROWSER_NAME}:"
echo -e "  1. Open ${BROWSER_NAME} and go to: ${YELLOW}${BROWSER}://extensions${NC}"
echo -e "  2. Enable ${BOLD}Developer Mode${NC} (toggle top-right)"
echo -e "  3. Click ${BOLD}Load unpacked${NC} and select:"
echo -e "     ${GREEN}${EXT_DIR}${NC}"
echo -e ""
echo -e "To start the native host test:"
echo -e "  ${YELLOW}echo '{\"type\":\"PING\"}' | $HOST_BIN${NC}"
echo -e ""
echo -e "For Netflix/yt-dlp support, install:"
echo -e "  ${YELLOW}pip install yt-dlp${NC}  or  ${YELLOW}sudo pacman -S yt-dlp${NC}"
echo -e ""
echo -e "NOTE: Update EXTENSION_ID in this script after the extension is loaded."
echo -e "Get your extension ID from ${BROWSER}://extensions after loading."
