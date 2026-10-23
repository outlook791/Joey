//! Images to PDF through `img2pdf`: embeds JPG and others without
//! re-compressing, so quality stays exactly the same.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
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

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        // Embedding untouched means settings can't apply: let ImageMagick
        // take the job when the user asked for a smaller or lighter file.
        let o = step.opts();
        if o.max_size.is_some() || (step.last && o.quality != crate::options::Quality::High) {
            return Err(Error::Tool {
                tool: "img2pdf",
                message: "settings need re-encoding".into(),
            });
        }
        let out = step.output();
        run(
            "img2pdf",
            Command::new(&self.bin).arg(step.input).arg("-o").arg(&out),
        )?;
        Ok(vec![out])
    }
}
