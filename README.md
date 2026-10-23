# Joey

**Converts files. That's it, really.**

Joey is a free, open-source file converter for Linux. Select files, press a key, pick a format: done. Everything happens on your computer; nothing is uploaded.

> Status: early development, phase 2 of 5 — the engine and the pocket-shaped selector work. The Alt+C shortcut in your file manager and the Flatpak come next.

## Try it

You need Rust, GTK 4, libadwaita and the tools Joey drives. On Fedora:

```sh
sudo dnf install rust cargo gtk4-devel libadwaita-devel ImageMagick poppler-utils img2pdf librsvg2-tools
git clone https://github.com/outlook791/joey && cd joey
cargo build --release
```

### The window

```sh
./target/release/joey-gtk ~/Pictures/*.jpg     # open with files
./target/release/joey-gtk                      # empty: drop files on it
```

- Numbers pick a format, **Enter** converts, **Esc** closes.
- One kind of file: the number converts right away.
- Mixed files get a group each; every group starts on the format you chose last time for that kind of file.
- Sizes are rough estimates (`~`). When everything went well, the pocket closes by itself.

### The command line

```sh
./target/release/joey photo.heic --to jpg       # one file
./target/release/joey *.png --to webp           # many files, in parallel
./target/release/joey scan.pdf --to png         # every page → scan-1.png, scan-2.png…
./target/release/joey --targets photo.heic      # what can this file become?
./target/release/joey --engines                 # which tools were found
```

Results land next to the original, which is never touched. If a file with the same name exists, Joey writes `name (1).ext` instead of overwriting.

## How it works

1. **Recognises** each file by its content, not its extension — a JPG renamed `.png` is still treated as a JPG.
2. **Finds a route** in a graph of conversions. If no tool converts directly, it chains up to three steps (WebP → PNG → PDF).
3. **Drives proven tools** — ImageMagick, Poppler, img2pdf, librsvg — and if one fails on a step, tries the next able to do it.

Only conversions the installed tools can really do are offered: if ImageMagick on your system can't write HEIC, Joey won't pretend it can.

## Formats in this phase

| From | To |
| --- | --- |
| JPG, PNG, WebP, GIF, TIFF, BMP, ICO | each other, PDF |
| HEIC, AVIF | the above (reading, where ImageMagick supports it) |
| SVG | PNG, PDF (with librsvg) |
| PDF | PNG, JPG, TIFF, and on to the other image formats |

Audio, video, Office documents and more arrive in phase 4.

## Project layout

```
crates/joey-core   the engine: format detection, conversion graph, tool wrappers
crates/joey-cli    the `joey` command
crates/joey-gtk    the pocket-shaped selector (GTK 4 + libadwaita)
```

Run the tests with `cargo test`. Conversions the installed tools can't do are reported as skipped.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
