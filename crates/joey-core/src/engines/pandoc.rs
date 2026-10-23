//! Text documents through Pandoc: Markdown, web pages (HTML), EPUB e-books,
//! and Word/ODT/RTF/TXT when LibreOffice isn't there; with Typst, all of
//! them to PDF too.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

const INPUTS: [Format; 7] = [
    Format::Docx,
    Format::Odt,
    Format::Md,
    Format::Html,
    Format::Epub,
    Format::Txt,
    Format::Rtf,
];
const OUTPUTS: [Format; 7] = INPUTS;

pub struct Pandoc {
    bin: PathBuf,
    /// Typst lays documents out as PDF when LibreOffice isn't there.
    typst: Option<PathBuf>,
    edges: Vec<(Format, Format)>,
}

impl Pandoc {
    pub fn probe() -> Option<Self> {
        let bin = which("pandoc")?;
        let mut edges = Vec::new();
        for from in INPUTS {
            for to in OUTPUTS {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        let typst = which("typst");
        if typst.is_some() {
            edges.extend(INPUTS.iter().map(|&f| (f, Format::Pdf)));
        }
        Some(Self { bin, typst, edges })
    }
}

fn reader(f: Format) -> &'static str {
    match f {
        Format::Docx => "docx",
        Format::Odt => "odt",
        Format::Html => "html",
        Format::Epub => "epub",
        Format::Rtf => "rtf",
        // Plain text reads fine as Markdown.
        _ => "markdown",
    }
}

fn writer(f: Format) -> &'static str {
    match f {
        Format::Docx => "docx",
        Format::Odt => "odt",
        Format::Html => "html",
        Format::Epub => "epub",
        Format::Rtf => "rtf",
        Format::Txt => "plain",
        Format::Pdf => "typst",
        _ => "gfm",
    }
}

impl Engine for Pandoc {
    fn name(&self) -> &'static str {
        "pandoc"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let out = step.output();
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-f", reader(step.from), "-t", writer(step.to), "-o"])
            .arg(&out);
        match step.to {
            Format::Html => {
                cmd.args(["--standalone", "--embed-resources"]);
                cmd.arg(format!("--metadata=pagetitle:{}", step.stem));
            }
            Format::Epub => {
                cmd.arg(format!("--metadata=title:{}", step.stem));
            }
            Format::Rtf => {
                cmd.arg("--standalone");
            }
            Format::Pdf => {
                if let Some(typst) = &self.typst {
                    cmd.arg(format!("--pdf-engine={}", typst.display()));
                }
            }
            _ => {}
        }
        // Images referenced by the document are found next to it.
        if let Some(dir) = step.input.parent() {
            cmd.arg(format!("--resource-path={}", dir.display()));
        }
        cmd.arg(step.input);
        run("pandoc", &mut cmd)?;
        Ok(vec![out])
    }
}
