#!/bin/sh
# Signs the Flatpak repository built by the checks and lays out the web
# site that serves it (GitHub Pages): the repository, one-click install
# files (.flatpakref) and a small page. Needs the signing key in
# $FLATPAK_GPG_KEY (the repository secret of the same name).
set -eu
URL=https://outlook791.github.io/Joey
KEY=$(cat flatpak/joey-repo.keyid)

export GNUPGHOME="$(mktemp -d)"
printf '%s\n' "$FLATPAK_GPG_KEY" | gpg --batch --import
sign="--gpg-sign=$KEY --gpg-homedir=$GNUPGHOME"

flatpak build-sign $sign repo io.github.outlook791.Joey master
if flatpak repo --branches repo | grep -q 'runtime/io.github.outlook791.Joey.Office/'; then
    flatpak build-sign $sign --runtime repo io.github.outlook791.Joey.Office master
fi
flatpak build-update-repo $sign --prune \
    --title=Joey --homepage=https://github.com/outlook791/Joey repo

rm -rf site
mkdir -p site
cp -a repo site/repo
touch site/.nojekyll
cp data/io.github.outlook791.Joey.svg site/joey.svg
gpgkey=$(base64 -w0 flatpak/joey-repo.gpg)

cat >site/joey.flatpakrepo <<REPO
[Flatpak Repo]
Title=Joey
Url=$URL/repo/
Homepage=https://github.com/outlook791/Joey
Comment=Converts files. That's it, really.
Icon=$URL/joey.svg
GPGKey=$gpgkey
REPO

ref() { # id, title, is-runtime
    cat <<REF
[Flatpak Ref]
Name=$1
Branch=master
Title=$2
Url=$URL/repo/
SuggestRemoteName=joey
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=$3
Homepage=https://github.com/outlook791/Joey
Icon=$URL/joey.svg
GPGKey=$gpgkey
REF
}
ref io.github.outlook791.Joey Joey false >site/joey.flatpakref
ref io.github.outlook791.Joey.Office "Joey Office" true >site/joey-office.flatpakref

cp ci/site/index.html site/index.html
echo "Site ready: $(du -sh site | cut -f1)"
