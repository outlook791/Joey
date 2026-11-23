//! A PDF as Markdown made for AI apps: headings, lists, paragraphs and
//! tables as text, with the document's details on top. An AI reading this
//! uses far fewer tokens than reading the pages as pictures, and sees the
//! structure that plain text loses.
//!
//! Reads the text and where each piece sits on the page from Poppler's
//! `pdftotext -bbox-layout` or, when Poppler isn't installed (the Flatpak),
//! from Ghostscript's `txtwrite`. Font sizes give the headings; gaps give
//! words, table cells and page columns. Works on PDFs made by a computer:
//! scanned pages are pictures and have no text to read.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::Format;
use crate::Error;

pub struct PdfMarkdown {
    pdftotext: Option<PathBuf>,
    gs: Option<PathBuf>,
    edges: Vec<(Format, Format)>,
}

impl PdfMarkdown {
    pub fn probe() -> Option<Self> {
        let pdftotext = which("pdftotext");
        let gs = which("gs");
        if pdftotext.is_none() && gs.is_none() {
            return None;
        }
        Some(Self {
            pdftotext,
            gs,
            edges: vec![(Format::Pdf, Format::Md)],
        })
    }
}

impl Engine for PdfMarkdown {
    fn name(&self) -> &'static str {
        "pdf-markdown"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        // Ghostscript knows the fonts (true sizes, bold) and comes with
        // the Flatpak; Poppler stands in without it. JOEY_PDF_TEXT=poppler
        // tries Poppler where both are installed (tests).
        let poppler = self.pdftotext.as_ref().filter(|_| {
            std::env::var("JOEY_PDF_TEXT").as_deref() == Ok("poppler") || self.gs.is_none()
        });
        let pages = if let Some(bin) = poppler {
            let xml = run(
                "pdftotext",
                Command::new(bin)
                    .args(["-bbox-layout", "-enc", "UTF-8"])
                    .arg(step.input)
                    .arg("-"),
            )?;
            parse_poppler(&xml)
        } else {
            let bin = self.gs.as_ref().expect("probe checked");
            let xml_path = step.work_dir.join(format!("{}.text.xml", step.stem));
            let mut cmd = Command::new(bin);
            cmd.args([
                "-q",
                "-dNOPAUSE",
                "-dBATCH",
                "-dSAFER",
                "-sDEVICE=txtwrite",
                "-dTextFormat=0",
            ])
            .arg("-o")
            .arg(&xml_path)
            .arg(step.input);
            // Ghostscript sometimes exits with a harmless complaint after
            // writing everything: judge by what it wrote.
            let ran = run("ghostscript", &mut cmd);
            let xml = std::fs::read_to_string(&xml_path).unwrap_or_default();
            let _ = std::fs::remove_file(&xml_path);
            if xml.is_empty() {
                ran?;
            }
            parse_ghostscript(&xml)
        };

        let chosen: Option<BTreeSet<usize>> = {
            let ranges = o.page_ranges();
            (!ranges.is_empty()).then(|| {
                ranges
                    .iter()
                    .flat_map(|&(a, b)| a as usize..=b as usize)
                    .collect()
            })
        };
        let info = info(step.input);
        let name = step
            .input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let md = to_markdown(&pages, chosen.as_ref(), &info, &name)?;
        let out = step.output();
        std::fs::write(&out, md)?;
        Ok(vec![out])
    }
}

/// A piece of text on a page: a glyph (Ghostscript) or a word (Poppler).
/// `y` is the baseline, growing down the page.
#[derive(Debug, Clone)]
struct Piece {
    x0: f32,
    x1: f32,
    y: f32,
    size: f32,
    bold: bool,
    text: String,
}

/// One line of text: its words, and the same text split into cells where
/// there is a wide gap.
#[derive(Debug, Clone)]
struct Line {
    words: Vec<Cell>,
    cells: Vec<Cell>,
    y: f32,
    size: f32,
    bold: bool,
}

#[derive(Debug, Clone)]
struct Cell {
    x0: f32,
    x1: f32,
    text: String,
}

