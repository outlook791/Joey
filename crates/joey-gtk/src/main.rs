//! Joey's window: a small pocket-shaped selector.
//!
//! `joey-gtk FILE…` opens it with those files; without files it waits for
//! a drop. Numbers pick a format, Enter converts, Esc closes.

mod choices;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use joey_core::{estimate, Category, Format, Joey};

const APP_ID: &str = "io.github.outlook791.Joey";
/// Formats shown per group before "More formats…": fewer as groups add up,
/// so the pocket never outgrows the screen.
fn visible_per_group(groups: usize) -> usize {
    match groups {
        1 => 5,
        2 => 3,
        _ => 1,
    }
}

struct Group {
    from: Format,
    files: Vec<PathBuf>,
    bytes: u64,
    targets: Vec<Format>,
    selected: usize,
    expanded: bool,
}

#[derive(Default)]
struct State {
    groups: Vec<Group>,
    skipped: Vec<(String, String)>,
    busy: bool,
    choices: HashMap<Format, Format>,
}

impl Group {
    /// Indices of the targets on screen. The selected one is always there.
    fn shown(&self, limit: usize) -> Vec<usize> {
        if self.expanded || self.targets.len() <= limit {
            return (0..self.targets.len()).collect();
        }
        let mut v: Vec<usize> = (0..limit).collect();
        if self.selected >= limit {
            v[limit - 1] = self.selected;
        }
        v
    }

    fn has_more(&self, limit: usize) -> bool {
        !self.expanded && self.targets.len() > limit
    }
}

impl State {
    fn limit(&self) -> usize {
        visible_per_group(self.groups.len())
    }

    /// Rows that currently have a number key, in screen order.
    fn numbered_rows(&self) -> Vec<(usize, usize)> {
        let limit = self.limit();
        let mut rows = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for ti in g.shown(limit) {
                rows.push((gi, ti));
            }
        }
        rows.truncate(9);
        rows
    }

    fn file_count(&self) -> usize {
        self.groups.iter().map(|g| g.files.len()).sum()
    }
}

struct Ui {
    app: adw::Application,
    window: gtk::ApplicationWindow,
    stack: gtk::Stack,
    list: gtk::Box,
    progress: gtk::ProgressBar,
    progress_label: gtk::Label,
    done_icon: gtk::Image,
    done_title: gtk::Label,
    done_sub: gtk::Label,
    done_errors: gtk::Label,
    show_folder: gtk::Button,
    joey: Arc<Joey>,
    state: RefCell<State>,
    first_output: RefCell<Option<PathBuf>>,
    hovered: Cell<bool>,
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_startup(|_| load_css());
    app.connect_activate(|app| {
        build(app).window.present();
    });
    app.connect_open(|app, files, _| {
        let ui = build(app);
        add_files(&ui, files.iter().filter_map(|f| f.path()).collect());
        ui.window.present();
    });
    app.run()
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("a display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build(app: &adw::Application) -> Rc<Ui> {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Joey")
        .decorated(false)
        .resizable(false)
        .default_width(340)
        .build();
    window.add_css_class("joey");

    let pocket = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pocket.add_css_class("pocket");
    pocket.append(&stitch());

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .vhomogeneous(false)
        .interpolate_size(true)
        .build();

    // Empty: nothing selected yet.
    let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
    empty.add_css_class("empty-page");
    let drop_icon = gtk::Image::from_icon_name("document-send-symbolic");
    drop_icon.set_pixel_size(32);
    drop_icon.add_css_class("dim-label");
    empty.append(&drop_icon);
    let drop_title = gtk::Label::new(Some("Drop files here"));
    drop_title.add_css_class("title-4");
    empty.append(&drop_title);
    let choose = gtk::Button::with_label("Choose files…");
    choose.add_css_class("pill");
    choose.set_halign(gtk::Align::Center);
    choose.set_margin_top(6);
    empty.append(&choose);
    empty.append(&hint("Esc to close"));
    stack.add_named(&empty, Some("empty"));

    // List: the formats to pick from. Scrolls only if it would outgrow
    // the screen (many files of many kinds, groups expanded).
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(560)
        .build();
    stack.add_named(&scroller, Some("list"));

    // Progress.
    let progress_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    progress_page.add_css_class("progress-page");
    let progress_label = gtk::Label::new(Some("Converting…"));
    let progress = gtk::ProgressBar::new();
    progress_page.append(&progress_label);
    progress_page.append(&progress);
    stack.add_named(&progress_page, Some("progress"));

