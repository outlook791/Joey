//! Engines wrap proven open-source tools. Each one says which direct
//! conversions it can do on this machine, and does them in a work folder.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::format::Format;
use crate::options::Options;
use crate::Error;

/// One direct conversion for an engine to carry out.
pub struct Step<'a> {
    pub input: &'a Path,
    pub from: Format,
    pub to: Format,
    /// Where to write the result(s).
    pub work_dir: &'a Path,
    /// Base name for the result(s), without extension.
    pub stem: &'a str,
    pub options: &'a Options,
    /// True for the last step of a route: settings like quality apply
    /// here; intermediate steps keep everything at full quality.
    pub last: bool,
}

impl Step<'_> {
    pub fn output(&self) -> PathBuf {
        self.work_dir
            .join(format!("{}.{}", self.stem, self.to.extension()))
    }

    /// Settings that apply to this step: the user's on the last step,
    /// lossless-ish defaults in between.
    pub fn opts(&self) -> Options {
        if self.last {
            self.options.clone()
        } else {
            Options {
                quality: crate::options::Quality::Max,
                pages: self.options.pages.clone(),
                dpi: self.options.dpi,
                ..Default::default()
            }
        }
    }
}

pub trait Engine: Send + Sync {
    /// Short name, e.g. "imagemagick".
    fn name(&self) -> &'static str;

    /// Direct conversions this engine can do here, as (from, to) pairs.
    fn edges(&self) -> &[(Format, Format)];

    /// Carries out the step and returns every file written (a multi-page
    /// PDF becomes several images).
    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error>;
}

/// Finds an executable on PATH.
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|p| p.is_file())
}

/// Runs a command; on failure returns the end of its stderr as the error.
pub fn run(tool: &'static str, cmd: &mut Command) -> Result<String, Error> {
    let out = cmd.output().map_err(|e| Error::Tool {
        tool,
        message: e.to_string(),
    })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let mut message: String = err
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        if message.is_empty() {
            message = format!("exited with {}", out.status);
        }
        Err(Error::Tool { tool, message })
    }
}

/// Every engine whose tool is installed, in order of preference.
pub fn available_engines() -> Vec<Box<dyn Engine>> {
    use crate::engines::*;
    let mut engines: Vec<Box<dyn Engine>> = Vec::new();
    macro_rules! add {
        ($e:expr) => {
            if let Some(e) = $e {
                engines.push(Box::new(e));
            }
        };
    }
    add!(rsvg::Rsvg::probe());
    add!(poppler::Poppler::probe());
    add!(ghostscript::Ghostscript::probe());
    add!(img2pdf::Img2Pdf::probe());
    add!(imagemagick::ImageMagick::probe());
    add!(ffmpeg::Ffmpeg::probe());
    add!(libreoffice::LibreOffice::probe());
    engines
}