impl Line {
    fn x0(&self) -> f32 {
        self.cells.first().map_or(0.0, |c| c.x0)
    }
    fn text(&self) -> String {
        self.cells
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

type Page = Vec<Piece>;

// ---- Reading the tools' output ----

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn num(tag: &str, name: &str) -> f32 {
    attr(tag, name).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// Decodes XML entities, and splits ligatures into letters.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.retain(|c| !c.is_control());
    out.replace('\u{fb00}', "ff")
        .replace('\u{fb01}', "fi")
        .replace('\u{fb02}', "fl")
        .replace('\u{fb03}', "ffi")
        .replace('\u{fb04}', "ffl")
        .replace('\u{00ad}', "")
}

/// `pdftotext -bbox-layout`: pages of words with their boxes. Word height
/// stands in for the font size (only sizes relative to each other count).
fn parse_poppler(xml: &str) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut page: Option<Page> = None;
    for raw in xml.lines() {
        let line = raw.trim();
        if line.starts_with("<page ") {
            page = Some(Vec::new());
        } else if line.starts_with("</page>") {
            pages.extend(page.take());
        } else if line.starts_with("<word ") {
            let Some(p) = page.as_mut() else { continue };
            let (Some(open), Some(close)) = (line.find('>'), line.rfind("</word>")) else {
                continue;
            };
            let text = unescape(&line[open + 1..close]);
            let (y0, y1) = (num(line, "yMin"), num(line, "yMax"));
            p.push(Piece {
                x0: num(line, "xMin"),
                x1: num(line, "xMax"),
                y: y1,
                size: (y1 - y0).max(1.0),
                bold: false,
                text,
            });
        }
    }
    pages
}

/// Ghostscript `txtwrite -dTextFormat=0`: spans with a font and size, made
/// of glyphs with their boxes. TeX PDFs have no space glyphs: spaces come
/// from the gaps, later.
fn parse_ghostscript(xml: &str) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut page: Option<Page> = None;
    let (mut size, mut bold) = (10.0f32, false);
    for raw in xml.lines() {
        let line = raw.trim();
        if line.starts_with("<page") {
            page = Some(Vec::new());
        } else if line.starts_with("</page>") {
            pages.extend(page.take());
        } else if line.starts_with("<span ") {
            size = num(line, "size").max(1.0);
            let font = attr(line, "font").unwrap_or("").to_ascii_lowercase();
            bold = font.contains("bold")
                || font.contains("black")
                || font.contains("heavy")
                || font.contains("semibold")
                || font.contains("+cmbx")
                || font.contains("sfbx");
        } else if line.starts_with("<char ") {
            let Some(p) = page.as_mut() else { continue };
            let Some(c) = attr(line, "c") else { continue };
            let text = unescape(c);
            if text.trim().is_empty() {
                continue;
            }
            let b: Vec<f32> = attr(line, "bbox")
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if b.len() < 4 {
                continue;
            }
            p.push(Piece {
                x0: b[0],
                x1: b[2].max(b[0]),
                y: b[1],
                size,
                bold,
                text,
            });
        }
    }
    pages
}

// ---- Pieces into lines and cells ----

/// Groups a page's pieces into lines (by baseline), words (by small gaps)
/// and cells (by wide gaps), top to bottom.
fn lines(page: &Page) -> Vec<Line> {
    let mut pieces: Vec<&Piece> = page.iter().collect();
    pieces.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x0.total_cmp(&b.x0)));
    // Rows: pieces whose baselines are close (sub- and superscripts stay
    // in their line).
    let mut rows: Vec<Vec<&Piece>> = Vec::new();
    for p in pieces {
        match rows.last_mut() {
            Some(row)
                if {
                    let base = row[0];
                    (p.y - base.y).abs() <= 0.45 * base.size.max(p.size)
                } =>
            {
                row.push(p)
            }
            _ => rows.push(vec![p]),
        }
    }
    let mut out = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        // The size most of the line is written in.
        let mut weights: HashMap<i32, usize> = HashMap::new();
        for p in &row {
            *weights.entry((p.size * 2.0).round() as i32).or_default() += p.text.chars().count();
        }
        let size = weights
            .iter()
            .max_by_key(|(s, n)| (**n, **s))
            .map(|(s, _)| *s as f32 / 2.0)
            .unwrap_or(10.0);
        let bold = row.iter().all(|p| p.bold);
        let y = row
            .iter()
            .find(|p| ((p.size * 2.0).round() as i32) == (size * 2.0).round() as i32)
            .map_or(row[0].y, |p| p.y);

        // Words: pieces closer than a space.
        let mut words: Vec<Cell> = Vec::new();
        for p in row {
            match words.last_mut() {
                Some(w) if p.x0 - w.x1 < 0.15 * size.min(p.size.max(size * 0.6)) => {
                    w.text.push_str(&p.text);
                    w.x1 = w.x1.max(p.x1);
                }
                _ => words.push(Cell {
                    x0: p.x0,
                    x1: p.x1,
                    text: p.text.trim().to_string(),
                }),
            }
        }
        words.retain(|w| !w.text.is_empty());
        // Dot leaders, as in a table of contents: ". . . . ." becomes "…".
        let mut tidy: Vec<Cell> = Vec::new();
        for w in words {
            let dots = |t: &str| !t.is_empty() && t.chars().all(|c| c == '.' || c == '…');
            match tidy.last_mut() {
                Some(prev) if dots(&w.text) && (prev.text == "…" || prev.text.ends_with(". .")) =>
                {
                    prev.x1 = w.x1;
                }
                Some(prev) if dots(&w.text) && prev.text.ends_with('.') && w.text.len() >= 2 => {
                    prev.x1 = w.x1;
                    prev.text = prev.text.trim_end_matches(['.', ' ']).to_string() + " …";
                }
                _ => tidy.push(w),
            }
        }
        // Runs of lone dots, one word each.
        let mut words: Vec<Cell> = Vec::new();
        for w in tidy {
            let lone = w.text == ".";
            match words.last_mut() {
                Some(prev) if lone && (prev.text == "…" || prev.text == ".") => {
                    prev.text = "…".into();
                    prev.x1 = w.x1;
                }
                _ => words.push(w),
            }
        }
        let cells = cells_of(&words, size);
        if !cells.is_empty() {
            out.push(Line {
                words,
                cells,
                y,
                size,
                bold,
            });
        }
    }
    out
}

