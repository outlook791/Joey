#!/bin/sh
# Makes a new release numbered by date: YEAR.MONTH.N, where N counts the
# releases of that month (26.10.1, 26.10.2, then 26.11.1…). It sets the
# version everywhere and adds it to the store metadata, then commits.
# Pushing it is enough: GitHub creates the tag and the release page by
# itself (.github/workflows/release.yml), and the checks publish it.
#
#   scripts/release.sh "What changed, in one line"
#
# Without a terminal: GitHub → Actions → Release → Run workflow.
set -eu
cd "$(dirname "$0")/.."

note=${1:?"Usage: scripts/release.sh \"What changed\""}
yy=$(date +%y); mm=$(date +%-m); today=$(date +%F)
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
case "$current" in
    "$yy.$mm."*) version="$yy.$mm.$(( ${current##*.} + 1 ))" ;;
    *) version="$yy.$mm.1" ;;
esac

if [ -n "$(git status --porcelain)" ]; then
    echo "Commit or stash your changes first." >&2
    exit 1
fi

# Cargo (the workspace version is the only one).
sed -i "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null

# Store metadata: newest release first.
python3 - "$version" "$today" "$note" <<'PY'
import sys, html
v, d, note = sys.argv[1:]
p = "data/io.github.outlook791.Joey.metainfo.xml"
s = open(p).read()
entry = (f'    <release version="{v}" date="{d}">\n      <description>\n'
         f'        <p>{html.escape(note)}</p>\n      </description>\n    </release>\n')
s = s.replace("  <releases>\n", "  <releases>\n" + entry, 1)
open(p, "w").write(s)
PY

for f in README.md README.it.md; do
    sed -i -E "s/\(([0-9]+\.[0-9]+\.[0-9]+)\)/($version)/" "$f"
done

git commit -qam "Release $version" -m "$note"
echo "Release $version ready. Push it: git push"
