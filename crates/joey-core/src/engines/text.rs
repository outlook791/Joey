//! Built-in conversions that need no outside tool: subtitles to plain text
//! (just the spoken lines, no numbers or timings).

use std::path::PathBuf;

use crate::engine::{Engine, Step};
use crate::format::Format;
use crate::Error;

pub struct Text {
    edges: Vec<(Format, Format)>,
}

impl Text {
    pub fn new() -> Self {
        Self {
            edges: vec![(Format::Srt, Format::Txt), (Format::Vtt, Format::Txt)],
        }
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

/// Keeps only the lines people read on screen.
pub fn subtitles_to_text(src: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut skip_block = false;
    for raw in src.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            skip_block = false;
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        if skip_block || line == "WEBVTT" || line.starts_with("WEBVTT ") {
            continue;
        }
        if line.starts_with("NOTE") || line.starts_with("STYLE") || line.starts_with("REGION") {
            skip_block = true;
            continue;
        }
        if line.contains("-->") || line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Drop formatting tags like <i> or {\an8}.
        let mut clean = String::with_capacity(line.len());
        let mut depth = 0;
        for c in line.chars() {
            match c {
                '<' | '{' => depth += 1,
                '>' | '}' if depth > 0 => depth -= 1,
                _ if depth == 0 => clean.push(c),
                _ => {}
            }
        }
        let clean = clean.trim();
        if !clean.is_empty() {
            out.push(clean.to_string());
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n") + "\n"
}

impl Engine for Text {
    fn name(&self) -> &'static str {
        "text"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let bytes = std::fs::read(step.input)?;
        let src = String::from_utf8_lossy(&bytes);
        let out = step.output();
        std::fs::write(&out, subtitles_to_text(&src))?;
        Ok(vec![out])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_spoken_lines() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n<i>Hello</i> there\n\n2\n00:00:03,000 --> 00:00:04,000\nGeneral Kenobi\n";
        assert_eq!(subtitles_to_text(srt), "Hello there\n\nGeneral Kenobi\n");
        let vtt = "WEBVTT\n\nNOTE a comment\nmore\n\n00:01.000 --> 00:02.000\n{\\an8}Hop\n";
        assert_eq!(subtitles_to_text(vtt), "Hop\n");
    }
}
