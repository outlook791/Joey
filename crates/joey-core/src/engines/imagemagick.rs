//! Raster images through ImageMagick (`magick` 7 or `convert` 6).
//! Supported formats are read from the installed build, so HEIC or AVIF
//! appear only when ImageMagick can really handle them.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{output_path, run, which, Engine};
use crate::format::{Category, Format};
use crate::Error;

pub struct ImageMagick {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl ImageMagick {
    pub fn probe() -> Option<Self> {
        let bin = which("magick").or_else(|| which("convert"))?;
        let list = run("imagemagick", Command::new(&bin).args(["-list", "format"])).ok()?;
        let (readable, writable) = parse_format_list(&list);

        let mut edges = Vec::new();
        for &from in &readable {
            for &to in &writable {
                if from == to {
                    continue;
                }
                // Images to PDF; PDF input needs Ghostscript, so left to Poppler.
                let ok = matches!(
                    (from.category(), to.category()),
                    (Category::Image, Category::Image)
                        | (Category::Vector, Category::Image)
                        | (Category::Image, Category::Document)
                );
                if ok {
                    edges.push((from, to));
                }
            }
        }
        Some(Self { bin, edges })
    }
}

/// Parses `-list format` lines like `  WEBP* WEBP  rw+  WebP Image Format`.
fn parse_format_list(list: &str) -> (Vec<Format>, Vec<Format>) {
    let mut readable = Vec::new();
    let mut writable = Vec::new();
    for line in list.lines() {
        let mut tokens = line.split_whitespace();
        let Some(name) = tokens.next() else { continue };
        let Some(format) = Format::parse(name.trim_end_matches('*')) else {
            continue;
        };
        let Some(mode) = tokens.find(|t| t.len() == 3 && t.chars().all(|c| "rw+-".contains(c)))
        else {
            continue;
        };
        let mode = mode.as_bytes();
        if mode[0] == b'r' && !readable.contains(&format) {
            readable.push(format);
        }
        if mode[1] == b'w' && !writable.contains(&format) {
            writable.push(format);
        }
    }
    (readable, writable)
}

impl Engine for ImageMagick {
    fn name(&self) -> &'static str {
        "imagemagick"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(
        &self,
        input: &Path,
        from: Format,
        to: Format,
        work_dir: &Path,
        stem: &str,
    ) -> Result<Vec<PathBuf>, Error> {
        let out = output_path(work_dir, stem, to);
        let keeps_frames = matches!(to, Format::Gif | Format::Webp | Format::Tiff | Format::Pdf);

        // An animated GIF to a still format: take the first frame only.
        let mut src = input.as_os_str().to_owned();
        if from == Format::Gif && !keeps_frames {
            src.push("[0]");
        }

        let mut cmd = Command::new(&self.bin);
        if from == Format::Svg {
            cmd.args(["-background", "none", "-density", "192"]);
        }
        cmd.arg(&src).arg("-auto-orient");
        if matches!(to, Format::Jpg | Format::Bmp) {
            // No transparency in these formats: put it on white.
            cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
        }
        if to == Format::Ico {
            cmd.args(["-resize", "256x256>"]);
        }
        if to == Format::Jpg {
            cmd.args(["-quality", "90"]);
        }
        cmd.arg(&out);
        run("imagemagick", &mut cmd)?;
        Ok(vec![out])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_im6_list() {
        let list = "   Format  Module    Mode  Description\n\
                    -------------------------------------\n\
                         AVIF            r--   AV1 Image File Format (1.17.6)\n\
                         HEIC            r--   Apple High efficiency Image Format\n\
                        JPEG* JPEG      rw-   Joint Photographic Experts Group\n\
                         PNG* PNG       rw-   Portable Network Graphics\n";
        let (r, w) = parse_format_list(list);
        assert!(r.contains(&Format::Heic) && !w.contains(&Format::Heic));
        assert!(r.contains(&Format::Jpg) && w.contains(&Format::Jpg));
        assert!(w.contains(&Format::Png));
    }
}
