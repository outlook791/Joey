//! File actions: things to do with files that aren't conversions, for any
//! kind of file, folders included (like Tangerine's Copy, Move, Copy Path,
//! Terminal and AirDrop — here KDE Connect, GSConnect and LocalSend).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use crate::i18n::{t, tf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    CopyPath,
    CopyTo,
    MoveTo,
    Terminal,
    Zip,
    /// Send to a phone or PC paired with KDE Connect (index in `phones()`).
    Phone(u8),
    /// Open LocalSend with the files, ready to pick a nearby device.
    LocalSend,
}

impl Action {
    /// Label in the interface language.
    pub fn title(self) -> String {
        match self {
            Action::CopyPath => t("Copy the path"),
            Action::CopyTo => t("Copy to…"),
            Action::MoveTo => t("Move to…"),
            Action::Terminal => t("Open a terminal here"),
            Action::Zip => t("Put in a ZIP"),
            Action::Phone(i) => {
                let name = phones()
                    .get(i as usize)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?");
                tf("Send to {device}", &[("device", &name)])
            }
            Action::LocalSend => t("Send with LocalSend…"),
        }
    }

    pub fn key(self) -> String {
        match self {
            Action::CopyPath => "copy-path".into(),
            Action::CopyTo => "copy-to".into(),
            Action::MoveTo => "move-to".into(),
            Action::Terminal => "terminal".into(),
            Action::Zip => "zip".into(),
            Action::Phone(i) => format!("phone-{i}"),
            Action::LocalSend => "localsend".into(),
        }
    }

    pub fn from_key(s: &str) -> Option<Action> {
        if let Some(i) = s.strip_prefix("phone-") {
            return i.parse().ok().map(Action::Phone);
        }
        [
            Action::CopyPath,
            Action::CopyTo,
            Action::MoveTo,
            Action::Terminal,
            Action::Zip,
            Action::LocalSend,
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
            Action::Phone(_) => "phone-symbolic",
            Action::LocalSend => "send-to-symbolic",
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
    // Like AirDrop: KDE Connect for paired phones and PCs, LocalSend for
    // any nearby device (Android, iPhone, Windows, Mac, Linux).
    for i in 0..phones().len().min(3) {
        v.push(Action::Phone(i as u8));
    }
    if localsend().is_some() {
        v.push(Action::LocalSend);
    }
    v
}

/// Runs a command and returns its output, giving up after `secs`.
fn output_within(mut cmd: Command, secs: u64) -> Option<String> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx.recv_timeout(Duration::from_secs(secs)).ok()?.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A phone or PC that files can be sent to now.
pub struct Device {
    pub id: String,
    pub name: String,
    via: Via,
}

#[derive(Clone, Copy, PartialEq)]
enum Via {
    /// KDE Connect, through its command.
    KdeConnect,
    /// GSConnect, KDE Connect for GNOME, through D-Bus.
    GsConnect,
}

const GSCONNECT: &str = "org.gnome.Shell.Extensions.GSConnect";

/// Phones and PCs paired with KDE Connect or GSConnect and reachable now.
pub fn phones() -> &'static [Device] {
    static PHONES: OnceLock<Vec<Device>> = OnceLock::new();
    PHONES.get_or_init(|| {
        let mut list = Vec::new();
        if let Some(bin) = joey_core::engine::which("kdeconnect-cli") {
            let mut cmd = Command::new(bin);
            cmd.args(["--list-available", "--id-name-only"]);
            list.extend(
                output_within(cmd, 2)
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|l| {
                        let (id, name) = l.trim().split_once(' ')?;
                        Some(Device {
                            id: id.to_string(),
                            name: name.trim().to_string(),
                            via: Via::KdeConnect,
                        })
                    }),
            );
        }
        list.extend(gsconnect_devices());
        list
    })
}

