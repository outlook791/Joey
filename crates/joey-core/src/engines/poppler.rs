//! PDF pages to images through Poppler's `pdftoppm`.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

pub struct Poppler {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Poppler {
    pub fn probe() -> Option<Self> {
        let bin = which("pdftoppm")?;
        let edges = vec![
            (Format::Pdf, Format::Png),
            (Format::Pdf, Format::Jpg),
            (Format::Pdf, Format::Tiff),
        ];
        Some(Self { bin, edges })
    }
}

impl Engine for Poppler {
    fn name(&self) -> &'static str {
        "poppler"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        // Render into a private sub-folder so we can collect every page.
        let pages_dir = step.work_dir.join(format!("{}.pages", step.stem));
        std::fs::create_dir_all(&pages_dir)?;
        let flag = match step.to {
            Format::Png => "-png",
            Format::Jpg => "-jpeg",
            _ => "-tiff",
        };
        let mut ranges = o.page_ranges();
        if ranges.is_empty() {
            ranges.push((0, 0)); // all pages
        }
        for (first, last) in ranges {
            let mut cmd = Command::new(&self.bin);
            cmd.args([flag, "-r", &o.dpi().to_string()]);
            if step.to == Format::Jpg {
                cmd.args(["-jpegopt", &format!("quality={}", o.quality.image())]);
            }
            if let Some(px) = o.max_size {
                cmd.args(["-scale-to", &px.to_string()]);
            }
            if first > 0 {
                cmd.args(["-f", &first.to_string(), "-l", &last.to_string()]);
            }
            cmd.arg(step.input).arg(pages_dir.join("page"));
            run("poppler", &mut cmd)?;
        }

        let mut pages: Vec<PathBuf> = std::fs::read_dir(&pages_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        // Order by page number (pdftoppm pads differently per range).
        pages.sort_by_key(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.rsplit('-').next())
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        });
        pages.dedup();

        let ext = step.to.extension();
        let mut out = Vec::with_capacity(pages.len());
        if pages.len() == 1 {
            let dest = step.work_dir.join(format!("{}.{ext}", step.stem));
            std::fs::rename(&pages[0], &dest)?;
            out.push(dest);
        } else {
            for page in &pages {
                let n = page
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.rsplit('-').next())
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(0);
                let dest = step.work_dir.join(format!("{}-{n}.{ext}", step.stem));
                std::fs::rename(page, &dest)?;
                out.push(dest);
            }
        }
        let _ = std::fs::remove_dir_all(&pages_dir);
        if out.is_empty() {
            return Err(Error::Tool {
                tool: "poppler",
                message: "no pages to convert (check the page numbers)".into(),
            });
        }
        Ok(out)
    }
}

/// PDF text through Poppler's `pdftotext`, in reading order.
pub struct PdfText {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl PdfText {
    pub fn probe() -> Option<Self> {
        let bin = which("pdftotext")?;
        Some(Self {
            bin,
            edges: vec![(Format::Pdf, Format::Txt)],
        })
    }
}

impl Engine for PdfText {
    fn name(&self) -> &'static str {
        "pdftotext"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        let out = step.output();
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-enc", "UTF-8"]);
        // pdftotext takes one range: the first one chosen.
        if let Some(&(first, last)) = o.page_ranges().first() {
            cmd.args(["-f", &first.to_string(), "-l", &last.to_string()]);
        }
        cmd.arg(step.input).arg(&out);
        run("pdftotext", &mut cmd)?;
        Ok(vec![out])
    }
}
