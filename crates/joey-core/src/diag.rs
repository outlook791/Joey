//! What a bug report needs: the system Joey runs on, and the exact
//! command that failed with its output. Nothing here leaves the computer
//! by itself: the interface shows the report and the person decides
//! whether to send it.

use std::path::Path;
use std::sync::Mutex;

/// One external command that failed.
#[derive(Debug, Clone)]
pub struct Failure {
    pub tool: &'static str,
    pub command: String,
    pub status: String,
    /// The last lines it printed on stderr.
    pub output: String,
}

static FAILURES: Mutex<Vec<Failure>> = Mutex::new(Vec::new());

/// Keeps the last few failures, for the report.
pub(crate) fn record(f: Failure) {
    if let Ok(mut v) = FAILURES.lock() {
        if v.len() >= 20 {
            v.remove(0);
        }
        v.push(f);
    }
}

/// Failures since the last call (and forgets them).
pub fn take_failures() -> Vec<Failure> {
    FAILURES
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default()
}

/// The last `n` non-empty lines of `text`.
pub fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Shell-like rendering of a command line.
pub fn command_line(cmd: &std::process::Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    for a in cmd.get_args() {
        let a = a.to_string_lossy();
        // Embedded scripts are long and always the same: name them only.
        if a.len() > 200 {
            parts.push("<script>".into());
        } else if a.is_empty() || a.contains(|c: char| c.is_whitespace() || c == '\'') {
            parts.push(format!("'{}'", a.replace('\'', "'\\''")));
        } else {
            parts.push(a.into_owned());
        }
    }
    parts.join(" ")
}

/// Hides personal details: the home folder, the user name and the names
/// of the person's files (`files`), which become `file1`, `file2`…
pub fn scrub(text: &str, files: &[&Path]) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    scrub_with(text, files, &home, &user)
}

fn scrub_with(text: &str, files: &[&Path], home: &str, user: &str) -> String {
    let mut out = text.to_string();
    let mut stems: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .filter(|s| s.chars().count() >= 3)
        .collect();
    stems.sort();
    stems.dedup();
    // Longest first, so "photo" doesn't eat part of "photo-big".
    stems.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for (i, s) in stems.iter().enumerate() {
        out = out.replace(s.as_str(), &format!("file{}", i + 1));
    }
    if home.len() > 1 {
        out = out.replace(home, "~");
    }
    if user.len() >= 3 && user != "root" {
        out = out.replace(user, "user");
    }
    out
}

/// One line per fact about the system: version, distribution, desktop,
/// packaging, language and the conversion engines found.
pub fn environment(engines: &[&str]) -> Vec<(String, String)> {
    let os = ["/run/host/os-release", "/etc/os-release"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".into());
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "?".into());
    let packaging = if std::env::var_os("FLATPAK_ID").is_some() {
        "Flatpak"
    } else {
        "local install"
    };
    vec![
        ("Joey".into(), env!("CARGO_PKG_VERSION").into()),
        ("System".into(), format!("{os} · Linux {kernel}")),
        (
            "Desktop".into(),
            format!(
                "{} ({})",
                env("XDG_CURRENT_DESKTOP"),
                env("XDG_SESSION_TYPE")
            ),
        ),
        ("Package".into(), packaging.into()),
        ("Language".into(), env("LANG")),
        ("Engines".into(), engines.join(", ")),
    ]
}

/// Address of a new GitHub issue with title and text already filled in.
/// Browsers and GitHub refuse very long addresses, so the text is cut at
/// `max` bytes once encoded; the second value tells whether it was cut.
pub fn issue_url(repo: &str, title: &str, body: &str, max: usize) -> (String, bool) {
    let mut body = body.to_string();
    let mut cut = false;
    while encode(&body).len() > max {
        cut = true;
        let keep = body.len() * 9 / 10;
        let mut at = keep;
        while !body.is_char_boundary(at) {
            at -= 1;
        }
        body.truncate(at);
    }
    if cut {
        body.push_str("\n\n[…]");
    }
    (
        format!(
            "https://github.com/{repo}/issues/new?title={}&body={}",
            encode(title),
            encode(&body)
        ),
        cut,
    )
}

