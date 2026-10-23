//! Spreadsheets without LibreOffice: Excel (XLSX) to and from CSV and TSV,
//! with Python's openpyxl. One CSV per sheet when a workbook has several.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

const SCRIPT: &str = r#"
import csv, sys, os
src, out, frm, to = sys.argv[1:5]
from openpyxl import Workbook, load_workbook

def cell(v):
    t = v.strip()
    try:
        return int(t) if t.lstrip("-").isdigit() else float(t)
    except ValueError:
        return v

if frm == "xlsx":
    book = load_workbook(src, read_only=True, data_only=True)
    sheets = book.worksheets
    stem, ext = os.path.splitext(out)
    written = []
    for i, ws in enumerate(sheets):
        path = out if len(sheets) == 1 else "%s-%s%s" % (stem, ws.title.replace("/", "-"), ext)
        with open(path, "w", newline="", encoding="utf-8") as f:
            w = csv.writer(f, delimiter="\t" if to == "tsv" else ",")
            for row in ws.iter_rows(values_only=True):
                w.writerow(["" if v is None else v for v in row])
        written.append(path)
    print("\n".join(written))
else:
    book = Workbook()
    ws = book.active
    ws.title = os.path.splitext(os.path.basename(src))[0][:31] or "Sheet"
    with open(src, newline="", encoding="utf-8-sig") as f:
        sample = f.read(4096)
        f.seek(0)
        delim = "\t" if frm == "tsv" else (csv.Sniffer().sniff(sample, ",;").delimiter if sample else ",")
        for row in csv.reader(f, delimiter=delim):
            ws.append([cell(v) for v in row])
    book.save(out)
    print(out)
"#;

pub struct Sheets {
    python: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl Sheets {
    pub fn probe() -> Option<Self> {
        let python = which("python3")?;
        let ok = Command::new(&python)
            .args([
                "-c",
                "import importlib.util as u, sys; sys.exit(0 if u.find_spec('openpyxl') else 1)",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then(|| Self {
            python,
            edges: vec![
                (Format::Xlsx, Format::Csv),
                (Format::Xlsx, Format::Tsv),
                (Format::Csv, Format::Xlsx),
                (Format::Tsv, Format::Xlsx),
            ],
        })
    }
}

impl Engine for Sheets {
    fn name(&self) -> &'static str {
        "sheets"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let out = step.output();
        let printed = run(
            "sheets",
            Command::new(&self.python)
                .args(["-c", SCRIPT])
                .arg(step.input)
                .arg(&out)
                .arg(step.from.extension())
                .arg(step.to.extension()),
        )?;
        Ok(printed.lines().map(PathBuf::from).collect())
    }
}
