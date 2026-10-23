//! Office documents through LibreOffice, run without a window.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::{Category, Format};
use crate::Error;

pub struct LibreOffice {
    bin: PathBuf,
    edges: Vec<(Format, Format)>,
}

const DOCUMENTS: [Format; 5] = [
    Format::Docx,
    Format::Doc,
    Format::Odt,
    Format::Rtf,
    Format::Txt,
];
const DOCUMENT_OUT: [Format; 5] = [
    Format::Pdf,
    Format::Docx,
    Format::Odt,
    Format::Rtf,
    Format::Txt,
];
const SHEETS: [Format; 4] = [Format::Xlsx, Format::Xls, Format::Ods, Format::Csv];
const SHEET_OUT: [Format; 4] = [Format::Pdf, Format::Xlsx, Format::Ods, Format::Csv];
const SLIDES: [Format; 3] = [Format::Pptx, Format::Ppt, Format::Odp];
const SLIDE_OUT: [Format; 3] = [Format::Pdf, Format::Pptx, Format::Odp];

impl LibreOffice {
    pub fn probe() -> Option<Self> {
        let bin = which("soffice").or_else(|| which("libreoffice"))?;
        let mut edges = Vec::new();
        for (ins, outs) in [
            (&DOCUMENTS[..], &DOCUMENT_OUT[..]),
            (&SHEETS[..], &SHEET_OUT[..]),
            (&SLIDES[..], &SLIDE_OUT[..]),
        ] {
            for &from in ins {
                for &to in outs {
                    if from != to {
                        edges.push((from, to));
                    }
                }
            }
        }
        Some(Self { bin, edges })
    }
}

/// LibreOffice's `--convert-to` value for a target.
fn filter(from: Format, to: Format, pages: Option<&str>) -> String {
    match to {
        Format::Pdf => {
            let export = match from.category() {
                Category::Spreadsheet => "calc_pdf_Export",
                Category::Presentation => "impress_pdf_Export",
                _ => "writer_pdf_Export",
            };
            match pages {
                Some(p) => format!(
                    "pdf:{export}:{{\"PageRange\":{{\"type\":\"string\",\"value\":\"{}\"}}}}",
                    p.replace(['"', ' '], "")
                ),
                None => format!("pdf:{export}"),
            }
        }
        Format::Docx => "docx:MS Word 2007 XML".into(),
        Format::Xlsx => "xlsx:Calc MS Excel 2007 XML".into(),
        Format::Pptx => "pptx:Impress MS PowerPoint 2007 XML".into(),
        Format::Txt => "txt:Text (encoded):UTF8".into(),
        Format::Csv => "csv:Text - txt - csv (StarCalc):44,34,76".into(),
        other => other.extension().into(),
    }
}

impl Engine for LibreOffice {
    fn name(&self) -> &'static str {
        "libreoffice"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        // A private profile per run: lets several conversions run at once
        // and never touches the user's own LibreOffice settings.
        let profile = tempfile::Builder::new().prefix("joey-lo-").tempdir()?;
        let out_dir = step.work_dir.join(format!("{}.lo", step.stem));
        std::fs::create_dir_all(&out_dir)?;
        let o = step.opts();
        run(
            "libreoffice",
            Command::new(&self.bin)
                .arg(format!(
                    "-env:UserInstallation=file://{}",
                    profile.path().display()
                ))
                .args(["--headless", "--norestore", "--nolockcheck", "--convert-to"])
                .arg(filter(step.from, step.to, o.pages.as_deref()))
                .arg("--outdir")
                .arg(&out_dir)
                .arg(step.input),
        )?;
        let produced = std::fs::read_dir(&out_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.is_file())
            .ok_or_else(|| Error::Tool {
                tool: "libreoffice",
                message: "it produced no file (is the document protected or damaged?)".into(),
            })?;
        let dest = step.output();
        std::fs::rename(&produced, &dest)?;
        let _ = std::fs::remove_dir_all(&out_dir);
        Ok(vec![dest])
    }
}
