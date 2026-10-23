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
    /// Keys of the system-wide "files on the clipboard" shortcut, if set.
    pub global: Option<String>,
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

/// Dolphin names service menu actions `servicemenu_<file>::<action>` and,
/// from Dolphin 26.04, lets them carry a shortcut saved with its other
/// shortcuts in `dolphinui.rc`.
const DOLPHIN_ACTION: &str = "servicemenu_joey.desktop::convert";

fn dolphin_ui_rc() -> PathBuf {
    home().join(".local/share/kxmlgui5/dolphin/dolphinui.rc")
}

/// Sets (or, with `None`, removes) the shortcut of Joey's action in a
/// Dolphin `dolphinui.rc`, keeping everything else in the file.
fn dolphin_shortcut(xml: Option<&str>, shortcut: Option<&str>) -> Option<String> {
    let line =
        shortcut.map(|s| format!("  <Action shortcut=\"{s}\" name=\"{DOLPHIN_ACTION}\"/>\n"));
    let Some(xml) = xml.filter(|x| x.contains("<gui")) else {
        // No file yet: a minimal one. Its low version makes Dolphin use its
        // built-in layout while keeping these shortcuts.
        let line = line?;
        return Some(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE gui SYSTEM \"kpartgui.dtd\">\n<gui name=\"dolphin\" version=\"1\">\n <ActionProperties scheme=\"Default\">\n{line} </ActionProperties>\n</gui>\n"
        ));
    };
    // Drop any earlier Joey line.
    let mut out: String = xml
        .lines()
        .filter(|l| !l.contains(DOLPHIN_ACTION))
        .map(|l| format!("{l}\n"))
        .collect();
    let Some(line) = line else { return Some(out) };
    if let Some(i) = out.find("</ActionProperties>") {
        out.insert_str(i, &line);
    } else if let Some(i) = out.find("<ActionProperties/>") {
        out.replace_range(
            i..i + "<ActionProperties/>".len(),
            &format!("<ActionProperties scheme=\"Default\">\n{line} </ActionProperties>"),
        );
    } else if let Some(i) = out.rfind("</gui>") {
        out.insert_str(
            i,
            &format!(" <ActionProperties scheme=\"Default\">\n{line} </ActionProperties>\n"),
        );
    }
    Some(out)
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
    let rc = dolphin_ui_rc();
    let old = fs::read_to_string(&rc).ok();
    if let Some(new) = dolphin_shortcut(old.as_deref(), Some("Alt+C")) {
        if let Some(dir) = rc.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&rc, new)?;
    }
    Ok(true)
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

/// PCManFM (LXDE) and PCManFM-Qt (LXQt) read the shared "file manager
/// actions" format.
fn pcmanfm() -> std::io::Result<bool> {
    let action = format!(
        "[Desktop Entry]\nType=Action\nName={}\nTooltip={MARK}\nIcon={APP_ID}\nProfiles=files;\n\n[X-Action-Profile files]\nMimeTypes=all/allfiles;\nSelectionCount=>0\nExec={}\n",
        t("Convert with Joey"),
        command("%F")
    );
    let path = home().join(".local/share/file-manager/actions/joey.desktop");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(path, action)?;
    Ok(false)
}

/// Sets Joey up in every file manager found. Safe to run again.
pub fn setup() -> Report {
    let mut done = Vec::new();
    let all: [(&str, &'static str, Setup); 7] = [
        ("nautilus", "Nautilus", nautilus),
        ("dolphin", "Dolphin", dolphin),
        ("thunar", "Thunar", thunar),
        ("nemo", "Nemo", nemo),
        ("caja", "Caja", caja),
        ("pcmanfm-qt", "PCManFM-Qt", pcmanfm),
        ("pcmanfm", "PCManFM", pcmanfm),
    ];
    for (bin, name, f) in all {
        if installed(bin) {
            if let Ok(shortcut) = f() {
                done.push((name, shortcut));
            }
        }
    }
    let global = global_shortcut().ok().flatten();
    Report { done, global }
}

const CLIP_ID: &str = "io.github.outlook791.Joey.Clipboard";

/// A hidden launcher that opens Joey with the files on the clipboard.
fn clipboard_launcher() -> std::io::Result<()> {
    let exec = if in_flatpak() {
        format!("flatpak run {APP_ID} --clipboard")
    } else {
        command("--clipboard")
    };
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Joey (copied files)\nName[it]=Joey (file copiati)\nComment={MARK}: open the copied files\nExec={exec}\nIcon={APP_ID}\nNoDisplay=true\nTerminal=false\n"
    );
    let path = home()
        .join(".local/share/applications")
        .join(format!("{CLIP_ID}.desktop"));
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(path, entry)
}

fn desktop_is(name: &str) -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_ascii_uppercase().split(':').any(|p| p == name))
        .unwrap_or(false)
}

