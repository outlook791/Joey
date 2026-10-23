//! Tables in a PDF to an Excel workbook, one sheet per table.
//!
//! Uses the best free extractor installed: Camelot (ruled tables, then
//! whitespace-aligned ones) or pdfplumber; if a page has no ruled table,
//! text laid out in columns is split by its gaps. Works on PDFs made by a
//! computer; scanned pages have no text to read.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

const SCRIPT: &str = r#"
import sys, re
src, out, pages = sys.argv[1], sys.argv[2], (sys.argv[3] or "all")

def page_list(spec, count):
    if spec == "all":
        return list(range(1, count + 1))
    chosen = []
    for part in spec.replace(";", ",").split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            chosen += range(int(a), min(int(b), count) + 1)
        else:
            chosen.append(int(part))
    return [p for p in chosen if 1 <= p <= count]

def number(cell):
    text = ("" if cell is None else str(cell)).strip()
    plain = text.replace(" ", "").replace(" ", "")
    if re.fullmatch(r"-?\d+", plain):
        return int(plain)
    if re.fullmatch(r"-?\d{1,3}(,\d{3})+(\.\d+)?|-?\d+\.\d+", plain):
        return float(plain.replace(",", ""))
    if re.fullmatch(r"-?\d{1,3}(\.\d{3})+(,\d+)?|-?\d+,\d+", plain):
        return float(plain.replace(".", "").replace(",", "."))
    return text

tables = []
try:
    import camelot
    for flavor in ("lattice", "stream"):
        found = camelot.read_pdf(src, pages=pages if pages != "all" else "1-end", flavor=flavor)
        good = [t.df.values.tolist() for t in found
                if t.df.size and t.parsing_report.get("accuracy", 100) >= 70]
        if good:
            tables = good
            break
except Exception:
    tables = []

if not tables:
    import pdfplumber
    with pdfplumber.open(src) as pdf:
        for n in page_list(pages, len(pdf.pages)):
            page = pdf.pages[n - 1]
            found = [t for t in page.extract_tables() if t and any(any(c for c in r) for r in t)]
            if found:
                tables += found
                continue
            # No ruled table: split text laid out in columns at its gaps.
            text = page.extract_text(layout=True) or ""
            rows = [re.split(r"\s{2,}", line.strip()) for line in text.splitlines() if line.strip()]
            if sum(1 for r in rows if len(r) > 1) >= 2:
                tables.append(rows)

if not tables:
    sys.stderr.write("NO_TABLES\n")
    sys.exit(3)

from openpyxl import Workbook
book = Workbook()
book.remove(book.active)
for i, table in enumerate(tables, 1):
    sheet = book.create_sheet("Table %d" % i)
    for row in table:
        sheet.append([number(c) for c in row])
    for col in sheet.columns:
        width = max((len(str(c.value)) for c in col if c.value is not None), default=8)
        sheet.column_dimensions[col[0].column_letter].width = min(max(width + 2, 8), 60)
book.save(out)
"#;

pub struct PdfTables {
    python: PathBuf,
    edges: Vec<(Format, Format)>,
}

impl PdfTables {
    pub fn probe() -> Option<Self> {
        let python = which("python3")?;
        // Checks the libraries exist without importing them (fast).
        let check = "import importlib.util as u, sys; \
                     ok = u.find_spec('openpyxl') and (u.find_spec('pdfplumber') or u.find_spec('camelot')); \
                     sys.exit(0 if ok else 1)";
        let ok = Command::new(&python)
            .args(["-c", check])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then(|| Self {
            python,
            edges: vec![(Format::Pdf, Format::Xlsx)],
        })
    }
}

impl Engine for PdfTables {
    fn name(&self) -> &'static str {
        "pdf-tables"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        let out = step.output();
        let pages = o.pages.clone().unwrap_or_default().replace(' ', "");
        run(
            "pdf-tables",
            Command::new(&self.python)
                .args(["-c", SCRIPT])
                .arg(step.input)
                .arg(&out)
                .arg(pages),
        )
        .map_err(|e| match e {
            Error::Tool { message, .. } if message.contains("NO_TABLES") => Error::Tool {
                tool: "joey",
                message: "no tables found in this PDF".into(),
            },
            other => other,
        })?;
        Ok(vec![out])
    }
}
