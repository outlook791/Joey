//! PDF pages to images through Ghostscript: the fallback when Poppler isn't
//! there (the Flatpak ships Ghostscript, which also compresses PDFs).

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

pub struct Ghostscript {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Ghostscript {
    pub fn probe() -> Option<Self> {
        let bin = which("gs")?;
        let edges = vec![
            (Format::Pdf, Format::Png),
            (Format::Pdf, Format::Jpg),
            (Format::Pdf, Format::Tiff),
        ];
        Some(Self { bin, edges })
    }
}

impl Engine for Ghostscript {
    fn name(&self) -> &'static str {
        "ghostscript"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        let ext = step.to.extension();
        let pages_dir = step.work_dir.join(format!("{}.gs", step.stem));
        std::fs::create_dir_all(&pages_dir)?;
        let device = match step.to {
            Format::Png => "png16m",
            Format::Jpg => "jpeg",
            _ => "tiff24nc",
        };
        let mut ranges = o.page_ranges();
        let all = ranges.is_empty();
        if all {
            ranges.push((1, u32::MAX));
        }
        let mut out = Vec::new();
        for (first, last) in ranges {
            let mut cmd = Command::new(&self.bin);
            cmd.args(["-dSAFER", "-dBATCH", "-dNOPAUSE", "-dQUIET"])
                .arg(format!("-sDEVICE={device}"))
                .arg(format!("-r{}", o.dpi()))
                .arg(format!("-dFirstPage={first}"));
            if last != u32::MAX {
                cmd.arg(format!("-dLastPage={last}"));
            }
            if step.to == Format::Jpg {
                cmd.arg(format!("-dJPEGQ={}", o.quality.image()));
            }
            // Ghostscript numbers from 1 within each run; offset to real pages.
            let pattern = pages_dir.join(format!("r{first}-%d.{ext}"));
            cmd.arg(format!("-sOutputFile={}", pattern.display()))
                .arg(step.input);
            run("ghostscript", &mut cmd)?;
            let mut files: Vec<(u32, PathBuf)> = std::fs::read_dir(&pages_dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter_map(|p| {
                    let name = p.file_stem()?.to_str()?.to_string();
                    let rest = name.strip_prefix(&format!("r{first}-"))?;
                    Some((first + rest.parse::<u32>().ok()? - 1, p))
                })
                .collect();
            files.sort();
            out.extend(files);
        }
        let single = out.len() == 1;
        let mut written = Vec::with_capacity(out.len());
        for (page, file) in out {
            let dest = if single {
                step.work_dir.join(format!("{}.{ext}", step.stem))
            } else {
                step.work_dir.join(format!("{}-{page}.{ext}", step.stem))
            };
            std::fs::rename(&file, &dest)?;
            written.push(dest);
        }
        let _ = std::fs::remove_dir_all(&pages_dir);
        if written.is_empty() {
            return Err(Error::Tool {
                tool: "ghostscript",
                message: "no pages to convert (check the page numbers)".into(),
            });
        }
        Ok(written)
    }
}