/// Words into cells, split where the gap is wide.
fn cells_of(words: &[Cell], size: f32) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for w in words {
        match cells.last_mut() {
            Some(c) if w.x0 - c.x1 < 1.2 * size => {
                c.text.push(' ');
                c.text.push_str(&w.text);
                c.x1 = c.x1.max(w.x1);
            }
            _ => cells.push(w.clone()),
        }
    }
    cells
}

/// Reading order for pages printed in two columns: finds the white strip
/// down the middle that the lines don't cross, and reads the left column,
/// then the right one, between lines that span both (titles, figures).
fn reading_order(lines: Vec<Line>) -> Vec<Line> {
    if lines.len() < 6 {
        return lines;
    }
    let left_edge = lines.iter().map(|l| l.x0()).fold(f32::MAX, f32::min);
    let right_edge = lines
        .iter()
        .filter_map(|l| l.words.last().map(|w| w.x1))
        .fold(0.0, f32::max);
    let width = right_edge - left_edge;
    if width <= 0.0 {
        return lines;
    }
    // A line crosses x when a word covers it or x falls in a gap between
    // words narrower than a column gap.
    let crosses = |l: &Line, x: f32| {
        l.words.iter().any(|w| w.x0 <= x && x <= w.x1)
            || l.words
                .windows(2)
                .any(|p| p[0].x1 < x && x < p[1].x0 && p[1].x0 - p[0].x1 < 0.8 * l.size)
    };
    let mut best: Option<(usize, f32, f32)> = None; // (crossing, x, distance from the middle)
    let middle = left_edge + width / 2.0;
    let mut x = left_edge + 0.3 * width;
    while x <= left_edge + 0.7 * width {
        let crossing = lines.iter().filter(|l| crosses(l, x)).count();
        let dist = (x - middle).abs();
        if best.is_none_or(|(c, _, d)| crossing < c || crossing == c && dist < d) {
            best = Some((crossing, x, dist));
        }
        x += 1.0;
    }
    let Some((crossing, gutter, _)) = best else {
        return lines;
    };
    let left = lines
        .iter()
        .filter(|l| l.words.iter().any(|w| w.x1 < gutter))
        .count();
    let right = lines
        .iter()
        .filter(|l| l.words.iter().any(|w| w.x0 > gutter))
        .count();
    if crossing * 100 > lines.len() * 30 || left < 4 || right < 4 {
        return lines;
    }
    // Most lines on the two sides must really be text, not a narrow table.
    let full = |side_left: bool| {
        lines
            .iter()
            .filter(|l| !crosses(l, gutter))
            .filter(|l| {
                let words: Vec<&Cell> = l
                    .words
                    .iter()
                    .filter(|w| {
                        if side_left {
                            w.x1 < gutter
                        } else {
                            w.x0 > gutter
                        }
                    })
                    .collect();
                words.len() >= 4
            })
            .count()
    };
    if full(true) * 2 < left || full(false) * 2 < right {
        return lines;
    }
    let crossing_line = |l: &Line| crosses(l, gutter);
    let mut out = Vec::new();
    let (mut left, mut right): (Vec<Line>, Vec<Line>) = (Vec::new(), Vec::new());
    let flush = |out: &mut Vec<Line>, left: &mut Vec<Line>, right: &mut Vec<Line>| {
        out.append(left);
        out.append(right);
    };
    for line in lines {
        if crossing_line(&line) {
            flush(&mut out, &mut left, &mut right);
            out.push(line);
            continue;
        }
        let (l, r): (Vec<Cell>, Vec<Cell>) =
            line.words.iter().cloned().partition(|c| c.x1 <= gutter);
        if !l.is_empty() {
            left.push(Line {
                cells: cells_of(&l, line.size),
                words: l,
                ..line.clone()
            });
        }
        if !r.is_empty() {
            right.push(Line {
                cells: cells_of(&r, line.size),
                words: r,
                ..line
            });
        }
    }
    flush(&mut out, &mut left, &mut right);
    out
}

// ---- Running headers, footers and page numbers ----

