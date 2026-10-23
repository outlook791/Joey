//! What Joey remembers between runs, as plain text in ~/.config/joey:
//! - `choices`: the last choice for each kind of file (`png jpg`,
//!   `pdf tool:merge-pdf`), so next time Alt+C, Enter is all it takes;
//! - `settings`: saved "Adjust" settings per target (`jpg quality=low`).

use std::collections::HashMap;
use std::path::PathBuf;

use joey_core::{Format, Options, Tool};

use crate::actions::Action;

/// One thing to do with a group of files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    To(Format),
    Tool(Tool),
    Action(Action),
}

impl Choice {
    fn key(self) -> String {
        match self {
            Choice::To(f) => f.extension().to_string(),
            Choice::Tool(t) => format!("tool:{}", t.key()),
            Choice::Action(a) => format!("action:{}", a.key()),
        }
    }

    fn parse(s: &str) -> Option<Choice> {
        if let Some(k) = s.strip_prefix("tool:") {
            return Tool::from_key(k).map(Choice::Tool);
        }
        if let Some(k) = s.strip_prefix("action:") {
            return Action::from_key(k).map(Choice::Action);
        }
        Format::parse(s).map(Choice::To)
    }
}

fn dir() -> PathBuf {
    gtk::glib::user_config_dir().join("joey")
}

fn read_map(name: &str) -> Vec<(String, String)> {
    std::fs::read_to_string(dir().join(name))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (a, b) = l.split_once(' ')?;
            Some((a.to_string(), b.to_string()))
        })
        .collect()
}

fn write_map(name: &str, mut lines: Vec<String>) {
    lines.sort();
    let _ = std::fs::create_dir_all(dir());
    // Losing a remembered choice is harmless; never bother the user.
    let _ = std::fs::write(dir().join(name), lines.join("\n") + "\n");
}

pub fn load() -> HashMap<Format, Choice> {
    read_map("choices")
        .into_iter()
        .filter_map(|(a, b)| Some((Format::parse(&a)?, Choice::parse(&b)?)))
        .collect()
}

pub fn save(map: &HashMap<Format, Choice>) {
    write_map(
        "choices",
        map.iter()
            .map(|(a, b)| format!("{} {}", a.extension(), b.key()))
            .collect(),
    );
}

pub fn load_settings() -> HashMap<Format, Options> {
    read_map("settings")
        .into_iter()
        .filter_map(|(a, b)| Some((Format::parse(&a)?, Options::from_text(&b))))
        .collect()
}

pub fn save_settings(map: &HashMap<Format, Options>) {
    write_map(
        "settings",
        map.iter()
            .map(|(a, b)| format!("{} {}", a.extension(), b.to_text()))
            .collect(),
    );
}
