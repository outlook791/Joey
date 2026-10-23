# Joey

**Converts files. That's it, really.**

Joey is a free, open-source file converter for Linux. Select files in your file manager, press **Alt+C**, pick a format: done. The result lands next to the original, which is never touched. Everything happens on your computer; nothing is uploaded.

> Status: first complete version (0.1.0). It works end to end; polish comes next.

## What it does

- **Images**: JPG, PNG, WebP, AVIF, GIF, TIFF, BMP, ICO, and HEIC photos from phones; SVG drawings
- **Audio**: MP3, M4A (AAC), OGG, Opus, FLAC, WAV, AIFF, WMA; the sound from any video
- **Video**: MP4, MKV, WebM, MOV, AVI, WMV; video to animated GIF and back
- **PDF**: pages to images, images to PDF; PDF to Word, ODT, PowerPoint, text, Markdown, EPUB; tables to Excel (one sheet per table)
- **Documents**: Word, ODT, RTF, TXT, Markdown, web pages (HTML), EPUB; Excel, ODS, CSV, TSV; PowerPoint, ODP
- **Subtitles**: SRT, WebVTT, ASS, and subtitles to plain text
- **Archives**: ZIP, TAR, TAR.GZ, 7Z, and RAR (opening only)
- **Quick tools**: compress, remove metadata, rotate, black and white, crop to a shape, trim audio/video, even out the volume, remove the sound, split a PDF, keep only some pages, protect or unlock a PDF, extract an archive, join PDFs/tracks/videos, put anything in a ZIP
- **File actions** for any file or folder, even ones Joey can't convert: copy the path, copy to…, move to…, open a terminal here, put in a ZIP
- **Send to your devices**, the free alternative to AirDrop: to phones and PCs paired with **KDE Connect** (GSConnect on GNOME uses the same app), or to anything nearby with **LocalSend** (Android, iPhone, Windows, Mac, Linux)

Select files of different kinds at once: each kind gets its own group, with only the choices that make sense for it. Tools that need one answer (trim, pages, shape, password) ask it on a single small screen.

Not included, on purpose: Apple Pages (a closed format with no free engine; Word, ODT and the rest are the alternatives). PDF to Word and PowerPoint work, but the layout is rebuilt as boxes and may need touching up. PDF to Excel reads PDFs made by a computer, not scans; it uses Camelot if installed, otherwise pdfplumber.

## Using it

1. Select files in your file manager and press **Alt+C** (or right-click → *Joey*).
2. Press a number, or **Enter** for the highlighted choice.

That's it. A few extras:

- Each kind of file starts on the choice you made last time.
- **Adjust** (or **Shift+number**) opens the settings: quality, size, sound, pages… in plain words, with the estimated size updating as you go. *Remember these settings* makes them the default for that format.
- **Esc** closes; the pocket also closes by itself when all went well.
- You can also open Joey from the app menu and drop files on it.

Alt+C works in **Nautilus** (GNOME), **Dolphin** 26.04+ (KDE) and **Thunar** (Xfce). **Nemo** (Cinnamon), **Caja** (MATE), **PCManFM** and **PCManFM-Qt** (LXDE/LXQt) get a right-click entry. With any other file manager, drop files on Joey's window.

## Install

### From source (any distribution)

On Fedora:

```sh
sudo dnf install rust cargo gtk4-devel libadwaita-devel \
    ImageMagick poppler-utils img2pdf librsvg2-tools ffmpeg-free qpdf ghostscript \
    libreoffice-core pandoc bsdtar python3-pdfplumber python3-openpyxl
git clone https://github.com/outlook791/Joey joey && cd joey
./install.sh
```

For sending to devices, pair your phone in KDE Connect and/or install LocalSend (`flatpak install flathub org.localsend.localsend_app`).

`install.sh` builds Joey, adds it to your app menu and sets up the file managers it finds. It installs only in your home folder (no sudo). `./install.sh --uninstall` removes everything.

Joey uses whatever tools are installed and only offers what they can do: without LibreOffice there are no document conversions, without FFmpeg no audio or video. `joey --engines` shows what it found. Fedora's `ffmpeg-free` lacks some codecs (e.g. H.264 MP4 output); the full FFmpeg from RPM Fusion covers them.

To update: `cd ~/joey && git pull && ./install.sh`.

### Flatpak

The manifest is in [`flatpak/`](flatpak/). To build and install it locally:

```sh
sudo dnf install flatpak-builder
flatpak-builder --user --install-deps-from=flathub --force-clean \
    --install build-dir flatpak/io.github.outlook791.Joey.yml
flatpak run io.github.outlook791.Joey --setup-file-managers   # Alt+C and right-click
```

The Flatpak bundles FFmpeg, ImageMagick, Ghostscript, qpdf and libarchive. It does not include LibreOffice or Pandoc (far too big to bundle), so Office documents, Markdown, HTML and EPUB need the source install. "Open a terminal here" is not available inside the Flatpak.

## The command line

```sh
joey photo.heic --to jpg       # one file
joey *.png --to webp           # many files, in parallel
joey scan.pdf --to png         # every page → scan-1.png, scan-2.png…
joey --targets photo.heic      # what can this file become?
joey --engines                 # which tools were found
```

## How it works

1. **Recognises** each file by its content, not its extension: a JPG renamed `.png` is still a JPG.
2. **Finds a route** in a graph of conversions, chaining up to three steps when no tool converts directly (WebP → PNG → PDF, DOCX → PDF → PNG), and offers only conversions a person would want.
3. **Drives proven tools**: ImageMagick, FFmpeg, LibreOffice, Pandoc, Poppler, Ghostscript, img2pdf, librsvg, qpdf, libarchive. If one fails on a step, it tries the next able to do it. Settings apply on the last step only, so intermediate steps lose nothing.

## Project layout

```
crates/joey-core   the engine: detection, conversion graph, settings, tools, tool wrappers
crates/joey-cli    the `joey` command
crates/joey-gtk    the pocket-shaped selector (GTK 4 + libadwaita), Adjust screen, file manager setup
data/              menu entry, icons, store metadata
flatpak/           Flatpak manifest and offline Rust sources
```

Run the tests with `cargo test`. Conversions the installed tools can't do are reported as skipped.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
