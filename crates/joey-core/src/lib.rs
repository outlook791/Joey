//! Joey's conversion engine.
//!
//! Joey recognises what a file really is, finds a route to the format you
//! want and drives proven open-source tools to get there. The result lands
//! next to the original; the original is never touched.
//!
//! ```no_run
//! use joey_core::{Joey, Format};
//! let joey = Joey::new();
//! let written = joey.convert("photo.heic".as_ref(), Format::Jpg, None).unwrap();
//! ```

pub mod engine;
pub mod engines;
pub mod format;
pub mod graph;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub use engine::Engine;
pub use format::{Category, Format};
use graph::Graph;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}: file type not recognised")]
    UnknownFormat(PathBuf),
    #[error("cannot convert {from} to {to} on this computer")]
    NoRoute { from: Format, to: Format },
    #[error("{tool}: {message}")]
    Tool { tool: &'static str, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Files written by one conversion, or why it failed.
pub type Outcome = Result<Vec<PathBuf>, Error>;

pub struct Joey {
    engines: Vec<Box<dyn Engine>>,
    graph: Graph,
}

impl Default for Joey {
    fn default() -> Self {
        Self::new()
    }
}

impl Joey {
    /// Uses every engine whose tool is installed.
    pub fn new() -> Self {
        Self::with_engines(engine::available_engines())
    }

    pub fn with_engines(engines: Vec<Box<dyn Engine>>) -> Self {
        let mut graph = Graph::default();
        for (i, e) in engines.iter().enumerate() {
            for &(from, to) in e.edges() {
                graph.add(from, to, i);
            }
        }
        Self { engines, graph }
    }

    pub fn engine_names(&self) -> Vec<&'static str> {
        self.engines.iter().map(|e| e.name()).collect()
    }

    pub fn detect(&self, path: &Path) -> Result<Format, Error> {
        Format::detect(path).ok_or_else(|| Error::UnknownFormat(path.to_path_buf()))
    }

    /// Formats this kind of file can become here.
    pub fn targets(&self, from: Format) -> Vec<Format> {
        self.graph.targets(from)
    }

    /// The chain of formats a conversion will go through.
    pub fn route(&self, from: Format, to: Format) -> Option<Vec<Format>> {
        self.graph.route(from, to)
    }

    /// Converts one file. Results go into `out_dir`, or next to the input
    /// when `None`. Existing files are never overwritten: a clash becomes
    /// `name (1).ext`. Returns the files written.
    pub fn convert(
        &self,
        input: &Path,
        to: Format,
        out_dir: Option<&Path>,
    ) -> Result<Vec<PathBuf>, Error> {
        let from = self.detect(input)?;
        let route = self
            .graph
            .route(from, to)
            .ok_or(Error::NoRoute { from, to })?;

        let dest_dir = match out_dir {
            Some(d) => d.to_path_buf(),
            None => input
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        };
        std::fs::create_dir_all(&dest_dir)?;
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();

        // Work in a hidden folder inside the destination: moving the result
        // into place is then a rename on the same disk. Removed when done.
        let work = tempfile::Builder::new()
            .prefix(".joey-")
            .tempdir_in(&dest_dir)?;

        let mut current = vec![input.to_path_buf()];
        for (step, hop) in route.windows(2).enumerate() {
            let (a, b) = (hop[0], hop[1]);
            let step_dir = work.path().join(format!("step{step}"));
            std::fs::create_dir_all(&step_dir)?;
            let mut produced = Vec::new();
            for file in &current {
                let file_stem = if step == 0 {
                    stem.clone()
                } else {
                    file.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                        .to_string()
                };
                produced.extend(self.run_step(file, a, b, &step_dir, &file_stem)?);
            }
            current = produced;
        }

        let mut written = Vec::with_capacity(current.len());
        for file in current {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            let dest = unique_path(&dest_dir, name);
            move_file(&file, &dest)?;
            written.push(dest);
        }
        Ok(written)
    }

    /// Converts many files at once, a few in parallel. Results keep the
    /// order of `inputs`.
    pub fn convert_all(
        &self,
        inputs: &[PathBuf],
        to: Format,
        out_dir: Option<&Path>,
    ) -> Vec<Outcome> {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().min(4))
            .unwrap_or(2)
            .min(inputs.len().max(1));
        let next = AtomicUsize::new(0);
        let results: Mutex<Vec<Option<Outcome>>> =
            Mutex::new((0..inputs.len()).map(|_| None).collect());
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(input) = inputs.get(i) else { break };
                    let r = self.convert(input, to, out_dir);
                    results.lock().unwrap()[i] = Some(r);
                });
            }
        });
        results
            .into_inner()
            .unwrap()
            .into_iter()
            .map(|r| r.expect("every job ran"))
            .collect()
    }

    /// Tries each engine able to do this step until one succeeds.
    fn run_step(
        &self,
        input: &Path,
        from: Format,
        to: Format,
        work_dir: &Path,
        stem: &str,
    ) -> Result<Vec<PathBuf>, Error> {
        let mut last_err = Error::NoRoute { from, to };
        for &i in self.graph.engines_for(from, to) {
            match self.engines[i].convert(input, from, to, work_dir, stem) {
                Ok(files) => return Ok(files),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }
}

/// `dir/name`, or `dir/stem (1).ext`, `(2)`… if taken.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("a free name exists")
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to).or_else(|_| {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names_never_clobber() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "a.png"), dir.path().join("a.png"));
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), "a.png"),
            dir.path().join("a (1).png")
        );
        std::fs::write(dir.path().join("a (1).png"), b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), "a.png"),
            dir.path().join("a (2).png")
        );
    }
}