/// Percent-encoding for a URL query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---- Error codes ----

/// A kind of problem, with a stable code people can quote and search for,
/// what it means in plain words, and what to do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code {
    pub id: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
}

const fn code(id: &'static str, title: &'static str, hint: &'static str) -> Code {
    Code { id, title, hint }
}

/// Every code, in order. The README lists the same table.
pub const CODES: &[Code] = &[
    code("E101", "File not found", "It was moved or deleted after you selected it. Select it again."),
    code("E102", "This is a folder", "Select the files inside it, or use Put in a ZIP."),
    code("E103", "File type not recognised", "Joey reads what's inside the file, not its name, and this content isn't a format it knows."),
    code("E104", "Damaged or incomplete file", "Check that it opens in another app. If you downloaded it, download it again."),
    code("E105", "The file is protected by a password", "Remove the password first, then convert it."),
    code("E106", "Wrong password", "Check the password and try again."),
    code("E107", "The file has no sound", "This video has no audio track to extract."),
    code("E108", "No tables found in the PDF", "Only PDFs made by a computer have tables Joey can read; scanned pages are pictures."),
    code("E109", "The archive is empty", "There is nothing inside to extract."),
    code("E110", "Those pages don't exist", "Check the page numbers: the document has fewer pages."),
    code("E111", "Already as small as it gets", "Compressing it more wouldn't make it smaller."),
    code("E201", "Not possible on this computer", "A program Joey needs isn't installed. The README lists what to install for each kind of file."),
    code("E202", "A program Joey needs is missing", "Install it (the README lists them), then try again."),
    code("E203", "The installed program can't handle this format", "Your version of it lacks this format. A fuller version adds it, e.g. FFmpeg from RPM Fusion on Fedora."),
    code("E301", "The disk is full", "Free some space, or save in another place."),
    code("E302", "Joey isn't allowed to write here", "Copy the files to one of your folders (e.g. Documents) and convert them there."),
    code("E303", "Not enough memory", "The file is too big for the free memory. Close other apps, or convert fewer files at a time."),
    code("E304", "Blocked by a safety limit", "The image is larger than the limit set by ImageMagick's security policy."),
    code("E900", "Unexpected error", "This is a bug in Joey. Please report it."),
    code("E999", "The conversion program failed", "Please report it: the details tell us why."),
];

/// The code with this id.
pub fn code_for(id: &str) -> Code {
    CODES
        .iter()
        .copied()
        .find(|c| c.id == id)
        .unwrap_or(CODES[CODES.len() - 1])
}

/// What kind of problem this is, from the error and the output of the
/// commands that failed meanwhile.
pub fn classify(e: &crate::Error, failures: &[Failure]) -> Code {
    use crate::Error;
    match e {
        Error::NotFound(_) => return code_for("E101"),
        Error::IsFolder(_) => return code_for("E102"),
        Error::UnknownFormat(_) => return code_for("E103"),
        Error::NoRoute { .. } => return code_for("E201"),
        _ => {}
    }
    let mut text = e.to_string();
    if let Error::Io(io) = e {
        text.push_str(&format!(" {:?}", io.kind()));
    }
    for f in failures {
        text.push('\n');
        text.push_str(&f.status);
        text.push('\n');
        text.push_str(&f.output);
    }
    classify_text(&text)
}