fn is_page_number(text: &str) -> bool {
    let t = text
        .trim()
        .trim_matches(|c: char| "-–—|·• ".contains(c))
        .to_lowercase();
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    let numeric = |w: &str| w.chars().all(|c| c.is_ascii_digit() || c == '/');
    match words.as_slice() {
        [w, n] => {
            [
                "page",
                "seite",
                "pagina",
                "pag.",
                "p.",
                "página",
                "страница",
                "strona",
            ]
            .contains(w)
                && numeric(n)
        }
        [w, n, _, m] => {
            ["page", "seite", "pagina", "página"].contains(w) && numeric(n) && numeric(m)
        }
        [n, of, m] => numeric(n) && ["of", "von", "di", "de", "/"].contains(of) && numeric(m),
        _ => false,
    }
}

fn shape(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drops lines repeated at the top or bottom of most pages, and lone page
/// numbers there.
fn drop_running(pages: &mut [Vec<Line>]) {
    let edge = |p: &Vec<Line>| -> Vec<usize> {
        // The first and last lines: two of each on a full page, fewer on
        // a short one, so the text itself is never taken for a header.
        let n = p.len();
        let k = (n / 3).min(2);
        let mut idx: Vec<usize> = (0..k).collect();
        idx.extend(n.saturating_sub(k).max(k)..n);
        idx
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    for p in pages.iter() {
        let shapes: BTreeSet<String> = edge(p).iter().map(|&i| shape(&p[i].text())).collect();
        for s in shapes {
            *seen.entry(s).or_default() += 1;
        }
    }
    let count = pages.len();
    let many = count >= 3;
    for p in pages.iter_mut() {
        let drop: BTreeSet<usize> = edge(p)
            .into_iter()
            .filter(|&i| {
                let t = p[i].text();
                let repeats = seen.get(&shape(&t)).copied().unwrap_or(0);
                // A chapter title printed with the page number, on the
                // first or last line of its pages.
                let numbered = {
                    let words: Vec<&str> = t.split_whitespace().collect();
                    let is_num = |w: &&str| w.chars().all(|c| c.is_ascii_digit());
                    words.len() >= 2
                        && (words.first().is_some_and(is_num) || words.last().is_some_and(is_num))
                };
                let outer = i == 0 || i + 1 == p.len();
                is_page_number(&t)
                    || (many && repeats * 2 >= count)
                    || (outer && numbered && repeats >= 3 && repeats * 10 >= count)
            })
            .collect();
        let mut i = 0;
        p.retain(|_| {
            let keep = !drop.contains(&i);
            i += 1;
            keep
        });
    }
}

// ---- Lines into Markdown ----

fn bullet(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if "•◦▪▫‣∙·●○■□–—-*➢➤►▸✓✔".contains(first) {
        let rest = chars.as_str().trim_start();
        if !rest.is_empty() && (first != '-' || text.starts_with("- ")) {
            return Some(format!("- {rest}"));
        }
    }
    // "1." "2)" "a)" "iv." followed by text.
    let (head, rest) = text.split_once(' ')?;
    let mark = head.trim_end_matches(['.', ')']);
    if head.len() > mark.len()
        && !rest.is_empty()
        && (mark.chars().all(|c| c.is_ascii_digit()) && mark.len() <= 3
            || mark.len() == 1 && mark.chars().all(|c| c.is_ascii_lowercase()))
    {
        let n = if mark.chars().all(|c| c.is_ascii_digit()) {
            format!("{mark}.")
        } else {
            head.to_string()
        };
        return Some(format!("{n} {}", rest.trim_start()));
    }
    None
}

fn escape(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('#') || t.starts_with('>') || t.starts_with("```") {
        format!("\\{t}")
    } else {
        t.to_string()
    }
}

fn cell_text(text: &str) -> String {
    text.replace('|', "\\|")
}

/// Joins wrapped lines into one, mending words split by a hyphen.
fn join(into: &mut String, next: &str) {
    if into.is_empty() {
        into.push_str(next);
        return;
    }
    let next_lower = next.chars().next().is_some_and(|c| c.is_lowercase());
    if into.ends_with('-')
        && next_lower
        && into.chars().rev().nth(1).is_some_and(|c| c.is_alphabetic())
    {
        into.pop();
    } else {
        into.push(' ');
    }
    into.push_str(next);
}

/// Columns of white running down a block of lines: the x positions
/// where no line has a word. Two or more of them make a table.
fn rivers(lines: &[Line]) -> Vec<f32> {
    let mut spans: Vec<(f32, f32)> = lines
        .iter()
        .flat_map(|l| l.words.iter().map(|w| (w.x0, w.x1)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let size = lines.iter().map(|l| l.size).fold(0.0, f32::max);
    let mut out = Vec::new();
    let mut reach = f32::MIN;
    for (x0, x1) in spans {
        if reach > f32::MIN && x0 - reach >= 0.2 * size {
            out.push((reach + x0) / 2.0);
        }
        reach = reach.max(x1);
    }
    out
}

/// Which column of a table a word falls in.
fn column(x: f32, splits: &[f32]) -> usize {
    splits.iter().take_while(|s| **s < x).count()
}

/// How many lines from `i` on read as a table (0 when they don't): at
/// least three close lines with two or more white columns in common, each
/// line having words in two columns or more. Two lines do when both are
/// split by wide gaps into three cells or more.
fn table_run(page: &[Line], i: usize, is_heading: &dyn Fn(&Line) -> bool) -> usize {
    let fits = |l: &Line| !is_heading(l) && l.words.len() >= 2 && bullet(&l.text()).is_none();
    if !fits(&page[i]) {
        return 0;
    }
    let mut j = i + 1;
    let mut best = 0;
    let (mut low, mut high) = (f32::MAX, 0.0f32);
    while j < page.len()
        && fits(&page[j])
        && page[j].y - page[j - 1].y > 0.0
        && page[j].y - page[j - 1].y < 3.0 * page[j].size.max(page[j - 1].size)
    {
        // Rows are evenly spaced: a wider gap is text before the table.
        let gap = page[j].y - page[j - 1].y;
        low = low.min(gap);
        high = high.max(gap);
        if high > 2.5 * low {
            break;
        }
        let block = &page[i..=j];
        let splits = rivers(block);
        if splits.len() < 2 {
            break;
        }
        let spread = block.iter().all(|l| {
            let cols: BTreeSet<usize> = l.words.iter().map(|w| column(w.x0, &splits)).collect();
            cols.len() >= 2
        });
        // Table cells are short; prose cut by a chance gap is not.
        let filled: usize = block
            .iter()
            .map(|l| {
                l.words
                    .iter()
                    .map(|w| column(w.x0, &splits))
                    .collect::<BTreeSet<_>>()
                    .len()
            })
            .sum();
        let words: usize = block.iter().map(|l| l.words.len()).sum();
        // Mostly empty, or cells of lone symbols: a formula, not a table.
        let cols = splits.len() + 1;
        let lone = block
            .iter()
            .flat_map(|l| l.words.iter())
            .filter(|w| w.text.chars().count() == 1 && !w.text.chars().all(char::is_alphanumeric))
            .count();
        if !spread
            || words > 3 * filled
            || filled * 100 < block.len() * cols * 45
            || lone * 4 > words
        {
            break;
        }
        j += 1;
        best = j - i;
    }
    if best >= 3 {
        return best;
    }
    if best == 2 && page[i].cells.len() >= 3 && page[i + 1].cells.len() >= 3 {
        return 2;
    }
    0
}

/// The lines as a Markdown table, columns found from the white between.
fn table(rows: &[Line]) -> String {
    let splits = rivers(rows);
    let cols = splits.len() + 1;
    let mut grid: Vec<Vec<String>> = Vec::new();
    for r in rows {
        let mut row = vec![String::new(); cols];
        for w in &r.words {
            let c = column(w.x0, &splits);
            if !row[c].is_empty() {
                row[c].push(' ');
            }
            row[c].push_str(&cell_text(&w.text));
        }
        grid.push(row);
    }
    // Columns left empty in every row (a stray gap) go.
    let keep: Vec<usize> = (0..cols)
        .filter(|&c| grid.iter().any(|r| !r[c].is_empty()))
        .collect();
    let mut md = String::new();
    for (i, row) in grid.iter().enumerate() {
        let cells: Vec<&str> = keep.iter().map(|&c| row[c].as_str()).collect();
        md.push_str("| ");
        md.push_str(&cells.join(" | "));
        md.push_str(" |\n");
        if i == 0 {
            md.push('|');
            md.push_str(&" --- |".repeat(keep.len()));
            md.push('\n');
        }
    }
    md
}

/// What `pdfinfo` (or qpdf) knows about the document.
#[derive(Default)]
struct Info {
    title: String,
    author: String,
    subject: String,
    created: String,
}

fn info(pdf: &Path) -> Info {
    let mut i = Info::default();
    if let Some(bin) = which("pdfinfo") {
        if let Ok(out) = Command::new(bin).args(["-isodates"]).arg(pdf).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let Some((k, v)) = line.split_once(':') else {
                    continue;
                };
                let v = v.trim().to_string();
                match k {
                    "Title" => i.title = v,
                    "Author" => i.author = v,
                    "Subject" => i.subject = v,
                    "CreationDate" => i.created = v.chars().take(10).collect(),
                    _ => {}
                }
            }
            return i;
        }
    }
    let Some(qpdf) = which("qpdf") else { return i };
    let Ok(trailer) = Command::new(&qpdf)
        .arg("--show-object=trailer")
        .arg(pdf)
        .output()
    else {
        return i;
    };
    let trailer = String::from_utf8_lossy(&trailer.stdout).into_owned();
    let Some(id) = trailer
        .split("/Info ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse::<u32>().ok())
    else {
        return i;
    };
    let Ok(json) = Command::new(&qpdf)
        .args(["--json=2", "--json-key=qpdf"])
        .arg(format!("--json-object={id}"))
        .arg(pdf)
        .output()
    else {
        return i;
    };
    let json = String::from_utf8_lossy(&json.stdout).into_owned();
    let get = |key: &str| -> String {
        let pat = format!("\"/{key}\": \"u:");
        json.split(&pat)
            .nth(1)
            .and_then(|r| {
                // The value ends at the first unescaped quote.
                let mut end = None;
                let mut esc = false;
                for (n, c) in r.char_indices() {
                    if esc {
                        esc = false;
                    } else if c == '\\' {
                        esc = true;
                    } else if c == '"' {
                        end = Some(n);
                        break;
                    }
                }
                end.map(|e| r[..e].replace("\\\"", "\"").replace("\\\\", "\\"))
            })
            .unwrap_or_default()
    };
    i.title = get("Title");
    i.author = get("Author");
    i.subject = get("Subject");
    let d = get("CreationDate");
    let d = d.trim_start_matches("D:");
    if d.len() >= 8 && d[..8].chars().all(|c| c.is_ascii_digit()) {
        i.created = format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8]);
    }
    i
}

fn yaml(v: &str) -> String {
    format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The whole document as Markdown.
fn to_markdown(
    pages: &[Page],
    chosen: Option<&BTreeSet<usize>>,
    info: &Info,
    file_name: &str,
) -> Result<String, Error> {
    let mut laid: Vec<Vec<Line>> = pages.iter().map(lines).collect();
    drop_running(&mut laid);
    let laid: Vec<Vec<Line>> = laid.into_iter().map(reading_order).collect();

    // Body size: the one most text is written in.
    let mut weights: HashMap<i32, usize> = HashMap::new();
    for l in laid.iter().flatten() {
        *weights.entry((l.size * 2.0).round() as i32).or_default() += l.text().chars().count();
    }
    let total: usize = weights.values().sum();
    if total == 0 {
        return Err(Error::Tool {
            tool: "joey",
            message: "the PDF has no text: its pages are pictures (scanned)".into(),
        });
    }
    let body = weights
        .iter()
        .max_by_key(|(s, n)| (**n, -**s))
        .map(|(s, _)| *s as f32 / 2.0)
        .unwrap_or(10.0);
    // Heading levels: larger sizes, biggest first. Sizes used for whole
    // pages of text (a large-print booklet) are not headings.
    let mut heading_sizes: Vec<i32> = weights
        .iter()
        .filter(|(s, n)| **s as f32 / 2.0 >= body * 1.15 && **n * 4 < total)
        .map(|(s, _)| *s)
        .collect();
    heading_sizes.sort_unstable_by(|a, b| b.cmp(a));
    let size_level = |size: f32| -> Option<usize> {
        let s = (size * 2.0).round() as i32;
        heading_sizes
            .iter()
            .position(|h| *h == s)
            .map(|i| (i + 1).min(4))
    };
    // A heading is words: not a formula, not the numbers of a chart.
    let wordy = |text: &str| {
        let letters = text.chars().filter(|c| c.is_alphabetic()).count();
        let other = text.chars().filter(|c| !c.is_whitespace()).count();
        letters >= 2
            && letters * 10 >= other * 6
            && !text.contains(['=', '≤', '≥', '∀', '∃', '∈', '⊆', '→', '⇔', '∑', '∫', '|'])
    };
    let level = |line: &Line| size_level(line.size).filter(|_| wordy(&line.text()));

    let mut md = String::new();
    md.push_str("---\n");
    if !info.title.is_empty() {
        md.push_str(&format!("title: {}\n", yaml(&info.title)));
    }
    if !info.author.is_empty() {
        md.push_str(&format!("author: {}\n", yaml(&info.author)));
    }
    if !info.subject.is_empty() {
        md.push_str(&format!("subject: {}\n", yaml(&info.subject)));
    }
    if !info.created.is_empty() {
        md.push_str(&format!("created: {}\n", info.created));
    }
    md.push_str(&format!("pages: {}\n", pages.len()));
    md.push_str(&format!("source: {}\n", yaml(file_name)));
    md.push_str("---\n");

    let many = pages.len() > 1;
    let mut any = false;
    for (n, page) in laid.iter().enumerate() {
        let number = n + 1;
        if chosen.is_some_and(|c| !c.contains(&number)) {
            continue;
        }
        any = true;
        if many {
            md.push_str(&format!("\n<!-- page {number} -->\n"));
        }
        if page.is_empty() {
            md.push_str("<!-- no text on this page: a picture or a scan -->\n");
            continue;
        }
        let mut blocks: Vec<String> = Vec::new();
        let mut para = String::new();
        let mut i = 0;
        let flush = |para: &mut String, blocks: &mut Vec<String>| {
            if !para.is_empty() {
                blocks.push(std::mem::take(para));
            }
        };
        while i < page.len() {
            let line = &page[i];
            let prev = i.checked_sub(1).map(|p| &page[p]);

            // Tables: lines lined up in columns.
            let run = table_run(page, i, &|l: &Line| level(l).is_some());
            if run > 0 {
                flush(&mut para, &mut blocks);
                blocks.push(table(&page[i..i + run]).trim_end().to_string());
                i += run;
                continue;
            }

            let text = escape(&line.text());
            if let Some(h) = level(line).filter(|_| text.chars().count() <= 200) {
                flush(&mut para, &mut blocks);
                // A heading wrapped over two lines.
                let mut title = text;
                while i + 1 < page.len()
                    && level(&page[i + 1]) == Some(h)
                    && page[i + 1].y - page[i].y < 2.0 * page[i].size
                {
                    i += 1;
                    join(&mut title, &page[i].text());
                }
                blocks.push(format!("{} {title}", "#".repeat(h)));
                i += 1;
                continue;
            }
            if let Some(item) = bullet(&text) {
                flush(&mut para, &mut blocks);
                para = item;
                // Wrapped item lines are indented under the text.
                let indent = line.x0();
                while i + 1 < page.len() {
                    let next = &page[i + 1];
                    let close = next.y - page[i].y < 1.8 * next.size.max(line.size);
                    if close
                        && next.x0() > indent + 0.5 * line.size
                        && bullet(&next.text()).is_none()
                        && level(next).is_none()
                    {
                        i += 1;
                        join(&mut para, &next.text());
                    } else {
                        break;
                    }
                }
                flush(&mut para, &mut blocks);
                i += 1;
                continue;
            }
            // A whole line in bold, short: a label or a small heading.
            if line.bold && text.chars().count() <= 80 && !text.ends_with(['.', ',', ';']) {
                flush(&mut para, &mut blocks);
                blocks.push(format!("**{text}**"));
                i += 1;
                continue;
            }
            // Paragraphs end at a wider gap, a change of size, or an
            // indented first line after a full stop.
            // The right edge of the text around: a line ending well before
            // it closes its paragraph.
            let right = page[i.saturating_sub(3)..(i + 4).min(page.len())]
                .iter()
                .filter(|l| (l.x0() - line.x0()).abs() < 2.0 * line.size)
                .map(|l| l.words.last().map_or(0.0, |w| w.x1))
                .fold(0.0, f32::max);
            let new_para = prev.is_some_and(|p| {
                let gap = line.y - p.y;
                let p_right = p.words.last().map_or(0.0, |w| w.x1);
                let short = right > p.x0() && p_right - p.x0() < 0.7 * (right - p.x0());
                gap > 1.75 * line.size.max(p.size)
                    || gap < 0.0
                    || (line.size - p.size).abs() > 0.6
                    || (line.x0() > p.x0() + 1.0 * line.size
                        && p.text().ends_with(['.', ':', '!', '?']))
                    || p.bold
                    || short
            });
            if new_para {
                flush(&mut para, &mut blocks);
            }
            join(&mut para, &text);
            i += 1;
        }
        flush(&mut para, &mut blocks);
        let mut was_item = false;
        for b in blocks {
            let item = bullet(&b).is_some_and(|i| i == b);
            if !(item && was_item) {
                md.push('\n');
            }
            md.push_str(&b);
            md.push('\n');
            was_item = item;
        }
    }
    if !any {
        return Err(Error::Tool {
            tool: "joey",
            message: "no pages to convert (check the page numbers)".into(),
        });
    }
    Ok(dense_headings(&md))
}

/// Heading levels without holes: "#" and "####" become "#" and "##".
fn dense_headings(md: &str) -> String {
    let depth = |l: &str| {
        let n = l.chars().take_while(|c| *c == '#').count();
        (n > 0 && l[n..].starts_with(' ')).then_some(n)
    };
    let used: BTreeSet<usize> = md.lines().filter_map(depth).collect();
    let rank: HashMap<usize, usize> = used.iter().enumerate().map(|(i, d)| (*d, i + 1)).collect();
    let mut out = String::with_capacity(md.len());
    let mut front = 0;
    for l in md.lines() {
        if l == "---" && front < 2 {
            front += 1;
        }
        match depth(l).filter(|_| front >= 2) {
            Some(d) => {
                out.push_str(&"#".repeat(rank[&d]));
                out.push_str(&l[d..]);
            }
            None => out.push_str(l),
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(x0: f32, x1: f32, y: f32, size: f32, text: &str) -> Piece {
        Piece {
            x0,
            x1,
            y,
            size,
            bold: false,
            text: text.into(),
        }
    }

    fn sentence(y: f32, text: &str) -> Vec<Piece> {
        let mut x = 50.0;
        text.split(' ')
            .map(|w| {
                let w0 = x;
                x += w.len() as f32 * 5.0 + 3.0;
                word(w0, x - 3.0, y, 10.0, w)
            })
            .collect()
    }

    #[test]
    fn headings_paragraphs_lists_and_tables() {
        let mut page = vec![word(50.0, 200.0, 40.0, 20.0, "Report")];
        page.extend(sentence(
            70.0,
            "This is the first line of a paragraph that goes",
        ));
        page.extend(sentence(82.0, "on to a second line."));
        page.extend(sentence(110.0, "• First point"));
        page.extend(sentence(122.0, "• Second point"));
        for (n, y) in [(0, 150.0), (1, 164.0), (2, 178.0)] {
            let name = ["Region", "North", "South"][n];
            let a = ["July", "12,400", "7,300"][n];
            let b = ["August", "15,900", "7,950"][n];
            page.push(word(50.0, 90.0, y, 10.0, name));
            page.push(word(150.0, 190.0, y, 10.0, a));
            page.push(word(250.0, 290.0, y, 10.0, b));
        }
        let md = to_markdown(&[page], None, &Info::default(), "r.pdf").unwrap();
        assert!(md.contains("\n# Report\n"), "{md}");
        assert!(
            md.contains("This is the first line of a paragraph that goes on to a second line."),
            "{md}"
        );
        assert!(md.contains("- First point\n- Second point"), "{md}");
        assert!(
            md.contains(
                "| Region | July | August |\n| --- | --- | --- |\n| North | 12,400 | 15,900 |"
            ),
            "{md}"
        );
        assert!(
            md.starts_with("---\npages: 1\nsource: \"r.pdf\"\n---\n"),
            "{md}"
        );
    }

    #[test]
    fn glyphs_become_words_from_gaps() {
        // TeX-style: no space glyphs, words separated only by gaps.
        let mut page = Vec::new();
        let mut x = 50.0;
        for (i, c) in "Hello world".chars().enumerate() {
            if c == ' ' {
                x += 3.5;
                continue;
            }
            page.push(Piece {
                x0: x,
                x1: x + 5.0,
                y: 100.0,
                size: 10.0,
                bold: false,
                text: c.to_string(),
            });
            x += 5.0 + if i % 2 == 0 { 0.2 } else { 0.0 };
        }
        let l = lines(&page);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].text(), "Hello world");
    }

    #[test]
    fn running_headers_and_page_numbers_go() {
        let mut pages = Vec::new();
        for n in 1..=4 {
            let mut p = sentence(20.0, "Annual Report 2026");
            p.extend(sentence(
                100.0,
                &format!("Body text of page {n} ends here."),
            ));
            p.push(word(290.0, 300.0, 800.0, 10.0, &n.to_string()));
            pages.push(p);
        }
        let md = to_markdown(&pages, None, &Info::default(), "a.pdf").unwrap();
        assert!(!md.contains("Annual Report"), "{md}");
        assert!(md.contains("Body text of page 3 ends here."), "{md}");
        assert!(!md.lines().any(|l| l.trim() == "3"), "{md}");
        assert!(md.contains("<!-- page 2 -->"));
    }

    #[test]
    fn two_columns_read_left_then_right() {
        let mut page = Vec::new();
        for i in 0..8 {
            let y = 100.0 + i as f32 * 12.0;
            for (start, side) in [(50.0, "left"), (300.0, "right")] {
                let text = format!("{side} {} column line number {i}", "a".repeat(i + 1));
                let mut x = start;
                for w in text.split(' ') {
                    let w1 = x + w.len() as f32 * 4.0;
                    page.push(word(x, w1, y, 10.0, w));
                    x = w1 + 3.0;
                }
            }
        }
        let md = to_markdown(&[page], None, &Info::default(), "c.pdf").unwrap();
        let l7 = md.find("left aaaaaaaa column line number 7").expect(&md);
        let r0 = md.find("right a column line number 0").expect(&md);
        assert!(l7 < r0, "{md}");
        assert!(!md.contains('|'), "{md}");
    }

    #[test]
    fn scanned_pdf_says_so() {
        let err = to_markdown(&[Vec::new()], None, &Info::default(), "s.pdf").unwrap_err();
        assert!(err.to_string().contains("pictures"));
    }

    #[test]
    fn entities_and_ligatures() {
        assert_eq!(
            unescape("De&#xfb01;nition &lt;x&gt; &amp; f&#252;r"),
            "Definition <x> & für"
        );
    }

    #[test]
    fn page_numbers() {
        assert!(is_page_number("12"));
        assert!(is_page_number("- 3 -"));
        assert!(is_page_number("Page 4 of 10"));
        assert!(is_page_number("4 / 10"));
        assert!(!is_page_number("2026 results"));
    }
}
