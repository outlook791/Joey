//! Spreadsheets without LibreOffice: Excel (XLSX, and XLS with xlrd), ODS,
//! CSV and TSV to Excel, CSV, TSV and (with Typst) PDF tables, in Python
//! with openpyxl. One CSV per sheet when a workbook has several.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

const SCRIPT: &str = r##"
import csv, json, os, re, subprocess, sys, zipfile
import xml.etree.ElementTree as ET
src, out, frm, to = sys.argv[1:5]
typst = sys.argv[5] if len(sys.argv) > 5 else ""

def cell(v):
    t = v.strip()
    try:
        return int(t) if t.lstrip("-").isdigit() else float(t)
    except ValueError:
        return v

def plain(v):
    if v is None:
        return ""
    if isinstance(v, float) and v.is_integer():
        return str(int(v))
    return str(v)

def trim(rows):
    rows = [list(r) for r in rows]
    while rows and not any(plain(c).strip() for c in rows[-1]):
        rows.pop()
    width = max((max((i + 1 for i, c in enumerate(r) if plain(c).strip()), default=0) for r in rows), default=0)
    return [(r + [""] * width)[:width] for r in rows]

def read():
    """[(sheet name, rows)] from any supported spreadsheet."""
    if frm == "xlsx":
        from openpyxl import load_workbook
        book = load_workbook(src, read_only=True, data_only=True)
        return [(ws.title, trim(ws.iter_rows(values_only=True))) for ws in book.worksheets]
    if frm == "xls":
        import xlrd
        book = xlrd.open_workbook(src)
        return [(sh.name, trim(sh.row_values(i) for i in range(sh.nrows))) for sh in book.sheets()]
    if frm == "ods":
        ns = {"table": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
              "office": "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
              "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0"}
        q = lambda p, n: "{%s}%s" % (ns[p], n)
        root = ET.fromstring(zipfile.ZipFile(src).read("content.xml"))
        sheets = []
        for t in root.iter(q("table", "table")):
            rows = []
            for r in t.iter(q("table", "table-row")):
                row = []
                for c in r:
                    if c.tag not in (q("table", "table-cell"), q("table", "covered-table-cell")):
                        continue
                    v = c.get(q("office", "value"))
                    if v is None:
                        v = "\n".join("".join(p.itertext()) for p in c.findall(q("text", "p")))
                    else:
                        v = cell(v)
                    n = int(c.get(q("table", "number-columns-repeated"), "1"))
                    row += [v] * (n if plain(v) or n < 1000 else 1)
                n = int(r.get(q("table", "number-rows-repeated"), "1"))
                rows += [row] * (n if any(plain(x) for x in row) or n < 1000 else 1)
            sheets.append((t.get(q("table", "name")) or "Sheet", trim(rows)))
        return sheets
    with open(src, newline="", encoding="utf-8-sig") as f:
        sample = f.read(4096)
        f.seek(0)
        delim = "\t" if frm == "tsv" else (csv.Sniffer().sniff(sample, ",;").delimiter if sample else ",")
        name = os.path.splitext(os.path.basename(src))[0][:31] or "Sheet"
        return [(name, trim([[cell(v) for v in row] for row in csv.reader(f, delimiter=delim)]))]

def outputs(sheets, ext):
    stem = os.path.splitext(out)[0]
    if len(sheets) == 1:
        return [out]
    return ["%s-%s.%s" % (stem, re.sub(r"[/\\:]", "-", n), ext) for n, _ in sheets]

sheets = read()
if to in ("csv", "tsv"):
    paths = outputs(sheets, to)
    for (name, rows), path in zip(sheets, paths):
        with open(path, "w", newline="", encoding="utf-8") as f:
            w = csv.writer(f, delimiter="\t" if to == "tsv" else ",")
            for r in rows:
                w.writerow([plain(c) for c in r])
    print("\n".join(paths))
elif to == "xlsx":
    from openpyxl import Workbook
    book = Workbook()
    book.remove(book.active)
    for name, rows in sheets:
        ws = book.create_sheet(name[:31] or "Sheet")
        for r in rows:
            ws.append(r)
    book.save(out)
    print(out)
elif to == "pdf":
    # Every sheet as a table, laid out by Typst.
    parts = ['#set page(flipped: true, margin: 1.5cm)', '#set text(size: 8pt)']
    for name, rows in sheets:
        if not rows:
            continue
        if len(sheets) > 1:
            parts.append("= " + json.dumps(name)[1:-1].replace("#", "\\#"))
        cells = ", ".join(json.dumps(plain(c)) for r in rows for c in r)
        parts.append("#table(columns: %d, %s)" % (len(rows[0]), cells))
        parts.append("#pagebreak(weak: true)")
    source = os.path.splitext(out)[0] + ".typ"
    with open(source, "w", encoding="utf-8") as f:
        f.write("\n".join(parts) + "\n")
    try:
        subprocess.run([typst, "compile", source, out], check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as e:
        sys.stderr.write(e.stderr)
        sys.exit(1)
    finally:
        os.remove(source)
    print(out)
"##;

pub struct Sheets {
    python: PathBuf,
    typst: Option<PathBuf>,
    edges: Vec<(Format, Format)>,
}

impl Sheets {
    pub fn probe() -> Option<Self> {
        let python = which("python3")?;
        let has = |module: &str| {
            Command::new(&python)
                .args([
                    "-c",
                    &format!(
                        "import importlib.util as u, sys; sys.exit(0 if u.find_spec('{module}') else 1)"
                    ),
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        if !has("openpyxl") {
            return None;
        }
        let typst = which("typst");
        let mut inputs = vec![Format::Xlsx, Format::Ods, Format::Csv, Format::Tsv];
        if has("xlrd") {
            inputs.push(Format::Xls);
        }
        let mut outputs = vec![Format::Xlsx, Format::Csv, Format::Tsv];
        if typst.is_some() {
            outputs.push(Format::Pdf);
        }
        let mut edges = Vec::new();
        for &from in &inputs {
            for &to in &outputs {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        Some(Self {
            python,
            typst,
            edges,
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
                .arg(step.to.extension())
                .arg(self.typst.as_deref().unwrap_or(std::path::Path::new(""))),
        )?;
        Ok(printed.lines().map(PathBuf::from).collect())
    }
}
