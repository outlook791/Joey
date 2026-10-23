//! Images to PDF through `img2pdf`: embeds JPG and others without
//! re-compressing, so quality stays exactly the same.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{output_path, run, which, Engine};
use crate::format::Format;
use crate::Error;

pub struct Img2Pdf {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Img2Pdf {
    pub fn probe() -> Option<Self> {
        let bin = which("img2pdf")?;
        let edges = vec![
            (Format::Jpg, Format::Pdf),
            (Format::Png, Format::Pdf),
            (Format::Tiff, Format::Pdf),
        ];
        Some(Self { bin, edges })
    }
}

impl Engine for Img2Pdf {
    fn name(&self) -> &'static str {
        "img2pdf"
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
        let out = output_path(work_dir, stem, to);
        run(
            "img2pdf",
            Command::new(&self.bin).arg(input).arg("-o").arg(&out),
        )?;
        Ok(vec![out])
    }
}
