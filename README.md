# Joey

**Converts files. That's it, really.**

Joey is a free, open-source file converter for Linux. Select files in your file manager, press **Alt+C**, pick a format: done. The result lands next to the original, which is never touched. Everything happens on your computer; nothing is uploaded.

> Status: first complete version (0.1.0). It works end to end; polish comes next.

## What it does

- **Images**: JPG, PNG, WebP, AVIF, GIF, TIFF, BMP, ICO, and HEIC photos from phones
- **Drawings**: SVG to PNG or PDF
- **Audio**: MP3, M4A (AAC), OGG, Opus, FLAC, WAV, AIFF; the sound from any video
- **Video**: MP4, MKV, WebM, MOV, AVI; video to animated GIF and back
- **PDF**: pages to images (one per page, or the pages you choose), images to PDF
- **Documents** (install.sh build): Word, ODT, RTF, TXT, Excel, ODS, CSV, PowerPoint, ODP, to each other and to PDF or images
- **Quick tools**: compress, remove metadata, merge PDFs, join audio tracks, even out the volume, remove a video's sound

Select files of different kinds at once: each kind gets its own group, with only the formats that make sense for it.

## Using it

1. Select files in your file manager and press **Alt+C** (or right-click → *Joey*).
2. Press a number, or **Enter** for the highlighted choice.

That's it. A few extras:

- Each kind of file starts on the choice you made last time.
- **Adjust** (or **Shift+number**) opens the settings: quality, size, sound, pages… in plain words, with the estimated size updating as you go. *Remember these settings* makes them the default for that format.
- **Esc** closes; the pocket also closes by itself when all went well.
- You can also open Joey from the app menu and drop files on it.

Alt+C works in **Nautilus** (GNOME Files) and **Thunar**. In **Dolphin**, **Nemo** and **Caja** there's a right-click entry; Dolphin 26.04 and later let you give it a shortcut in *Configure Keyboard Shortcuts*.

## Install

### From source (any distribution)

On Fedora:

```sh
sudo dnf install rust cargo gtk4-devel libadwaita-devel \
    ImageMagick poppler-utils img2pdf librsvg2-tools ffmpeg-free qpdf ghostscript libreoffice-core
git clone https://github.com/outlook791/Joey joey && cd joey
./install.sh
```

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

The Flatpak bundles FFmpeg, ImageMagick, Ghostscript and qpdf. It does not include LibreOffice (far too big to bundle), so documents need the source install.

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
3. **Drives proven tools**: ImageMagick, FFmpeg, LibreOffice, Poppler, Ghostscript, img2pdf, librsvg, qpdf. If one fails on a step, it tries the next able to do it. Settings apply on the last step only, so intermediate steps lose nothing.

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
