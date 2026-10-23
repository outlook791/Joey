//! SVG drawings through librsvg's `rsvg-convert`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{output_path, run, which, Engine};
use crate::format::Format;
use crate::Error;

pub struct Rsvg {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Rsvg {
    pub fn probe() -> Option<Self> {
        let bin = which("rsvg-convert")?;
        let edges = vec![(Format::Svg, Format::Png), (Format::Svg, Format::Pdf)];
        Some(Self { bin, edges })
    }
}

impl Engine for Rsvg {
    fn name(&self) -> &'static str {
        "rsvg"
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
        let kind = if to == Format::Pdf { "pdf" } else { "png" };
        run(
            "rsvg",
            Command::new(&self.bin)
                .args(["-f", kind, "-o"])
                .arg(&out)
                .arg(input),
        )?;
        Ok(vec![out])
    }
}