/// GSConnect's paired, connected devices, from its D-Bus objects.
fn gsconnect_devices() -> Vec<Device> {
    use gtk::gio;
    use gtk::glib;
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return Vec::new();
    };
    let Ok(reply) = bus.call_sync(
        Some(GSCONNECT),
        "/org/gnome/Shell/Extensions/GSConnect",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        None,
        Some(glib::VariantTy::new("(a{oa{sa{sv}}})").unwrap()),
        gio::DBusCallFlags::NO_AUTO_START,
        1500,
        gio::Cancellable::NONE,
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let objects = reply.child_value(0);
    for i in 0..objects.n_children() {
        let entry = objects.child_value(i);
        let path: String = entry.child_value(0).str().unwrap_or_default().to_string();
        let ifaces = entry.child_value(1);
        let props = (0..ifaces.n_children())
            .map(|j| ifaces.child_value(j))
            .find(|e| e.child_value(0).str() == Some("org.gnome.Shell.Extensions.GSConnect.Device"))
            .map(|e| glib::VariantDict::new(Some(&e.child_value(1))));
        let Some(props) = props else {
            continue;
        };
        let get_bool = |k: &str| {
            props
                .lookup_value(k, Some(glib::VariantTy::BOOLEAN))
                .and_then(|v| v.get::<bool>())
                .unwrap_or(false)
        };
        let name = props
            .lookup_value("Name", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>())
            .unwrap_or_else(|| "GSConnect".into());
        if get_bool("Connected") && get_bool("Paired") {
            out.push(Device {
                id: path,
                name,
                via: Via::GsConnect,
            });
        }
    }
    out
}

/// Sends files to a phone or PC. The transfer continues in KDE Connect or
/// GSConnect, which show their own progress.
pub fn send_to_phone(index: u8, files: &[PathBuf]) -> Result<String, String> {
    let d = phones().get(index as usize).ok_or("device not found")?;
    let ok = match d.via {
        Via::KdeConnect => {
            let bin =
                joey_core::engine::which("kdeconnect-cli").ok_or("KDE Connect isn't installed")?;
            Command::new(bin)
                .args(["--device", &d.id])
                .arg("--share")
                .args(files)
                .status()
                .map_err(|e| e.to_string())?
                .success()
        }
        Via::GsConnect => files.iter().all(|f| gsconnect_share(&d.id, f).is_ok()),
    };
    if ok {
        Ok(tf("Sent to {device}", &[("device", &d.name)]))
    } else {
        Err(tf("{device} didn't answer", &[("device", &d.name)]))
    }
}

/// Asks GSConnect to send one file (its "shareFile" device action).
fn gsconnect_share(device: &str, file: &Path) -> Result<(), String> {
    use gtk::gio::{self, prelude::*};
    use gtk::glib;
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|e| e.to_string())?;
    let uri = gio::File::for_path(file).uri().to_string();
    let param: glib::Variant = (uri, false).to_variant();
    let args = (
        "shareFile",
        vec![param],
        std::collections::HashMap::<String, glib::Variant>::new(),
    )
        .to_variant();
    bus.call_sync(
        Some(GSCONNECT),
        device,
        "org.gtk.Actions",
        "Activate",
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// How to start LocalSend: a program, or its Flatpak.
fn localsend() -> Option<Command> {
    for bin in ["localsend_app", "localsend"] {
        if let Some(p) = joey_core::engine::which(bin) {
            return Some(Command::new(p));
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let id = "org.localsend.localsend_app";
    let installed = Path::new("/var/lib/flatpak/app").join(id).exists()
        || home.join(".local/share/flatpak/app").join(id).exists();
    installed.then(|| {
        let mut c = Command::new("flatpak");
        c.args(["run", "--file-forwarding", id]);
        c
    })
}

/// Opens LocalSend on its Send page with the files already selected.
pub fn open_localsend(files: &[PathBuf]) -> Result<String, String> {
    let mut cmd = localsend().ok_or("LocalSend isn't installed")?;
    if cmd.get_program() == "flatpak" {
        cmd.arg("@@").args(files).arg("@@");
    } else {
        cmd.args(files);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(t("Opened in LocalSend: pick the device there"))
}

/// A terminal program and how to tell it the starting folder.
fn terminal() -> Option<(PathBuf, &'static str)> {
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