    // Done.
    let done = gtk::Box::new(gtk::Orientation::Vertical, 4);
    done.add_css_class("done-page");
    let done_icon = gtk::Image::from_icon_name("object-select-symbolic");
    done_icon.set_pixel_size(28);
    done_icon.add_css_class("success");
    let done_title = gtk::Label::new(None);
    done_title.add_css_class("title-4");
    let done_sub = gtk::Label::new(None);
    done_sub.add_css_class("dim-label");
    let done_errors = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .css_classes(["error"])
        .build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(10);
    let show_folder = gtk::Button::with_label("Show in folder");
    let close = gtk::Button::with_label("Close");
    show_folder.add_css_class("pill");
    close.add_css_class("pill");
    buttons.append(&show_folder);
    buttons.append(&close);
    for w in [
        done_icon.upcast_ref::<gtk::Widget>(),
        done_title.upcast_ref(),
        done_sub.upcast_ref(),
        done_errors.upcast_ref(),
        buttons.upcast_ref(),
    ] {
        done.append(w);
    }
    stack.add_named(&done, Some("done"));

    pocket.append(&stack);
    let handle = gtk::WindowHandle::new();
    handle.set_child(Some(&pocket));
    window.set_child(Some(&handle));

    let ui = Rc::new(Ui {
        app: app.clone(),
        window: window.clone(),
        stack,
        list,
        progress,
        progress_label,
        done_icon,
        done_title,
        done_sub,
        done_errors,
        show_folder: show_folder.clone(),
        joey: Arc::new(Joey::new()),
        state: RefCell::new(State {
            choices: choices::load(),
            ..Default::default()
        }),
        first_output: RefCell::new(None),
        hovered: Cell::new(false),
    });

    {
        let ui = ui.clone();
        choose.connect_clicked(move |_| choose_files(&ui));
    }
    {
        let w = window.clone();
        close.connect_clicked(move |_| w.close());
    }
    {
        let ui = ui.clone();
        show_folder.connect_clicked(move |_| {
            if let Some(p) = ui.first_output.borrow().clone() {
                gtk::FileLauncher::new(Some(&gio::File::for_path(p))).open_containing_folder(
                    Some(&ui.window),
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            }
        });
    }

    // Keys: digits pick, Enter converts, Esc closes. Captured before any
    // focused button sees them.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let ui = ui.clone();
        keys.connect_key_pressed(move |_, key, _, _| on_key(&ui, key));
    }
    window.add_controller(keys);

    // Dropping files anywhere on the pocket adds them.
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    {
        let ui = ui.clone();
        drop.connect_drop(move |_, value, _, _| {
            let Ok(list) = value.get::<gdk::FileList>() else {
                return false;
            };
            add_files(&ui, list.files().iter().filter_map(|f| f.path()).collect());
            true
        });
    }
    window.add_controller(drop);

    // Don't auto-close under the pointer of someone who's still looking.
    let motion = gtk::EventControllerMotion::new();
    {
        let ui = ui.clone();
        motion.connect_enter(move |_, _, _| ui.hovered.set(true));
    }
    window.add_controller(motion);

    ui.stack.set_visible_child_name("empty");
    ui
}

/// The pocket's opening: a dashed seam curving down in the middle.
fn stitch() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(24);
    area.set_draw_func(|area, cr, w, _h| {
        let c = area.color();
        cr.set_source_rgba(c.red().into(), c.green().into(), c.blue().into(), 0.35);
        cr.set_line_width(1.0);
        cr.set_dash(&[3.0, 4.0], 0.0);
        let (w, x0, y0, sag) = (w as f64, 18.0, 7.0, 10.0);
        // Quadratic curve through the middle, written as a cubic.
        let (cx, cy) = (w / 2.0, y0 + 2.0 * sag);
        let (x1, y1) = (w - x0, y0);
        cr.move_to(x0, y0);
        cr.curve_to(
            x0 + 2.0 / 3.0 * (cx - x0),
            y0 + 2.0 / 3.0 * (cy - y0),
            x1 + 2.0 / 3.0 * (cx - x1),
            y1 + 2.0 / 3.0 * (cy - y1),
            x1,
            y1,
        );
        let _ = cr.stroke();
    });
    area
}

fn hint(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("hint");
    l.add_css_class("dim-label");
    l
}

fn choose_files(ui: &Rc<Ui>) {
    let dialog = gtk::FileDialog::builder()
        .title("Choose files to convert")
        .build();
    let ui = ui.clone();
    dialog.open_multiple(
        Some(&ui.window.clone()),
        None::<&gio::Cancellable>,
        move |res| {
            let Ok(model) = res else { return };
            let files = (0..model.n_items())
                .filter_map(|i| model.item(i)?.downcast::<gio::File>().ok()?.path())
                .collect();
            add_files(&ui, files);
        },
    );
}

