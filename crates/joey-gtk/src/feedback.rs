//! Feedback and problem reports. Nothing is ever sent by itself: Joey
//! writes the report, shows it, and the person decides — edit it, copy
//! it, or open it as a new issue on GitHub. File names, the home folder
//! and the user name are replaced before the text is even shown.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use joey_core::diag::{self, Failure};

use crate::i18n::t;
use crate::Ui;

pub const REPO: &str = "outlook791/Joey";
/// Where reports go by email, for people without a GitHub account. The
/// address is public (it's in the code), so use one made for this.
/// `None` hides the Email button.
pub const EMAIL: Option<&str> = None;
/// Longest issue text that still fits in a link browsers and GitHub accept.
const MAX_URL_BODY: usize = 6000;

/// One thing that went wrong in a run.
#[derive(Debug, Clone)]
pub struct Problem {
    pub file: PathBuf,
    /// "PDF → XLSX", "Tool: CompressPdf"…
    pub what: String,
    /// What kind of problem (E104…).
    pub code: diag::Code,
    /// The message the person saw.
    pub message: String,
    /// The error as the engine reported it.
    pub error: String,
    /// The external commands that failed meanwhile.
    pub failures: Vec<Failure>,
}

// ---- Crashes ----

static LAST_PANIC: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn crash_file() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("joey").join("crash.txt")
}

/// Remembers every crash: in memory for a failed job, on disk in case the
/// whole app goes down, so the next start can offer to report it.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let text = format!(
            "panic in thread '{}': {}\n\n{}",
            thread.name().unwrap_or("?"),
            info,
            std::backtrace::Backtrace::force_capture()
        );
        if let Ok(mut last) = LAST_PANIC.lock() {
            *last = Some(text.clone());
        }
        let f = crash_file();
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&f, &text);
        default(info);
    }));
}

/// The crash of a job that Joey survived (forgets the file on disk: the
/// person sees it now, not at the next start).
pub fn take_job_panic() -> Option<String> {
    let _ = std::fs::remove_file(crash_file());
    LAST_PANIC.lock().ok().and_then(|mut p| p.take())
}

/// A crash from the last time Joey ran, if it closed because of one.
/// Read once: it isn't offered again at the following start.
pub fn take_last_crash() -> Option<String> {
    let f = crash_file();
    let text = std::fs::read_to_string(&f).ok()?;
    let _ = std::fs::remove_file(&f);
    (!text.trim().is_empty()).then_some(text)
}

// ---- Report text ----

