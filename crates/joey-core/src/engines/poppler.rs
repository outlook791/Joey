//! PDF pages to images through Poppler's `pdftoppm`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine};
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

    fn convert(
        &self,
        input: &Path,
        _from: Format,
        to: Format,
        work_dir: &Path,
        stem: &str,
    ) -> Result<Vec<PathBuf>, Error> {
        // Render into a private sub-folder so we can collect every page.
        let pages_dir = work_dir.join(format!("{stem}.pages"));
        std::fs::create_dir_all(&pages_dir)?;
        let flag = match to {
            Format::Png => "-png",
            Format::Jpg => "-jpeg",
            _ => "-tiff",
        };
        run(
            "poppler",
            Command::new(&self.bin)
                .args([flag, "-r", "150"])
                .arg(input)
                .arg(pages_dir.join("page")),
        )?;

        let mut pages: Vec<PathBuf> = std::fs::read_dir(&pages_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        // pdftoppm zero-pads page numbers, so name order is page order.
        pages.sort();

        let ext = to.extension();
        let mut out = Vec::with_capacity(pages.len());
        if pages.len() == 1 {
            let dest = work_dir.join(format!("{stem}.{ext}"));
            std::fs::rename(&pages[0], &dest)?;
            out.push(dest);
        } else {
            for (i, page) in pages.iter().enumerate() {
                let dest = work_dir.join(format!("{stem}-{}.{ext}", i + 1));
                std::fs::rename(page, &dest)?;
                out.push(dest);
            }
        }
        let _ = std::fs::remove_dir(&pages_dir);
        if out.is_empty() {
            return Err(Error::Tool {
                tool: "poppler",
                message: "the PDF has no pages".into(),
            });
        }
        Ok(out)
    }
}
