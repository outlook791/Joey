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
    fn command_line_quotes() {
        let mut c = std::process::Command::new("magick");
        c.args(["a b.png", "-quality", "80", "out.jpg"]);
        assert_eq!(command_line(&c), "magick 'a b.png' -quality 80 out.jpg");
    }
}