fn add_files(ui: &Rc<Ui>, files: Vec<PathBuf>) {
    {
        let mut st = ui.state.borrow_mut();
        if st.busy {
            return;
        }
        for file in files {
            if st.groups.iter().any(|g| g.files.contains(&file)) {
                continue;
            }
            let name = file_name(&file);
            let from = match ui.joey.detect(&file) {
                Ok(f) => f,
                Err(joey_core::Error::IsFolder(_)) => {
                    st.skipped
                        .push((name, "folders aren't supported yet".into()));
                    continue;
                }
                Err(joey_core::Error::NotFound(_)) => {
                    st.skipped.push((name, "not found".into()));
                    continue;
                }
                Err(_) => {
                    st.skipped.push((name, "file type not recognised".into()));
                    continue;
                }
            };
            let bytes = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            if let Some(g) = st.groups.iter_mut().find(|g| g.from == from) {
                g.files.push(file);
                g.bytes += bytes;
                continue;
            }
            let targets = ui.joey.suggested_targets(from);
            if targets.is_empty() {
                st.skipped
                    .push((name, format!("nothing to convert {from} to here")));
                continue;
            }
            let selected = st
                .choices
                .get(&from)
                .and_then(|c| targets.iter().position(|t| t == c))
                .unwrap_or(0);
            st.groups.push(Group {
                from,
                files: vec![file],
                bytes,
                targets,
                selected,
                expanded: false,
            });
        }
    }
    render(ui);
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn category_label(f: Format) -> &'static str {
    let _: Category = f.category();
    f.category().label()
}

fn render(ui: &Rc<Ui>) {
    let list = &ui.list;
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let st = ui.state.borrow();
    if st.groups.is_empty() && st.skipped.is_empty() {
        ui.stack.set_visible_child_name("empty");
        return;
    }

    let n = st.file_count();
    let summary = gtk::Label::builder()
        .label(if n == 1 {
            "1 file".to_string()
        } else {
            format!("{n} files")
        })
        .xalign(0.0)
        .css_classes(["summary", "dim-label"])
        .build();
    list.append(&summary);

    let numbered = st.numbered_rows();
    let limit = st.limit();
    for (gi, g) in st.groups.iter().enumerate() {
        let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        title.add_css_class("group-title");
        let name = if g.files.len() == 1 {
            file_name(&g.files[0])
        } else {
            format!("{} {} files", g.files.len(), g.from)
        };
        let name_label = gtk::Label::builder()
            .label(name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["dim-label"])
            .build();
        let kind_text = if g.from == Format::Pdf {
            "PDF · one image per page"
        } else {
            category_label(g.from)
        };
        let kind = gtk::Label::builder()
            .label(kind_text)
            .css_classes(["dim-label"])
            .build();
        title.append(&name_label);
        title.append(&kind);
        list.append(&title);

        for ti in g.shown(limit) {
            let key = numbered.iter().position(|&r| r == (gi, ti)).map(|i| i + 1);
            list.append(&format_row(ui, g, gi, ti, key));
        }
        if g.has_more(limit) {
            let hidden = g.targets.len() - g.shown(limit).len();
            let more = gtk::Button::with_label(&format!("{hidden} more formats…"));
            more.add_css_class("flat");
            more.add_css_class("more");
            let ui2 = ui.clone();
            more.connect_clicked(move |_| {
                ui2.state.borrow_mut().groups[gi].expanded = true;
                render(&ui2);
            });
            list.append(&more);
        }
    }

    for (name, why) in &st.skipped {
        let l = gtk::Label::builder()
            .label(format!("{name}: {why}"))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["skipped", "dim-label"])
            .build();
        list.append(&l);
    }

    if !st.groups.is_empty() {
        let convert = gtk::Button::new();
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inner.set_halign(gtk::Align::Center);
        inner.append(&gtk::Label::new(Some("Convert")));
        let enter = gtk::Label::new(Some("Enter"));
        enter.add_css_class("dim-label");
        enter.add_css_class("size");
        inner.append(&enter);
        convert.set_child(Some(&inner));
        convert.add_css_class("suggested-action");
        convert.add_css_class("convert");
        let ui2 = ui.clone();
        convert.connect_clicked(move |_| start(&ui2));
        list.append(&convert);
    }
    list.append(&hint("Esc to close"));
    ui.stack.set_visible_child_name("list");
}

fn format_row(ui: &Rc<Ui>, g: &Group, gi: usize, ti: usize, key: Option<usize>) -> gtk::Button {
    let to = g.targets[ti];
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let key_label = gtk::Label::new(key.map(|k| k.to_string()).as_deref());
    key_label.add_css_class("key");
    key_label.set_opacity(if key.is_some() { 1.0 } else { 0.0 });
    row.append(&key_label);
    row.append(
        &gtk::Label::builder()
            .label(to.display_name())
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );

    if let Some(s) = estimate::estimate(g.bytes, g.from, to).map(estimate::human) {
        let l = gtk::Label::new(Some(&s));
        l.add_css_class("size");
        l.add_css_class("dim-label");
        row.append(&l);
    }

    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("flat");
    button.add_css_class("fmt-row");
    if ti == g.selected {
        button.add_css_class("on");
    }
    let ui2 = ui.clone();
    button.connect_clicked(move |_| pick(&ui2, gi, ti));
    button
}

