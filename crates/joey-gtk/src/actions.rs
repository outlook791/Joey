//! File actions: things to do with files that aren't conversions, for any
//! kind of file, folders included (like Tangerine's Copy, Move, Copy Path
//! and Terminal).

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    CopyPath,
    CopyTo,
    MoveTo,
    Terminal,
    Zip,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::CopyPath => "Copy the path",
            Action::CopyTo => "Copy to…",
            Action::MoveTo => "Move to…",
            Action::Terminal => "Open a terminal here",
            Action::Zip => "Put in a ZIP",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Action::CopyPath => "copy-path",
            Action::CopyTo => "copy-to",
            Action::MoveTo => "move-to",
            Action::Terminal => "terminal",
            Action::Zip => "zip",
        }
    }

    pub fn from_key(s: &str) -> Option<Action> {
        [
            Action::CopyPath,
            Action::CopyTo,
            Action::MoveTo,
            Action::Terminal,
            Action::Zip,
        ]
        .into_iter()
        .find(|a| a.key() == s)
    }

    pub fn icon(self) -> &'static str {
        match self {
            Action::CopyPath => "edit-copy-symbolic",
            Action::CopyTo => "folder-copy-symbolic",
            Action::MoveTo => "folder-move-symbolic",
            Action::Terminal => "utilities-terminal-symbolic",
            Action::Zip => "package-x-generic-symbolic",
        }
    }

    /// True when the action asks for a destination folder first.
    pub fn needs_folder(self) -> bool {
        matches!(self, Action::CopyTo | Action::MoveTo)
    }
}

/// Actions for a group of files. `zip` adds "Put in a ZIP" (groups of
/// convertible files have it as a quick tool already).
pub fn available(zip: bool) -> Vec<Action> {
    let mut v = vec![Action::CopyPath, Action::CopyTo, Action::MoveTo];
    if terminal().is_some() {
        v.push(Action::Terminal);
    }
    if zip {
        v.push(Action::Zip);
    }
    v
}

/// A terminal program and how to tell it the starting folder.
fn terminal() -> Option<(PathBuf, &'static str)> {
    if std::env::var_os("FLATPAK_ID").is_some() {
        return None; // the sandbox can't start host programs
    }
    let known: [(&str, &str); 12] = [
        ("xdg-terminal-exec", ""),
        ("ptyxis", "--working-directory="),
        ("kgx", "--working-directory="),
        ("gnome-terminal", "--working-directory="),
        ("konsole", "--workdir="),
        ("xfce4-terminal", "--working-directory="),
        ("mate-terminal", "--working-directory="),
        ("tilix", "--working-directory="),
        ("qterminal", "--workdir="),
        ("lxterminal", "--working-directory="),
        ("kitty", "--directory="),
        ("alacritty", "--working-directory="),
    ];
    known
        .iter()
        .find_map(|(bin, flag)| joey_core::engine::which(bin).map(|p| (p, *flag)))
}

/// Opens a terminal in the folder of `path` (or in it, if it's a folder).
pub fn open_terminal(path: &Path) -> std::io::Result<()> {
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let Some((bin, flag)) = terminal() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no terminal found",
        ));
    };
    let mut cmd = Command::new(bin);
    cmd.current_dir(&dir);
    if !flag.is_empty() {
        cmd.arg(format!("{flag}{}", dir.display()));
    }
    cmd.spawn().map(|_| ())
}

/// Copies or moves files and folders into `dest`, never overwriting:
/// a clash becomes `name (1).ext`. Returns the new paths.
pub fn copy_or_move(files: &[PathBuf], dest: &Path, moving: bool) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for f in files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = free_name(dest, &name);
        let result = if moving {
            std::fs::rename(f, &target)
                .or_else(|_| copy_any(f, &target).and_then(|_| remove_any(f)))
        } else {
            copy_any(f, &target)
        };
        result.map_err(|e| format!("{name}: {e}"))?;
        out.push(target);
    }
    Ok(out)
}

fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("a free name exists")
}

fn copy_any(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_any(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_move_never_overwrite() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let a = src.path().join("a.txt");
        std::fs::write(&a, b"1").unwrap();
        std::fs::write(dst.path().join("a.txt"), b"old").unwrap();
        let copied = copy_or_move(std::slice::from_ref(&a), dst.path(), false).unwrap();
        assert_eq!(copied[0].file_name().unwrap(), "a (1).txt");
        assert!(a.exists());
        let moved = copy_or_move(std::slice::from_ref(&a), dst.path(), true).unwrap();
        assert_eq!(moved[0].file_name().unwrap(), "a (2).txt");
        assert!(!a.exists());
        assert_eq!(std::fs::read(dst.path().join("a.txt")).unwrap(), b"old");
    }
}
