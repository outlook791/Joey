//! Remembers the last format chosen for each kind of file, so next time
//! it's already selected and Alt+C, Enter is all it takes.
//! Stored as plain lines like `png jpg` in ~/.config/joey/choices.

use std::collections::HashMap;
use std::path::PathBuf;

use joey_core::Format;

fn path() -> PathBuf {
    gtk::glib::user_config_dir().join("joey").join("choices")
}

pub fn load() -> HashMap<Format, Format> {
    std::fs::read_to_string(path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (a, b) = l.split_once(' ')?;
            Some((Format::parse(a)?, Format::parse(b)?))
        })
        .collect()
}

pub fn save(map: &HashMap<Format, Format>) {
    let mut lines: Vec<String> = map
        .iter()
        .map(|(a, b)| format!("{} {}", a.extension(), b.extension()))
        .collect();
    lines.sort();
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Losing a remembered choice is harmless; never bother the user.
    let _ = std::fs::write(p, lines.join("\n") + "\n");
}
