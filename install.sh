#!/bin/sh
# Installs Joey for the current user only (no sudo): builds it, then puts
# the programs, the menu entry and the icon in ~/.local.
# Run ./install.sh --uninstall to remove everything it installed.
set -eu

APP_ID=io.github.outlook791.Joey
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN="$HOME/.local/bin"
APPS="$DATA/applications"
HICOLOR="$DATA/icons/hicolor"
cd "$(dirname "$0")"

refresh() {
    update-desktop-database "$APPS" 2>/dev/null || true
    # A partial icon cache in the home folder hides icons from the menu;
    # without one, the system finds them by itself.
    rm -f "$HICOLOR/icon-theme.cache"
    touch "$HICOLOR" 2>/dev/null || true
}

remove_icons() {
    rm -f "$HICOLOR/scalable/apps/$APP_ID.svg"
    for dir in data/icons/*/; do
        rm -f "$HICOLOR/$(basename "$dir")/apps/$APP_ID.png"
    done
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN/joey" "$BIN/joey-gtk" "$APPS/$APP_ID.desktop"
    remove_icons
    refresh
    echo "Joey removed."
    exit 0
fi

cargo build --release
mkdir -p "$BIN" "$APPS" "$HICOLOR/scalable/apps"
install -m 755 target/release/joey target/release/joey-gtk "$BIN/"
# Full path in Exec, so the menu works even if ~/.local/bin isn't on PATH.
sed "s|^Exec=joey-gtk|Exec=$BIN/joey-gtk|" "data/$APP_ID.desktop" > "$APPS/$APP_ID.desktop"
install -m 644 "data/$APP_ID.svg" "$HICOLOR/scalable/apps/"
for dir in data/icons/*/; do
    size=$(basename "$dir")
    mkdir -p "$HICOLOR/$size/apps"
    install -m 644 "$dir$APP_ID.png" "$HICOLOR/$size/apps/"
done
refresh
echo "Joey installed. If the icon doesn't show yet, log out and back in."
