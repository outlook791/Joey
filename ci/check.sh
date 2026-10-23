#!/bin/sh
# Everything the automatic checks verify on each distribution, after
# ci/deps.sh: build, tests, the command line, the AI server, the window
# and install.sh (in a throwaway home folder).
set -eu
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
step() { printf '\n=== %s ===\n' "$1"; }

step "Versions"
rustc --version
cargo --version

step "Build"
cargo build --release --locked

step "Tests"
cargo test --release --locked --workspace

step "Conversion programs found"
target/release/joey --engines || true

step "Command line"
tmp=$(mktemp -d)
cp crates/joey-core/tests/fixtures/photo.jpg crates/joey-core/tests/fixtures/two-pages.pdf "$tmp/"
target/release/joey --targets "$tmp/photo.jpg"
if target/release/joey --engines | grep -q imagemagick; then
    target/release/joey "$tmp/photo.jpg" --to png
    test -s "$tmp/photo.png"
fi
# A missing file must fail with its error code, not crash.
if target/release/joey /nonexistent.jpg --to png 2>"$tmp/err"; then
    echo "a missing file should fail" >&2
    exit 1
fi
grep -q E101 "$tmp/err"

step "AI server (MCP)"
printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"joey_inspect\",\"arguments\":{\"paths\":[\"$tmp/photo.jpg\"]}}}" |
    target/release/joey --mcp >"$tmp/mcp"
cat "$tmp/mcp"
grep -q '"serverInfo"' "$tmp/mcp"
grep -q joey_convert "$tmp/mcp"
grep -q '"format":"jpg"' "$tmp/mcp"

step "Window starts and stays open"
# timeout ends it after 10 s: exit code 124 means it was still running.
set +e
dbus-run-session -- xvfb-run -a timeout 10 target/release/joey-gtk "$tmp/photo.jpg"
code=$?
set -e
if [ "$code" != 124 ]; then
    echo "joey-gtk exited early with code $code" >&2
    exit 1
fi

step "install.sh, in a throwaway home folder"
home=$(mktemp -d)
mkdir -p "$home/.config"
touch "$home/.config/kglobalshortcutsrc"
HOME=$home XDG_CURRENT_DESKTOP=KDE ./install.sh
for f in .local/bin/joey .local/bin/joey-gtk \
    .local/share/applications/io.github.outlook791.Joey.desktop \
    .local/share/icons/hicolor/scalable/apps/io.github.outlook791.Joey.svg; do
    test -e "$home/$f" || { echo "missing $f" >&2; exit 1; }
done
grep -q "Meta+C" "$home/.config/kglobalshortcutsrc"
HOME=$home ./install.sh --uninstall
test ! -e "$home/.local/bin/joey-gtk"
if grep -q "outlook791" "$home/.config/kglobalshortcutsrc"; then
    echo "uninstall left the shortcut behind" >&2
    exit 1
fi

step "All good"
