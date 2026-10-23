#!/bin/sh
# Installs what Joey needs on each distribution, for the automatic checks.
# Build tools are required; conversion programs are installed one by one,
# so a package missing from one distribution doesn't stop the others.
#   ci/deps.sh fedora|ubuntu|debian|arch|opensuse
set -u
distro=$1
missing=""

optional() {
    for p in "$@"; do
        case $distro in
            fedora) dnf -y -q install "$p" >/dev/null 2>&1 ;;
            ubuntu|debian) apt-get install -y -qq "$p" >/dev/null 2>&1 ;;
            arch) pacman -S --noconfirm --needed "$p" >/dev/null 2>&1 ;;
            opensuse) zypper -n -q in "$p" >/dev/null 2>&1 ;;
        esac || missing="$missing $p"
    done
}

set -e
case $distro in
fedora)
    dnf -y -q install rust cargo gcc pkgconf-pkg-config gtk4-devel libadwaita-devel \
        git python3 xorg-x11-server-Xvfb dbus-daemon dbus-tools desktop-file-utils appstream
    set +e
    optional ImageMagick poppler-utils img2pdf librsvg2-tools ffmpeg-free qpdf ghostscript \
        libreoffice-writer libreoffice-calc libreoffice-impress libreoffice-pdfimport \
        pandoc bsdtar python3-pdfplumber python3-openpyxl perl-Image-ExifTool xvfb-run
    ;;
ubuntu|debian)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq build-essential pkg-config curl git ca-certificates python3 \
        libgtk-4-dev libadwaita-1-dev xvfb xauth dbus dbus-x11 desktop-file-utils appstream
    # Their own Rust is too old for GTK's Rust bindings (1.92 or newer).
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    set +e
    optional imagemagick poppler-utils img2pdf librsvg2-bin ffmpeg qpdf ghostscript \
        libreoffice-writer-nogui libreoffice-calc-nogui libreoffice-impress-nogui \
        pandoc libarchive-tools python3-pdfplumber python3-openpyxl libimage-exiftool-perl
    ;;
arch)
    pacman -Syu --noconfirm --needed base-devel rust gtk4 libadwaita git python \
        xorg-server-xvfb dbus desktop-file-utils appstream
    set +e
    optional imagemagick poppler img2pdf librsvg ffmpeg qpdf ghostscript libreoffice-fresh \
        pandoc-cli libarchive python-pdfplumber python-openpyxl perl-image-exiftool
    ;;
opensuse)
    zypper -n -q ref
    # The container's busybox awk blocks the real one some tools need.
    zypper -n -q in --force-resolution gawk
    zypper -n -q in --force-resolution rust cargo gcc pkgconf gtk4-devel libadwaita-devel git python3 \
        xvfb-run dbus-1 desktop-file-utils AppStream
    set +e
    optional ImageMagick poppler-tools python3-img2pdf rsvg-convert ffmpeg-7 qpdf ghostscript \
        libreoffice libreoffice-writer libreoffice-calc libreoffice-impress pandoc-cli bsdtar python3-pdfplumber python3-openpyxl exiftool
    ;;
*)
    echo "unknown distribution $distro" >&2
    exit 2
    ;;
esac

if [ -n "$missing" ]; then
    echo "Not available here (Joey will skip what needs them):$missing"
    echo "::notice title=Packages not available on $distro::$missing"
fi