/// Selects a format. With a single kind of file, that's the whole job:
/// convert right away.
fn pick(ui: &Rc<Ui>, gi: usize, ti: usize) {
    let single = {
        let mut st = ui.state.borrow_mut();
        st.groups[gi].selected = ti;
        st.groups.len() == 1
    };
    if single {
        start(ui);
    } else {
        render(ui);
    }
}

fn on_key(ui: &Rc<Ui>, key: gdk::Key) -> glib::Propagation {
    if key == gdk::Key::Escape {
        ui.window.close();
        return glib::Propagation::Stop;
    }
    if ui.stack.visible_child_name().as_deref() != Some("list") {
        return glib::Propagation::Proceed;
    }
    if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
        start(ui);
        return glib::Propagation::Stop;
    }
    if let Some(d) = key.to_unicode().and_then(|c| c.to_digit(10)) {
        let target = ui
            .state
            .borrow()
            .numbered_rows()
            .get((d as usize).wrapping_sub(1))
            .copied();
        if let Some((gi, ti)) = target {
            pick(ui, gi, ti);
            return glib::Propagation::Stop;
        }
    }
    glib::Propagation::Proceed
}

fn start(ui: &Rc<Ui>) {
    let jobs: Vec<(PathBuf, Format)> = {
        let mut st = ui.state.borrow_mut();
        if st.busy || st.groups.is_empty() {
            return;
        }
        st.busy = true;
        let picks: Vec<(Format, Format)> = st
            .groups
            .iter()
            .map(|g| (g.from, g.targets[g.selected]))
            .collect();
        for (from, to) in picks {
            st.choices.insert(from, to);
        }
        choices::save(&st.choices);
        st.groups
            .iter()
            .flat_map(|g| {
                g.files
                    .iter()
                    .map(move |f| (f.clone(), g.targets[g.selected]))
            })
            .collect()
    };

    ui.stack.set_visible_child_name("progress");
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let total = jobs.len();
        let mut written = Vec::new();
        let mut errors = Vec::new();
        for (i, (file, to)) in jobs.into_iter().enumerate() {
            ui.progress_label.set_text(&if total == 1 {
                format!("Converting {}…", file_name(&file))
            } else {
                format!("Converting {} of {total}…", i + 1)
            });
            ui.progress.set_fraction(i as f64 / total as f64);
            let joey = ui.joey.clone();
            let f2 = file.clone();
            match gio::spawn_blocking(move || joey.convert(&f2, to, None)).await {
                Ok(Ok(out)) => written.extend(out),
                Ok(Err(e)) => errors.push(format!("{}: {}", file_name(&file), friendly(&e))),
                Err(_) => errors.push(format!("{}: unexpected error", file_name(&file))),
            }
        }
        ui.progress.set_fraction(1.0);
        finish(&ui, written, errors);
    });
}

/// Error text for people, not developers.
fn friendly(e: &joey_core::Error) -> String {
    match e {
        joey_core::Error::Tool { tool, .. } => format!("{tool} couldn't convert it"),
        other => other.to_string(),
    }
}

fn finish(ui: &Rc<Ui>, written: Vec<PathBuf>, errors: Vec<String>) {
    *ui.first_output.borrow_mut() = written.first().cloned();
    let n = written.len();
    let ok = errors.is_empty();
    ui.done_icon.set_icon_name(Some(if ok {
        "object-select-symbolic"
    } else {
        "dialog-warning-symbolic"
    }));
    ui.done_icon
        .set_css_classes(if ok { &["success"] } else { &["warning"] });
    ui.done_title.set_text(&match n {
        0 => "Nothing converted".to_string(),
        1 => "1 file ready".to_string(),
        n => format!("{n} files ready"),
    });
    ui.done_sub
        .set_text(if n > 0 { "Next to the originals" } else { "" });
    ui.done_sub.set_visible(n > 0);
    ui.done_errors.set_text(&errors.join("\n"));
    ui.done_errors.set_visible(!ok);
    ui.show_folder.set_visible(n > 0);
    ui.stack.set_visible_child_name("done");

    if n > 0 {
        let note = gio::Notification::new(&ui.done_title.text());
        note.set_body(Some("Next to the originals"));
        ui.app.send_notification(Some("done"), &note);
    }

    // All went well and nobody is looking: close by itself.
    if ok {
        let ui = ui.clone();
        glib::timeout_add_seconds_local_once(3, move || {
            if !ui.hovered.get() {
                ui.window.close();
            }
        });
    }
}