/// Same as [`classify`], from text alone.
pub fn classify_text(text: &str) -> Code {
    let t = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    let id = if has(&["no space left", "storagefull", "disk quota"]) {
        "E301"
    } else if has(&[
        "permission denied",
        "read-only file system",
        "readonlyfilesystem",
        "operation not permitted",
    ]) {
        "E302"
    } else if has(&[
        "cannot allocate memory",
        "out of memory",
        "outofmemory",
        "memory allocation failed",
        "cache resources exhausted",
        "signal: 9",
        "sigkill",
    ]) {
        "E303"
    } else if has(&[
        "not authorized",
        "exceeds limit",
        "width or height exceeds",
        "resource limit",
    ]) {
        "E304"
    } else if has(&["wrong password", "invalid password", "incorrect password"]) {
        "E106"
    } else if has(&["password", "encrypted"]) {
        "E105"
    } else if has(&[
        "no audio track",
        "does not contain any stream",
        "matches no streams",
    ]) {
        "E107"
    } else if has(&["no tables found"]) {
        "E108"
    } else if has(&["archive is empty"]) {
        "E109"
    } else if has(&["check the page numbers"]) {
        "E110"
    } else if has(&["as small as it gets"]) {
        "E111"
    } else if has(&[
        "unknown encoder",
        "encoder not found",
        "decoder (codec) not found",
        "no decode delegate",
        "no encode delegate",
        "nodecodedelegate",
        "noencodedelegate",
        "not currently supported",
    ]) {
        "E203"
    } else if has(&["isn't installed", "didn't start"]) {
        "E202"
    } else if has(&[
        "damaged",
        "corrupt",
        "invalid data found",
        "moov atom not found",
        "couldn't read xref",
        "couldn't find trailer",
        "premature end",
        "unexpected end",
        "truncated",
        "improper image header",
        "not a jpeg file",
        "insufficient image data",
        "invalid pdf",
        "unexpectedeof",
        "invaliddata",
    ]) {
        "E104"
    } else if has(&["panic"]) {
        "E900"
    } else {
        "E999"
    };
    code_for(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_hides_names_and_home() {
        let f = Path::new("/home/mario/Documenti/Referto Rossi.pdf");
        let s = scrub_with(
            "gs -o '/home/mario/Documenti/.joey-1/Referto Rossi.png' '/home/mario/Documenti/Referto Rossi.pdf'",
            &[f],
            "/home/mario",
            "mario",
        );
        assert!(!s.contains("Rossi") && !s.contains("mario"), "{s}");
        assert!(s.contains("~/Documenti/file1.pdf"), "{s}");
    }

    #[test]
    fn url_is_encoded_and_cut() {
        let (url, cut) = issue_url("a/b", "x y", "è&", 1000);
        assert!(!cut);
        assert!(url.ends_with("title=x%20y&body=%C3%A8%26"), "{url}");
        let long = "é".repeat(5000);
        let (url, cut) = issue_url("a/b", "t", &long, 2000);
        assert!(cut && url.len() < 2100);
    }

    #[test]
    fn codes_are_unique_and_classified() {
        let mut ids: Vec<_> = CODES.iter().map(|c| c.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), CODES.len());
        assert_eq!(
            classify_text("Syntax Error: Couldn't read xref table").id,
            "E104"
        );
        assert_eq!(classify_text("moov atom not found").id, "E104");
        assert_eq!(
            classify_text("write error: No space left on device").id,
            "E301"
        );
        assert_eq!(
            classify_text("magick: cache resources exhausted").id,
            "E303"
        );
        assert_eq!(classify_text("[ffmpeg · signal: 9 (SIGKILL)]").id, "E303");
        assert_eq!(classify_text("Unknown encoder 'libx264'").id, "E203");
        assert_eq!(classify_text("qpdf: invalid password").id, "E106");
        assert_eq!(classify_text("something odd").id, "E999");
    }

    #[test]
    fn command_line_quotes() {
        let mut c = std::process::Command::new("magick");
        c.args(["a b.png", "-quality", "80", "out.jpg"]);
        assert_eq!(command_line(&c), "magick 'a b.png' -quality 80 out.jpg");
    }
}
