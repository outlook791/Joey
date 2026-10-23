//! Engines wrap proven open-source tools. Each one says which direct
//! conversions it can do on this machine, and does them in a work folder.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::format::Format;
use crate::Error;

pub trait Engine: Send + Sync {
    /// Short name, e.g. "imagemagick".
    fn name(&self) -> &'static str;

    /// Direct conversions this engine can do here, as (from, to) pairs.
    fn edges(&self) -> &[(Format, Format)];

    /// Converts `input` into `to`, writing result file(s) into `work_dir`
    /// named after `stem`. Returns every file written (a multi-page PDF
    /// becomes several images).
    fn convert(
        &self,
        input: &Path,
        from: Format,
        to: Format,
        work_dir: &Path,
        stem: &str,
    ) -> Result<Vec<PathBuf>, Error>;
}

/// Finds an executable on PATH.
pub(crate) fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|p| p.is_file())
}

/// Runs a command; on failure returns its stderr as the error message.
pub(crate) fn run(tool: &'static str, cmd: &mut Command) -> Result<String, Error> {
    let out = cmd.output().map_err(|e| Error::Tool {
        tool,
        message: e.to_string(),
    })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let mut message = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if message.is_empty() {
            message = format!("exited with {}", out.status);
        }
        Err(Error::Tool { tool, message })
    }
}

pub(crate) fn output_path(work_dir: &Path, stem: &str, to: Format) -> PathBuf {
    work_dir.join(format!("{stem}.{}", to.extension()))
}

/// Every engine whose tool is installed, in order of preference.
pub fn available_engines() -> Vec<Box<dyn Engine>> {
    let mut engines: Vec<Box<dyn Engine>> = Vec::new();
    if let Some(e) = crate::engines::rsvg::Rsvg::probe() {
        engines.push(Box::new(e));
    }
    if let Some(e) = crate::engines::poppler::Poppler::probe() {
        engines.push(Box::new(e));
    }
    if let Some(e) = crate::engines::img2pdf::Img2Pdf::probe() {
        engines.push(Box::new(e));
    }
    if let Some(e) = crate::engines::imagemagick::ImageMagick::probe() {
        engines.push(Box::new(e));
    }
    engines
}