fn environment(ui: &Ui) -> String {
    let names = ui.joey.engine_names();
    diag::environment(&names)
        .into_iter()
        .map(|(k, v)| format!("- **{k}:** {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn size(p: &Path) -> String {
    match std::fs::metadata(p) {
        Ok(m) if m.is_dir() => "folder".into(),
        Ok(m) if m.len() < 1_048_576 => format!("{} KB", m.len().div_ceil(1024)),
        Ok(m) => format!("{:.1} MB", m.len() as f64 / 1_048_576.0),
        Err(_) => "?".into(),
    }
}

/// A report for problems in one run. Returns (title, text).
pub fn problem_report(ui: &Ui, problems: &[Problem]) -> (String, String) {
    let files: Vec<&Path> = problems.iter().map(|p| p.file.as_path()).collect();
    let mut body = String::new();
    body.push_str(&format!(
        "### {}\n\n{}\n\n",
        t("What were you doing?"),
        t("(optional) Write it here.")
    ));
    body.push_str("### Details\n\n");
    body.push_str(&environment(ui));
    body.push_str("\n\n### Problems\n");
    for (i, p) in problems.iter().enumerate() {
        body.push_str(&format!(
            "\n**{}. {}** ({}) · {}\n\n- Code: **{}** ({})\n- Shown: {}\n- Error: `{}`\n",
            i + 1,
            crate::file_name(&p.file),
            size(&p.file),
            p.what,
            p.code.id,
            p.code.title,
            p.message,
            p.error.replace('`', "'").replace('\n', " ⏎ ")
        ));
        for f in &p.failures {
            body.push_str(&format!(
                "\n```\n$ {}\n[{} · {}]\n{}\n```\n",
                f.command, f.tool, f.status, f.output
            ));
        }
    }
    let body = diag::scrub(&body, &files);
    let what = problems.first().map(|p| p.what.clone()).unwrap_or_default();
    let code = problems.first().map(|p| p.code.id).unwrap_or("E999");
    let title = if problems.len() == 1 {
        format!("{code}: {what}")
    } else {
        format!("{code}: {what} (+{} more)", problems.len() - 1)
    };
    (diag::scrub(&title, &files), body)
}

/// A report for a crash. Returns (title, text).
pub fn crash_report(ui: &Ui, crash: &str) -> (String, String) {
    let first = crash.lines().next().unwrap_or("crash").to_string();
    let body = format!(
        "### {}\n\n{}\n\n### Details\n\n{}\n\n### Crash\n\n```\n{}\n```\n",
        t("What were you doing?"),
        t("(optional) Write it here."),
        environment(ui),
        diag::tail(crash, 60)
    );
    let short: String = first.chars().take(90).collect();
    (
        diag::scrub(&format!("Crash: {short}"), &[]),
        diag::scrub(&body, &[]),
    )
}

/// A blank feedback text, with the system details below. Returns
/// (title, text).
pub fn idea_report(ui: &Ui) -> (String, String) {
    let body = format!(
        "### {}\n\n\n\n### Details\n\n{}\n",
        t("Your idea, request or opinion"),
        environment(ui)
    );
    (String::new(), diag::scrub(&body, &[]))
}

// ---- Window ----

/// Shows a report: editable, never sent by itself.
pub fn show(ui: &Rc<Ui>, heading: &str, title: &str, body: &str) {
    let win = adw::Window::builder()
        .modal(true)
        .transient_for(&ui.window)
        .title(heading)
        .default_width(560)
        .default_height(620)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(6);
    content.set_margin_bottom(14);
    content.set_margin_start(14);
    content.set_margin_end(14);

    let intro = gtk::Label::builder()
        .label(t(
            "Nothing is sent by itself. Read the text, change or remove what you want: your file names and your user name are already hidden.",
        ))
        .wrap(true)
        .xalign(0.0)
        .build();
    content.append(&intro);

    let title_row = gtk::Entry::builder()
        .text(title)
        .placeholder_text(t("Title, in a few words"))
        .build();
    content.append(&title_row);

    let text = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .monospace(true)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    text.buffer().set_text(body);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .vexpand(true)
        .css_classes(["card"])
        .build();
    content.append(&scroll);

    let status = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .label(t(
            "Sending opens GitHub in your browser, where you check it once more: a free account is needed. Without one, copy the text and send it as you prefer.",
        ))
        .build();
    content.append(&status);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let copy = gtk::Button::with_label(&t("Copy"));
    let mail = gtk::Button::with_label(&t("Send by email"));
    mail.add_css_class("pill");
    mail.set_visible(EMAIL.is_some());
    let send = gtk::Button::with_label(&t("Open on GitHub"));
    send.add_css_class("suggested-action");
    copy.add_css_class("pill");
    send.add_css_class("pill");
    buttons.append(&copy);
    buttons.append(&mail);
    buttons.append(&send);
    content.append(&buttons);

    view.set_content(Some(&content));
    win.set_content(Some(&view));

    let read = {
        let text = text.clone();
        let title_row = title_row.clone();
        move || {
            let b = text.buffer();
            let body = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
            (title_row.text().to_string(), body)
        }
    };
    let read = Rc::new(read);
    if let Some(address) = EMAIL {
        let read = read.clone();
        let win = win.clone();
        mail.connect_clicked(move |_| {
            let (title, body) = read();
            // Mail apps accept shorter links than browsers: the whole text
            // also goes on the clipboard.
            win.clipboard().set_text(&body);
            let mut short: String = body.chars().take(1800).collect();
            if short.len() < body.len() {
                short.push_str("\n\n[…] Ctrl+V");
            }
            let url = format!(
                "mailto:{address}?subject={}&body={}",
                diag::encode(&format!("[Joey] {title}")),
                diag::encode(&short)
            );
            gtk::UriLauncher::new(&url).launch(Some(&win), None::<&gtk::gio::Cancellable>, |_| {});
        });
    }
    {
        let read = read.clone();
        let status = status.clone();
        let win = win.clone();
        copy.connect_clicked(move |_| {
            let (title, body) = read();
            win.clipboard().set_text(&format!("{title}\n\n{body}"));
            status.set_text(&t("Copied"));
        });
    }
    {
        let read = read.clone();
        let status = status.clone();
        let win2 = win.clone();
        send.connect_clicked(move |_| {
            let (title, body) = read();
            let title = if title.trim().is_empty() {
                "Feedback".to_string()
            } else {
                title
            };
            let (url, cut) = diag::issue_url(REPO, &title, &body, MAX_URL_BODY);
            if cut {
                // The link holds only the beginning: the rest goes on the
                // clipboard, to paste on GitHub.
                win2.clipboard().set_text(&body);
                status.set_text(&t(
                    "The text is long: GitHub opens with the beginning. The whole text is copied: replace it with Ctrl+V.",
                ));
            }
            let win3 = win2.clone();
            let cut2 = cut;
            gtk::UriLauncher::new(&url).launch(
                Some(&win2),
                None::<&gtk::gio::Cancellable>,
                move |res| {
                    if res.is_ok() && !cut2 {
                        win3.close();
                    }
                },
            );
        });
    }
    win.present();
}
