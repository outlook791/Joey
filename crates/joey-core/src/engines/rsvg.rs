//! SVG drawings through librsvg's `rsvg-convert`.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
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

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let out = step.output();
        let kind = if step.to == Format::Pdf { "pdf" } else { "png" };
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-f", kind, "-o"]).arg(&out);
        if let Some(px) = step.opts().max_size {
            cmd.args(["-w", &px.to_string(), "-a"]);
        }
        cmd.arg(step.input);
        run("rsvg", &mut cmd)?;
        Ok(vec![out])
    }
}
