//! Inside the Flatpak, some programs Joey can use live outside the sandbox:
//! LibreOffice (too big to bundle), KDE Connect, LocalSend and terminals.
//! `bridge()` finds them on the computer and puts small launchers on PATH
//! that start them there through `flatpak-spawn --host`, so the rest of
//! Joey finds them like any installed program.
//!
//! Needs the Flatpak permission `--talk-name=org.freedesktop.Flatpak`.
//! Outside a Flatpak it does nothing.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Programs worth reaching on the computer, by name.
const PROGRAMS: &[&str] = &[
    "soffice",
    "libreoffice",
    "kdeconnect-cli",
    "localsend_app",
    "localsend",
    "xdg-terminal-exec",
    "ptyxis",
    "kgx",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
    "mate-terminal",
    "tilix",
    "qterminal",
    "lxterminal",
    "kitty",
    "alacritty",
];

/// Flatpak apps on the computer that stand in for a program: (app, as).
const APPS: &[(&str, &str)] = &[
    ("org.libreoffice.LibreOffice", "soffice"),
    ("org.localsend.localsend_app", "localsend_app"),
];

pub fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

/// Finds the programs on the computer and makes them reachable. Call once,
/// at the very start, before other threads exist (it changes PATH).
pub fn bridge() {
    // Joey Office, the optional LibreOffice add-on, comes first.
    let office = Path::new("/app/office/program");
    if office.join("soffice").exists() {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs = std::iter::once(office.to_path_buf()).chain(std::env::split_paths(&path));
        if let Ok(joined) = std::env::join_paths(dirs) {
            std::env::set_var("PATH", joined);
        }
    }
    if !in_flatpak() || crate::engine::which("flatpak-spawn").is_none() {
        return;
    }
    let Some(found) = ask_host() else {
        return;
    };
    let dir = launcher_dir();
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut made = Vec::new();
    for line in found.lines() {
        let mut parts = line.split_whitespace();
        let (kind, name) = (parts.next(), parts.next());
        let script = match (kind, name) {
            (Some("bin"), Some(bin)) if PROGRAMS.contains(&bin) => {
                if made.contains(&bin.to_string()) {
                    continue;
                }
                made.push(bin.to_string());
                (
                    bin.to_string(),
                    format!("exec flatpak-spawn --host --directory=\"$PWD\" {bin} \"$@\""),
                )
            }
            (Some("app"), Some(app)) => {
                let Some(&(_, as_name)) = APPS.iter().find(|(a, _)| *a == app) else {
                    continue;
                };
                // A program of the same name on the computer wins.
                if made.contains(&as_name.to_string()) {
                    continue;
                }
                made.push(as_name.to_string());
                let run = if as_name == "soffice" {
                    format!("flatpak run --command=soffice {app} \"$@\"")
                } else {
                    format!("flatpak run --file-forwarding {app} @@ \"$@\" @@")
                };
                (
                    as_name.to_string(),
                    format!("exec flatpak-spawn --host --directory=\"$PWD\" {run}"),
                )
            }
            _ => continue,
        };
        let path = dir.join(&script.0);
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = writeln!(f, "#!/bin/sh\n{}", script.1);
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    if made.is_empty() {
        return;
    }
    // Programs inside the Flatpak come first; the computer's fill the gaps.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    dirs.push(dir);
    if let Ok(joined) = std::env::join_paths(dirs) {
        std::env::set_var("PATH", joined);
    }
}

fn launcher_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("joey-host")
}

/// One trip to the computer: which of the programs and apps it has.
fn ask_host() -> Option<String> {
    let bins = PROGRAMS.join(" ");
    let apps: Vec<&str> = APPS.iter().map(|(a, _)| *a).collect();
    let script = format!(
        "for p in {bins}; do command -v \"$p\" >/dev/null 2>&1 && echo \"bin $p\"; done; \
         for a in {}; do flatpak info \"$a\" >/dev/null 2>&1 && echo \"app $a\"; done; true",
        apps.join(" ")
    );
    let child = Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx.recv_timeout(Duration::from_secs(3)).ok()?.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
