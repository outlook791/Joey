//! Raster images through ImageMagick (`magick` 7 or `convert` 6).
//! Supported formats are read from the installed build, so HEIC or AVIF
//! appear only when ImageMagick can really handle them.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
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
                // PDF input needs Ghostscript, so it's left to Poppler.
                let ok = matches!(
                    (from.category(), to.category()),
                    (Category::Image, Category::Image)
                        | (Category::Vector, Category::Image)
                        | (Category::Image, Category::Pdf)
                );
                if ok {
                    edges.push((from, to));
                }
            }
        }
        Some(Self { bin, edges })
    }

    pub fn bin(&self) -> &Path {
        &self.bin
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
        if !matches!(
            format.category(),
            Category::Image | Category::Vector | Category::Pdf
        ) {
            continue;
        }
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

/// ImageMagick arguments for writing `to` with the step's settings.
pub(crate) fn output_args(cmd: &mut Command, from: Format, to: Format, step: &Step) {
    let o = step.opts();
    cmd.arg("-auto-orient");
    if o.rotate % 360 != 0 {
        cmd.args(["-rotate", &(o.rotate % 360).to_string()]);
    }
    if let Some((w, h)) = o.crop {
        cmd.args([
            "-gravity",
            "center",
            "-crop",
            &format!("{w}x{h}+0+0"),
            "+repage",
        ]);
    }
    if o.grayscale {
        cmd.args(["-colorspace", "Gray"]);
    }
    if let Some(px) = o.max_size {
        cmd.args(["-resize", &format!("{px}x{px}>")]);
    }
    if matches!(to, Format::Jpg | Format::Bmp) {
        // No transparency in these formats: put it on white.
        cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
    }
    if to == Format::Ico {
        cmd.args(["-resize", "256x256>"]);
    }
    if matches!(to, Format::Jpg | Format::Webp | Format::Avif | Format::Heic) {
        cmd.args(["-quality", &o.quality.image().to_string()]);
    }
    if o.strip_metadata {
        cmd.arg("-strip");
    }
    let _ = from;
}

impl Engine for ImageMagick {
    fn name(&self) -> &'static str {
        "imagemagick"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let out = step.output();
        let keeps_frames = matches!(
            step.to,
            Format::Gif | Format::Webp | Format::Tiff | Format::Pdf
        );

        // An animated GIF to a still format: take the first frame only.
        let mut src = step.input.as_os_str().to_owned();
        if step.from == Format::Gif && !keeps_frames {
            src.push("[0]");
        }

        let mut cmd = Command::new(&self.bin);
        if step.from == Format::Svg {
            cmd.args(["-background", "none", "-density", "192"]);
        }
        cmd.arg(&src);
        output_args(&mut cmd, step.from, step.to, step);
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
                         MP4* VIDEO     rw+   MPEG-4 Video Stream\n\
                         PNG* PNG       rw-   Portable Network Graphics\n";
        let (r, w) = parse_format_list(list);
        assert!(r.contains(&Format::Heic) && !w.contains(&Format::Heic));
        assert!(r.contains(&Format::Jpg) && w.contains(&Format::Jpg));
        assert!(w.contains(&Format::Png));
        assert!(!r.contains(&Format::Mp4), "video is FFmpeg's job");
    }
}
