#!/bin/sh
# Makes a new release numbered by date: YEAR.MONTH.N, where N counts the
# releases of that month (26.10.1, 26.10.2, then 26.11.1…). It sets the
# version everywhere, adds it to the store metadata, commits and tags.
#
#   scripts/release.sh "What changed, in one line"
set -eu
cd "$(dirname "$0")/.."

note=${1:?"Usage: scripts/release.sh \"What changed\""}
yy=$(date +%y); mm=$(date +%-m); today=$(date +%F)
n=$(git tag --list "v$yy.$mm.*" | wc -l)
version="$yy.$mm.$((n + 1))"

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

cargo build -q
git commit -qam "Release $version" -m "$note"
git tag -a "v$version" -m "$note"
echo "Release $version ready. Publish it with: git push --follow-tags"
