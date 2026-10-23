#!/bin/sh
# Installs Joey for the current user only (no sudo): builds it, then puts
# the programs, the menu entry and the icon in ~/.local.
# Run ./install.sh --uninstall to remove everything it installed.
set -eu

APP_ID=io.github.outlook791.Joey
BIN="$HOME/.local/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
cd "$(dirname "$0")"

refresh() {
    update-desktop-database "$APPS" 2>/dev/null || true
    gtk-update-icon-cache -q -t "${ICONS%/scalable/apps}" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN/joey" "$BIN/joey-gtk" "$APPS/$APP_ID.desktop" "$ICONS/$APP_ID.svg"
    refresh
    echo "Joey removed."
    exit 0
fi

cargo build --release
mkdir -p "$BIN" "$APPS" "$ICONS"
install -m 755 target/release/joey target/release/joey-gtk "$BIN/"
# Full path in Exec, so the menu works even if ~/.local/bin isn't on PATH.
sed "s|^Exec=joey-gtk|Exec=$BIN/joey-gtk|" "data/$APP_ID.desktop" > "$APPS/$APP_ID.desktop"
install -m 644 "data/$APP_ID.svg" "$ICONS/"
refresh
echo "Joey installed. Look for it in your app menu (it may take a few seconds to appear)."