/// System-wide Meta+C: opens Joey with the files last copied with Ctrl+C,
/// so the desktop and file managers without shortcuts work too.
/// KDE Plasma and GNOME; returns the keys set, if any.
fn global_shortcut() -> std::io::Result<Option<String>> {
    let h = home();
    if desktop_is("KDE") || h.join(".config/kglobalshortcutsrc").exists() {
        clipboard_launcher()?;
        let rc = h.join(".config/kglobalshortcutsrc");
        let old = fs::read_to_string(&rc).unwrap_or_default();
        let section = format!("[services][{CLIP_ID}.desktop]");
        let cleaned = remove_ini_section(&old, &section);
        let mut new = cleaned.trim_end().to_string();
        if !new.is_empty() {
            new.push_str("\n\n");
        }
        new.push_str(&format!("{section}\n_launch=Meta+C\n"));
        fs::write(&rc, new)?;
        // The shortcut service reads its settings when it starts.
        if let Some(q) = joey_core::engine::which("kquitapp6") {
            let _ = std::process::Command::new(q).arg("kglobalaccel").status();
        }
        return Ok(Some("Meta+C".into()));
    }
    if desktop_is("GNOME") {
        if let Some(gs) = joey_core::engine::which("gsettings") {
            clipboard_launcher()?;
            let base = "org.gnome.settings-daemon.plugins.media-keys";
            let path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/joey/";
            let out = std::process::Command::new(&gs)
                .args(["get", base, "custom-keybindings"])
                .output()?;
            let current = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !current.contains(path) {
                let inner = current
                    .trim_start_matches("@as")
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim();
                let list = if inner.is_empty() {
                    format!("['{path}']")
                } else {
                    format!("[{inner}, '{path}']")
                };
                std::process::Command::new(&gs)
                    .args(["set", base, "custom-keybindings", &list])
                    .status()?;
            }
            let key = format!("{base}.custom-keybinding:{path}");
            let cmd = if in_flatpak() {
                format!("flatpak run {APP_ID} --clipboard")
            } else {
                command("--clipboard")
            };
            for (k, v) in [
                ("name", "Joey".to_string()),
                ("command", cmd),
                ("binding", "<Super>c".to_string()),
            ] {
                std::process::Command::new(&gs)
                    .args(["set", &key, k, &v])
                    .status()?;
            }
            return Ok(Some("Super+C".into()));
        }
    }
    Ok(None)
}

/// Removes one `[section]` and its keys from an INI file.
fn remove_ini_section(ini: &str, section: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in ini.lines() {
        if line.trim_start().starts_with('[') {
            skipping = line.trim() == section;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// True if Joey is already set up in at least one file manager.
pub fn is_set_up() -> bool {
    let h = home();
    [
        h.join(".local/share/nautilus/scripts").join(SCRIPT),
        h.join(".local/share/kio/servicemenus/joey.desktop"),
        h.join(".local/share/nemo/actions/joey.nemo_action"),
        h.join(".config/caja/scripts").join(SCRIPT),
        h.join(".local/share/file-manager/actions/joey.desktop"),
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
        h.join(".local/share/file-manager/actions/joey.desktop"),
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
    let rc = dolphin_ui_rc();
    if let Ok(old) = fs::read_to_string(&rc) {
        if let Some(new) = dolphin_shortcut(Some(&old), None) {
            let _ = fs::write(&rc, new);
        }
    }
    let _ = fs::remove_file(
        h.join(".local/share/applications")
            .join(format!("{CLIP_ID}.desktop")),
    );
    let kde = h.join(".config/kglobalshortcutsrc");
    if let Ok(old) = fs::read_to_string(&kde) {
        let section = format!("[services][{CLIP_ID}.desktop]");
        if old.contains(&section) {
            let _ = fs::write(&kde, remove_ini_section(&old, &section));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dolphin_shortcut_is_added_replaced_and_removed() {
        let fresh = dolphin_shortcut(None, Some("Alt+C")).unwrap();
        assert!(fresh.contains("servicemenu_joey.desktop::convert") && fresh.contains("Alt+C"));

        let existing = "<gui name=\"dolphin\" version=\"42\">\n <MenuBar/>\n <ActionProperties scheme=\"Default\">\n  <Action shortcut=\"F4\" name=\"open_terminal\"/>\n </ActionProperties>\n</gui>\n";
        let once = dolphin_shortcut(Some(existing), Some("Alt+C")).unwrap();
        let twice = dolphin_shortcut(Some(&once), Some("Alt+C")).unwrap();
        assert_eq!(twice.matches(DOLPHIN_ACTION).count(), 1);
        assert!(twice.contains("open_terminal") && twice.contains("version=\"42\""));

        let removed = dolphin_shortcut(Some(&twice), None).unwrap();
        assert!(!removed.contains(DOLPHIN_ACTION) && removed.contains("open_terminal"));

        let no_props = "<gui name=\"dolphin\" version=\"42\">\n <MenuBar/>\n</gui>\n";
        let added = dolphin_shortcut(Some(no_props), Some("Alt+C")).unwrap();
        assert!(added.contains("<ActionProperties") && added.contains(DOLPHIN_ACTION));
    }

    #[test]
    fn ini_section_is_removed_cleanly() {
        let ini = "[a]\nx=1\n\n[services][io.github.outlook791.Joey.Clipboard.desktop]\n_launch=Meta+C\n[b]\ny=2\n";
        let out = remove_ini_section(
            ini,
            "[services][io.github.outlook791.Joey.Clipboard.desktop]",
        );
        assert_eq!(out, "[a]\nx=1\n\n[b]\ny=2\n");
    }

    #[test]
    fn thunar_action_is_replaced_not_duplicated() {
        let xml = "<actions>\n<action>\n\t<name>Other</name>\n\t<unique-id>1-1</unique-id>\n</action>\n<action>\n\t<unique-id>joey-convert-1</unique-id>\n</action>\n</actions>\n";
        let cleaned = remove_thunar_action(xml);
        assert!(!cleaned.contains("joey-convert-1"));
        assert!(cleaned.contains("<name>Other</name>"));
        assert!(cleaned.contains("</actions>"));
    }
}
