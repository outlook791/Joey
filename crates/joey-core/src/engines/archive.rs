//! Archives through libarchive's `bsdtar`: reads ZIP, TAR, TAR.GZ, 7Z and
//! RAR; writes all of them except RAR (a closed format).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

const READ: [Format; 5] = [
    Format::Zip,
    Format::Tar,
    Format::Tgz,
    Format::SevenZ,
    Format::Rar,
];
const WRITE: [Format; 4] = [Format::Zip, Format::Tar, Format::Tgz, Format::SevenZ];

pub struct Archive {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Archive {
    pub fn probe() -> Option<Self> {
        let bin = which("bsdtar")?;
        let mut edges = Vec::new();
        for from in READ {
            for to in WRITE {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        Some(Self { bin, edges })
    }

    /// Unpacks an archive into `dest` (created if needed).
    pub fn extract(&self, archive: &Path, dest: &Path) -> Result<(), Error> {
        std::fs::create_dir_all(dest)?;
        run(
            "bsdtar",
            Command::new(&self.bin)
                .arg("-xf")
                .arg(archive)
                .arg("-C")
                .arg(dest),
        )?;
        Ok(())
    }

    /// Packs `entries` (names inside `base`) into a new archive.
    pub fn create(
        &self,
        out: &Path,
        to: Format,
        base: &Path,
        entries: &[String],
    ) -> Result<(), Error> {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("-c");
        match to {
            Format::Zip => cmd.args(["--format", "zip"]),
            Format::SevenZ => cmd.args(["--format", "7zip"]),
            Format::Tgz => cmd.args(["-z", "--format", "pax"]),
            _ => cmd.args(["--format", "pax"]),
        };
        cmd.arg("-f")
            .arg(out)
            .arg("-C")
            .arg(base)
            .arg("--")
            .args(entries);
        run("bsdtar", &mut cmd)?;
        Ok(())
    }
}

impl Engine for Archive {
    fn name(&self) -> &'static str {
        "archive"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let tmp = step.work_dir.join(format!("{}.unpacked", step.stem));
        self.extract(step.input, &tmp)?;
        let mut entries: Vec<String> = std::fs::read_dir(&tmp)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        if entries.is_empty() {
            return Err(Error::Tool {
                tool: "joey",
                message: "the archive is empty".into(),
            });
        }
        let out = step.output();
        self.create(&out, step.to, &tmp, &entries)?;
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(vec![out])
    }
}
