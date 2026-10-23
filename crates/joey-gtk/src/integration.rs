//! Puts Joey into the file managers: a right-click entry everywhere it's
//! possible, and Alt+C where the file manager lets a shortcut be attached
//! to it (Nautilus, Thunar). Everything goes in the user's home folder and
//! is removed by `remove()`.
//!
//! Works both from a normal install and from the Flatpak: inside the
//! sandbox the entries call `flatpak run --file-forwarding`, which hands
//! Joey exactly the selected files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n::t;

pub const APP_ID: &str = "io.github.outlook791.Joey";
/// Script name for Nautilus and Caja: a single word, because their
/// shortcut file separates key and name with a space.
const SCRIPT: &str = "Joey";
const THUNAR_ID: &str = "joey-convert-1";
const MARK: &str = "Added by Joey";

type Setup = fn() -> std::io::Result<bool>;

pub struct Report {
    /// (file manager, has the Alt+C shortcut)
    pub done: Vec<(&'static str, bool)>,
}

fn home() -> PathBuf {
    // Not the XDG dirs: inside a Flatpak they point into the sandbox.
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

fn installed(bin: &str) -> bool {
    if in_flatpak() {
        // The sandbox can't see host programs; their config folders tell.
        let h = home();
        return match bin {
            "nautilus" => {
                h.join(".local/share/nautilus").exists() || h.join(".config/nautilus").exists()
            }
            "thunar" => h.join(".config/Thunar").exists(),
            "dolphin" => h.join(".config/dolphinrc").exists(),
            "nemo" => h.join(".local/share/nemo").exists() || h.join(".config/nemo").exists(),
            "caja" => h.join(".config/caja").exists(),
            _ => false,
        };
    }
    joey_core::engine::which(bin).is_some()
        || ["/usr/bin", "/usr/local/bin"]
            .iter()
            .any(|d| Path::new(d).join(bin).exists())
}

/// The command a file manager should run, with `files` standing for the
/// selected files (e.g. `"$@"` or `%F`).
fn command(files: &str) -> String {
    if in_flatpak() {
        return format!("flatpak run --file-forwarding {APP_ID} @@ {files} @@");
    }
    let local = home().join(".local/bin/joey-gtk");
    let exe = if local.exists() {
        local
    } else {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("joey-gtk"))
    };
    format!("\"{}\" {files}", exe.display())
}

fn write_exec(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn script() -> String {
    format!(
        "#!/bin/sh\n# {MARK}: converts the selected files.\nexec {}\n",
        command("\"$@\"")
    )
}

/// Adds `line` to a list file, replacing any older line that ends with
/// `name` (so running setup twice doesn't duplicate it).
fn set_line(path: &Path, matches: impl Fn(&str) -> bool, line: &str) -> std::io::Result<()> {
    let old = fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<&str> = old.lines().filter(|l| !matches(l)).collect();
    lines.push(line);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, lines.join("\n") + "\n")
}

fn remove_line(path: &Path, matches: impl Fn(&str) -> bool) {
    if let Ok(old) = fs::read_to_string(path) {
        let kept: Vec<&str> = old.lines().filter(|l| !matches(l)).collect();
        let _ = fs::write(path, kept.join("\n") + "\n");
    }
}

fn nautilus() -> std::io::Result<bool> {
    let h = home();
    write_exec(
        &h.join(".local/share/nautilus/scripts").join(SCRIPT),
        &script(),
    )?;
    set_line(
        &h.join(".config/nautilus/scripts-accels"),
        |l| l.split_whitespace().nth(1) == Some(SCRIPT),
        &format!("<Alt>c {SCRIPT}"),
    )?;
    Ok(true)
}

fn caja() -> std::io::Result<bool> {
    write_exec(&home().join(".config/caja/scripts").join(SCRIPT), &script())?;
    Ok(false)
}

fn thunar() -> std::io::Result<bool> {
    let dir = home().join(".config/Thunar");
    fs::create_dir_all(&dir)?;
    let uca = dir.join("uca.xml");
    let action = format!(
        "<action>\n\t<icon>{APP_ID}</icon>\n\t<name>{}</name>\n\t<unique-id>{THUNAR_ID}</unique-id>\n\t<command>{}</command>\n\t<description>{MARK}</description>\n\t<patterns>*</patterns>\n\t<image-files/>\n\t<audio-files/>\n\t<video-files/>\n\t<text-files/>\n\t<other-files/>\n</action>\n",
        xml_escape(&t("Convert with Joey")),
        xml_escape(&command("%F")),
    );
    let old = fs::read_to_string(&uca).unwrap_or_else(|_| {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into()
    });
    let cleaned = remove_thunar_action(&old);
    let new = match cleaned.rfind("</actions>") {
        Some(i) => format!("{}{action}{}", &cleaned[..i], &cleaned[i..]),
        None => format!("{cleaned}<actions>\n{action}</actions>\n"),
    };
    fs::write(&uca, new)?;
    set_line(
        &dir.join("accels.scm"),
        |l| l.contains(THUNAR_ID),
        &format!("(gtk_accel_path \"<Actions>/ThunarActions/uca-action-{THUNAR_ID}\" \"<Alt>c\")"),
    )?;
    Ok(true)
}

fn remove_thunar_action(xml: &str) -> String {
    let Some(id_at) = xml.find(&format!("<unique-id>{THUNAR_ID}</unique-id>")) else {
        return xml.to_string();
    };
    let start = xml[..id_at].rfind("<action>").unwrap_or(id_at);
    let end = xml[id_at..]
        .find("</action>")
        .map(|e| id_at + e + "</action>".len())
        .unwrap_or(id_at);
    let end = if xml[end..].starts_with('\n') {
        end + 1
    } else {
        end
    };
    format!("{}{}", &xml[..start], &xml[end..])
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn dolphin() -> std::io::Result<bool> {
    let entry = format!(
        "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;\nActions=convert;\nX-KDE-Priority=TopLevel\nX-Joey={MARK}\n\n[Desktop Action convert]\nName={}\nIcon={APP_ID}\nExec={}\n",
        t("Convert with Joey"),
        command("%F")
    );
    // Plasma 6 and Plasma 5 look in different places.
    write_exec(
        &home().join(".local/share/kio/servicemenus/joey.desktop"),
        &entry,
    )?;
    write_exec(
        &home().join(".local/share/kservices5/ServiceMenus/joey.desktop"),
        &entry,
    )?;
    Ok(false)
}

fn nemo() -> std::io::Result<bool> {
    let action = format!(
        "[Nemo Action]\nName={}\nComment={MARK}\nExec={}\nIcon-Name={APP_ID}\nSelection=notnone\nExtensions=nodirs;\nQuote=double\n",
        t("Convert with Joey"),
        command("%F")
    );
    let path = home().join(".local/share/nemo/actions/joey.nemo_action");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(path, action)?;
    Ok(false)
}

/// Sets Joey up in every file manager found. Safe to run again.
pub fn setup() -> Report {
    let mut done = Vec::new();
    let all: [(&str, &'static str, Setup); 5] = [
        ("nautilus", "Nautilus", nautilus),
        ("dolphin", "Dolphin", dolphin),
        ("thunar", "Thunar", thunar),
        ("nemo", "Nemo", nemo),
        ("caja", "Caja", caja),
    ];
    for (bin, name, f) in all {
        if installed(bin) {
            if let Ok(shortcut) = f() {
                done.push((name, shortcut));
            }
        }
    }
    Report { done }
}

/// True if Joey is already set up in at least one file manager.
pub fn is_set_up() -> bool {
    let h = home();
    [
        h.join(".local/share/nautilus/scripts").join(SCRIPT),
        h.join(".local/share/kio/servicemenus/joey.desktop"),
        h.join(".local/share/nemo/actions/joey.nemo_action"),
        h.join(".config/caja/scripts").join(SCRIPT),
    ]
    .iter()
    .any(|p| p.exists())
        || fs::read_to_string(h.join(".config/Thunar/uca.xml")).is_ok_and(|s| s.contains(THUNAR_ID))
}

/// Removes everything `setup` added.
pub fn remove() {
    let h = home();
    for p in [
        h.join(".local/share/nautilus/scripts").join(SCRIPT),
        h.join(".config/caja/scripts").join(SCRIPT),
        h.join(".local/share/kio/servicemenus/joey.desktop"),
        h.join(".local/share/kservices5/ServiceMenus/joey.desktop"),
        h.join(".local/share/nemo/actions/joey.nemo_action"),
    ] {
        let _ = fs::remove_file(p);
    }
    remove_line(&h.join(".config/nautilus/scripts-accels"), |l| {
        l.split_whitespace().nth(1) == Some(SCRIPT)
    });
    let uca = h.join(".config/Thunar/uca.xml");
    if let Ok(xml) = fs::read_to_string(&uca) {
        let _ = fs::write(&uca, remove_thunar_action(&xml));
    }
    remove_line(&h.join(".config/Thunar/accels.scm"), |l| {
        l.contains(THUNAR_ID)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thunar_action_is_replaced_not_duplicated() {
        let xml = "<actions>\n<action>\n\t<name>Other</name>\n\t<unique-id>1-1</unique-id>\n</action>\n<action>\n\t<unique-id>joey-convert-1</unique-id>\n</action>\n</actions>\n";
        let cleaned = remove_thunar_action(xml);
        assert!(!cleaned.contains("joey-convert-1"));
        assert!(cleaned.contains("<name>Other</name>"));
        assert!(cleaned.contains("</actions>"));
    }
}
