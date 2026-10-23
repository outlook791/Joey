//! Joey's window: a small pocket-shaped selector.
//!
//! `joey-gtk FILE…` opens it with those files; without files it waits for
//! a drop. Numbers pick, Enter converts, Shift+number adjusts, Esc closes.
//! `joey-gtk --setup-file-managers` adds Joey to the file managers;
//! `--remove-file-managers` takes it out again.

mod actions;
mod adjust;
mod ask;
mod choices;
mod i18n;
mod integration;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use actions::Action;
use adw::prelude::*;
use choices::Choice;
use gtk::{gdk, gio, glib};
use i18n::{t, tf};
use joey_core::{estimate, Format, Joey, Options, Tool, ToolParams};

const APP_ID: &str = integration::APP_ID;

/// Formats shown per group before "more…": fewer as groups add up, so
/// the pocket never outgrows the screen.
fn visible_per_group(groups: usize) -> usize {
    match groups {
        1 => 4,
        2 => 3,
        _ => 1,
    }
}

pub struct Group {
    /// Placeholder for `other` groups, which have no format.
    pub from: Format,
    /// Folders and files Joey can't convert: file actions only.
    pub other: bool,
    pub files: Vec<PathBuf>,
    pub bytes: u64,
    /// Formats first, then quick tools, then file actions.
    pub items: Vec<Choice>,
    pub n_formats: usize,
    pub n_tools: usize,
    pub selected: usize,
    pub expanded: bool,
    pub options: Options,
    /// Answers for a tool that asks something (trim, pages…).
    pub params: ToolParams,
}

impl Group {
    pub fn choice(&self) -> Choice {
        self.items[self.selected]
    }

    pub fn name(&self) -> String {
        if self.files.len() == 1 {
            file_name(&self.files[0])
        } else if self.other {
            tf("{n} other items", &[("n", &self.files.len())])
        } else {
            tf(
                "{n} {fmt} files",
                &[("n", &self.files.len()), ("fmt", &self.from)],
            )
        }
    }

    /// Indices of the items on screen. The selected one is always there.
    fn shown(&self, limit: usize, single: bool) -> Vec<usize> {
        if self.expanded || self.other {
            return (0..self.items.len()).collect();
        }
        let mut v: Vec<usize> = (0..self.n_formats.min(limit)).collect();
        if single {
            // A few quick tools; the rest and the file actions under "more".
            let tools = self.n_formats..self.n_formats + self.n_tools.min(3);
            v.extend(tools);
        }
        if !v.contains(&self.selected) {
            if let Some(last) = v.iter().rposition(|&i| i < self.n_formats) {
                v[last] = self.selected;
            } else {
                v.push(self.selected);
            }
            v.sort();
        }
        v
    }
}

#[derive(Default)]
pub struct State {
    pub groups: Vec<Group>,
    skipped: Vec<(String, String)>,
    busy: bool,
    choices: HashMap<Format, Choice>,
    pub settings: HashMap<Format, Options>,
    /// "Remember these settings" is ticked on the Adjust screen.
    pub remember: bool,
    /// Folder chosen for Copy to… / Move to…, used by the next run.
    dest: Option<PathBuf>,
}

impl State {
    fn limit(&self) -> usize {
        visible_per_group(self.groups.len())
    }

    fn single(&self) -> bool {
        self.groups.len() == 1
    }

    /// Rows that currently have a number key, in screen order.
    fn numbered_rows(&self) -> Vec<(usize, usize)> {
        let (limit, single) = (self.limit(), self.single());
        let mut rows = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for i in g.shown(limit, single) {
                rows.push((gi, i));
            }
        }
        rows.truncate(9);
        rows
    }

    fn file_count(&self) -> usize {
        self.groups.iter().map(|g| g.files.len()).sum()
    }

    /// Saved settings for a target, or defaults.
    pub fn options_for(&self, choice: Choice) -> Options {
        match choice {
            Choice::To(f) => self.settings.get(&f).cloned().unwrap_or_default(),
            _ => Options::default(),
        }
    }
}

pub struct Ui {
    app: adw::Application,
    pub window: gtk::ApplicationWindow,
    pub stack: gtk::Stack,
    list: gtk::Box,
    pub adjust: gtk::Box,
    empty_setup: gtk::Button,
    empty_setup_result: gtk::Label,
    progress: gtk::ProgressBar,
    progress_label: gtk::Label,
    done_icon: gtk::Image,
    done_title: gtk::Label,
    done_sub: gtk::Label,
    done_errors: gtk::Label,
    show_folder: gtk::Button,
    joey: Arc<Joey>,
    pub state: RefCell<State>,
    first_output: RefCell<Option<PathBuf>>,
    hovered: Cell<bool>,
    /// Bumped when leaving the done screen, so a pending auto-close from
    /// an earlier conversion doesn't shut the window.
    run: Cell<u32>,
}

fn main() -> glib::ExitCode {
    // Plain command-line actions, no window.
    match std::env::args().nth(1).as_deref() {
        Some("--setup-file-managers") => {
            print_report(&integration::setup());
            return glib::ExitCode::SUCCESS;
        }
        Some("--remove-file-managers") => {
            integration::remove();
            println!("Joey removed from the file managers.");
            return glib::ExitCode::SUCCESS;
        }
        _ => {}
    }

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

fn report_lines(r: &integration::Report) -> Vec<String> {
    if r.done.is_empty() {
        return vec![t("No supported file manager found")];
    }
    let mut lines: Vec<String> = r
        .done
        .iter()
        .map(|(fm, shortcut)| {
            if *shortcut {
                tf("Alt+C is ready in {fm}", &[("fm", fm)])
            } else {
                tf(
                    "Right-click → Convert with Joey works in {fm}",
                    &[("fm", fm)],
                )
            }
        })
        .collect();
    lines.push(t("Restart the file manager once to use it"));
    lines
}

fn print_report(r: &integration::Report) {
    for l in report_lines(r) {
        println!("{l}");
    }
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
        .icon_name(APP_ID)
        .decorated(false)
        .resizable(false)
        .default_width(400)
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
    let drop_icon = gtk::Image::from_icon_name(APP_ID);
    drop_icon.set_pixel_size(48);
    empty.append(&drop_icon);
    let drop_title = gtk::Label::new(Some(&t("Drop files here")));
    drop_title.add_css_class("title-4");
    empty.append(&drop_title);
    let choose = gtk::Button::with_label(&t("Choose files…"));
    choose.add_css_class("pill");
    choose.set_halign(gtk::Align::Center);
    choose.set_margin_top(6);
    empty.append(&choose);
    let empty_setup = gtk::Button::with_label(&t("Set up Alt+C in your file manager"));
    empty_setup.add_css_class("flat");
    empty_setup.add_css_class("more");
    empty_setup.set_tooltip_text(Some(&t("Select files, press Alt+C, pick a format")));
    empty_setup.set_visible(!integration::is_set_up());
    empty.append(&empty_setup);
    let empty_setup_result = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .css_classes(["hint"])
        .visible(false)
        .build();
    empty.append(&empty_setup_result);
    empty.append(&hint(&t("Esc to close")));
    stack.add_named(&empty, Some("empty"));

    // List: what to do with the files. Scrolls only if it would outgrow
    // the screen.
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    stack.add_named(&scroller(&list), Some("list"));

    // Adjust: settings for the chosen conversions.
    let adjust = gtk::Box::new(gtk::Orientation::Vertical, 10);
    adjust.add_css_class("adjust-page");
    stack.add_named(&scroller(&adjust), Some("adjust"));

    // Progress.
    let progress_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    progress_page.add_css_class("progress-page");
    let progress_label = gtk::Label::builder()
        .label(t("Converting…"))
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    let progress = gtk::ProgressBar::new();
    progress_page.append(&progress_label);
    progress_page.append(&progress);
    stack.add_named(&progress_page, Some("progress"));

    // Done.
    let done = gtk::Box::new(gtk::Orientation::Vertical, 4);
    done.add_css_class("done-page");
    let done_icon = gtk::Image::from_icon_name("object-select-symbolic");
    done_icon.set_pixel_size(28);
    let done_title = gtk::Label::new(None);
    done_title.add_css_class("title-4");
    let done_sub = gtk::Label::new(None);
    done_sub.add_css_class("dim-label");
    let done_errors = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .css_classes(["error", "skipped"])
        .build();
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(10);
    let back_menu = gtk::Button::with_label(&t("Back to the menu"));
    let close = gtk::Button::with_label(&t("Close"));
    back_menu.add_css_class("pill");
    close.add_css_class("pill");
    buttons.append(&back_menu);
    buttons.append(&close);
    let links = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    links.set_halign(gtk::Align::Center);
    links.set_margin_top(4);
    let show_folder = gtk::Button::with_label(&t("Show in folder"));
    let new_files = gtk::Button::with_label(&t("Other files"));
    for b in [&show_folder, &new_files] {
        b.add_css_class("flat");
        b.add_css_class("more");
        links.append(b);
    }
    for w in [
        done_icon.upcast_ref::<gtk::Widget>(),
        done_title.upcast_ref(),
        done_sub.upcast_ref(),
        done_errors.upcast_ref(),
        buttons.upcast_ref(),
        links.upcast_ref(),
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
        adjust,
        empty_setup: empty_setup.clone(),
        empty_setup_result,
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
            settings: choices::load_settings(),
            ..Default::default()
        }),
        first_output: RefCell::new(None),
        hovered: Cell::new(false),
        run: Cell::new(0),
    });

    {
        // Same files, choose something else (or add more files).
        let ui = ui.clone();
        back_menu.connect_clicked(move |_| back_to_menu(&ui, false));
    }
    {
        // Start over with other files.
        let ui = ui.clone();
        new_files.connect_clicked(move |_| back_to_menu(&ui, true));
    }

    {
        let ui = ui.clone();
        choose.connect_clicked(move |_| choose_files(&ui));
    }
    {
        let ui = ui.clone();
        empty_setup.connect_clicked(move |_| {
            let lines = report_lines(&integration::setup());
            ui.empty_setup.set_visible(false);
            ui.empty_setup_result.set_text(&lines.join("\n"));
            ui.empty_setup_result.set_visible(true);
        });
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

    // Keys: digits pick, Shift+digit adjusts, Enter converts, Esc closes or
    // goes back. Captured before any focused button sees them.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let ui = ui.clone();
        keys.connect_key_pressed(move |_, key, code, mods| on_key(&ui, key, code, mods));
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

fn scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(child)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(580)
        .build()
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

pub fn hint(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("hint");
    l.add_css_class("dim-label");
    l
}

fn choose_files(ui: &Rc<Ui>) {
    let dialog = gtk::FileDialog::builder()
        .title(t("Choose files to convert"))
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

/// Inside the Flatpak, files from the file manager arrive through the
/// document portal (`/run/user/…/doc/…`). Results must go next to the real
/// file, so ask the portal where it lives (Joey can reach it directly).
fn real_path(p: PathBuf) -> PathBuf {
    if !p.starts_with("/run/user") || !p.components().any(|c| c.as_os_str() == "doc") {
        return p;
    }
    match xattr::get(&p, "user.document-portal.host-path") {
        Ok(Some(raw)) => {
            let host = PathBuf::from(
                String::from_utf8_lossy(&raw)
                    .trim_end_matches('\0')
                    .to_string(),
            );
            if host.exists() {
                host
            } else {
                p
            }
        }
        _ => p,
    }
}

fn add_files(ui: &Rc<Ui>, files: Vec<PathBuf>) {
    let files: Vec<PathBuf> = files.into_iter().map(real_path).collect();
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
                Ok(f) => Some(f),
                Err(joey_core::Error::NotFound(_)) => {
                    st.skipped.push((name, t("not found")));
                    continue;
                }
                // Folders and unknown files still get the file actions.
                Err(_) => None,
            };
            let bytes = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            let Some(from) = from else {
                push_other(&mut st, file);
                continue;
            };
            if let Some(g) = st.groups.iter_mut().find(|g| !g.other && g.from == from) {
                g.files.push(file);
                g.bytes += bytes;
                continue;
            }
            st.groups.push(Group {
                from,
                other: false,
                files: vec![file],
                bytes,
                items: Vec::new(),
                n_formats: 0,
                n_tools: 0,
                selected: 0,
                expanded: false,
                options: Options::default(),
                params: ToolParams::default(),
            });
        }
        // Tools depend on how many files a group has (merging needs two),
        // so items are worked out once all files are in.
        let st = &mut *st;
        let mut empty = Vec::new();
        for (gi, g) in st.groups.iter_mut().enumerate() {
            let previous = (!g.items.is_empty()).then(|| g.choice());
            if g.other {
                g.items = actions::available(ui.joey.can_zip())
                    .into_iter()
                    .map(Choice::Action)
                    .collect();
                g.n_formats = 0;
                g.n_tools = 0;
                g.selected = previous
                    .and_then(|w| g.items.iter().position(|&i| i == w))
                    .unwrap_or(0);
                continue;
            }
            let formats = ui.joey.suggested_targets(g.from);
            let mut tools = ui.joey.tools_for(g.from, g.files.len());
            // Most wanted first for this kind of file; with several files,
            // joining them is the likely wish.
            let several = g.files.len() > 1;
            let cat = g.from.category();
            tools.sort_by_key(|&t| tool_rank(cat, t, several));
            g.n_formats = formats.len();
            g.n_tools = tools.len();
            g.items = formats
                .into_iter()
                .map(Choice::To)
                .chain(tools.into_iter().map(Choice::Tool))
                .chain(actions::available(false).into_iter().map(Choice::Action))
                .collect();
            if g.n_formats + g.n_tools == 0 {
                empty.push(gi);
                continue;
            }
            let wanted = previous.or_else(|| st.choices.get(&g.from).copied());
            g.selected = wanted
                .and_then(|w| g.items.iter().position(|&i| i == w))
                .unwrap_or(0);
            if previous.is_none() {
                g.options = match g.choice() {
                    Choice::To(f) => st.settings.get(&f).cloned().unwrap_or_default(),
                    _ => Options::default(),
                };
            }
        }
        // Nothing to convert them to: they become "other" items.
        for gi in empty.into_iter().rev() {
            let g = st.groups.remove(gi);
            for f in g.files {
                push_other(st, f);
            }
        }
        if let Some(g) = st.groups.iter_mut().find(|g| g.other) {
            if g.items.is_empty() {
                g.items = actions::available(ui.joey.can_zip())
                    .into_iter()
                    .map(Choice::Action)
                    .collect();
            }
        }
        // "Other" items always come last.
        st.groups.sort_by_key(|g| g.other);
    }
    render(ui);
}

fn push_other(st: &mut State, file: PathBuf) {
    if let Some(g) = st.groups.iter_mut().find(|g| g.other) {
        g.files.push(file);
        return;
    }
    st.groups.push(Group {
        from: Format::Zip,
        other: true,
        files: vec![file],
        bytes: 0,
        items: Vec::new(),
        n_formats: 0,
        n_tools: 0,
        selected: 0,
        expanded: false,
        options: Options::default(),
        params: ToolParams::default(),
    });
}

pub fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn choice_label(c: Choice) -> String {
    match c {
        Choice::To(f) => t(f.display_name()),
        Choice::Tool(tool) => t(tool.label()),
        Choice::Action(a) => a.title(),
    }
}

/// Order of quick tools: the first three show up without "more…".
fn tool_rank(cat: joey_core::Category, tool: Tool, several: bool) -> u8 {
    use joey_core::Category as C;
    if several && tool.merges() && tool != Tool::MakeZip {
        return 0;
    }
    let order: &[Tool] = match cat {
        C::Image => &[
            Tool::Compress,
            Tool::Crop,
            Tool::RotateRight,
            Tool::RotateLeft,
            Tool::Grayscale,
            Tool::RemoveMetadata,
        ],
        C::Video => &[
            Tool::Trim,
            Tool::Compress,
            Tool::RemoveAudio,
            Tool::RotateRight,
            Tool::RotateLeft,
            Tool::RemoveMetadata,
        ],
        C::Audio => &[Tool::Trim, Tool::NormalizeAudio, Tool::RemoveMetadata],
        C::Pdf => &[
            Tool::Compress,
            Tool::ExtractPages,
            Tool::SplitPdf,
            Tool::RotateRight,
            Tool::RotateLeft,
            Tool::ProtectPdf,
            Tool::UnlockPdf,
        ],
        C::Archive => &[Tool::ExtractArchive],
        _ => &[],
    };
    order
        .iter()
        .position(|&t| t == tool)
        .map(|p| p as u8 + 1)
        .unwrap_or(50)
}

fn tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Compress => "zoom-out-symbolic",
        Tool::RemoveMetadata => "edit-clear-symbolic",
        Tool::RotateLeft => "object-rotate-left-symbolic",
        Tool::RotateRight => "object-rotate-right-symbolic",
        Tool::Grayscale => "color-select-symbolic",
        Tool::Crop => "image-crop-symbolic",
        Tool::Trim => "edit-cut-symbolic",
        Tool::NormalizeAudio => "audio-volume-medium-symbolic",
        Tool::RemoveAudio => "audio-volume-muted-symbolic",
        Tool::SplitPdf => "view-paged-symbolic",
        Tool::ExtractPages => "edit-select-all-symbolic",
        Tool::ProtectPdf => "changes-prevent-symbolic",
        Tool::UnlockPdf => "changes-allow-symbolic",
        Tool::ExtractArchive => "folder-open-symbolic",
        Tool::MergePdf | Tool::MergeAudio | Tool::MergeVideo => "list-add-symbolic",
        Tool::MakeZip => "package-x-generic-symbolic",
    }
}

pub fn render(ui: &Rc<Ui>) {
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
            t("1 file")
        } else {
            tf("{n} files", &[("n", &n)])
        })
        .xalign(0.0)
        .css_classes(["summary", "dim-label"])
        .build();
    list.append(&summary);

    let numbered = st.numbered_rows();
    let (limit, single) = (st.limit(), st.single());
    for (gi, g) in st.groups.iter().enumerate() {
        let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        title.add_css_class("group-title");
        let name_label = gtk::Label::builder()
            .label(g.name())
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["dim-label"])
            .build();
        let kind_text = if g.other {
            t("other")
        } else if g.from == Format::Pdf {
            t("PDF")
        } else {
            t(g.from.category().label())
        };
        let kind = gtk::Label::builder()
            .label(kind_text)
            .css_classes(["dim-label"])
            .build();
        title.append(&name_label);
        title.append(&kind);
        list.append(&title);

        let shown = g.shown(limit, single);
        let mut section = None;
        for &i in &shown {
            let here = match g.items[i] {
                Choice::To(_) => None,
                Choice::Tool(_) => Some("Quick tools"),
                Choice::Action(_) => Some("File actions"),
            };
            if here.is_some() && here != section && !g.other {
                section = here;
                let l = gtk::Label::builder()
                    .label(t(here.unwrap_or_default()))
                    .xalign(0.0)
                    .css_classes(["tools-title", "dim-label"])
                    .build();
                list.append(&l);
            }
            let key = numbered.iter().position(|&r| r == (gi, i)).map(|k| k + 1);
            list.append(&item_row(ui, g, gi, i, key));
        }
        if shown.len() < g.items.len() {
            let hidden = g.items.len() - shown.len();
            let more = gtk::Button::with_label(&tf("{n} more…", &[("n", &hidden)]));
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
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("actions");
        let adjust_btn = gtk::Button::builder()
            .child(
                &adw::ButtonContent::builder()
                    .icon_name("emblem-system-symbolic")
                    .label(t("Adjust"))
                    .build(),
            )
            .build();
        adjust_btn.add_css_class("adjust-button");
        let any_settings = st
            .groups
            .iter()
            .any(|g| matches!(g.choice(), Choice::To(_)));
        adjust_btn.set_sensitive(any_settings);
        {
            let ui2 = ui.clone();
            adjust_btn.connect_clicked(move |_| adjust::show(&ui2));
        }
        let convert = convert_button(ui);
        convert.set_hexpand(true);
        row.append(&adjust_btn);
        row.append(&convert);
        list.append(&row);
    }
    list.append(&hint(&t("Esc to close")));
    ui.stack.set_visible_child_name("list");
}

pub fn convert_button(ui: &Rc<Ui>) -> gtk::Button {
    let convert = gtk::Button::new();
    let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    inner.set_halign(gtk::Align::Center);
    inner.append(&gtk::Label::new(Some(&t("Convert"))));
    let enter = gtk::Label::new(Some(&t("Enter")));
    enter.add_css_class("dim-label");
    enter.add_css_class("size");
    inner.append(&enter);
    convert.set_child(Some(&inner));
    convert.add_css_class("suggested-action");
    convert.add_css_class("convert");
    let ui2 = ui.clone();
    convert.connect_clicked(move |_| start(&ui2));
    convert
}

fn item_row(ui: &Rc<Ui>, g: &Group, gi: usize, i: usize, key: Option<usize>) -> gtk::Button {
    let choice = g.items[i];
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let key_label = gtk::Label::new(key.map(|k| k.to_string()).as_deref());
    key_label.add_css_class("key");
    key_label.set_opacity(if key.is_some() { 1.0 } else { 0.0 });
    row.append(&key_label);
    let icon = match choice {
        Choice::Tool(tool) => Some(tool_icon(tool)),
        Choice::Action(a) => Some(a.icon()),
        Choice::To(_) => None,
    };
    if let Some(icon) = icon {
        let icon = gtk::Image::from_icon_name(icon);
        icon.add_css_class("dim-label");
        row.append(&icon);
    }
    row.append(
        &gtk::Label::builder()
            .label(choice_label(choice))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );
    if let Choice::To(to) = choice {
        // The selected row reflects the current settings; others, defaults.
        let opts = if i == g.selected {
            g.options.clone()
        } else {
            Options::default()
        };
        if let Some(s) = estimate::estimate_with(g.bytes, g.from, to, &opts).map(estimate::human) {
            let l = gtk::Label::new(Some(&s));
            l.add_css_class("size");
            l.add_css_class("dim-label");
            row.append(&l);
        }
    }

    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("flat");
    button.add_css_class("fmt-row");
    if i == g.selected {
        button.add_css_class("on");
    }
    let ui2 = ui.clone();
    button.connect_clicked(move |_| pick(&ui2, gi, i, false));
    button
}

/// Selects an item. With a single kind of file, choosing is the whole job:
/// convert right away (unless the user asked to adjust first).
fn pick(ui: &Rc<Ui>, gi: usize, i: usize, adjust_first: bool) {
    let (single, asks) = {
        let mut st = ui.state.borrow_mut();
        let changed = st.groups[gi].selected != i;
        st.groups[gi].selected = i;
        if changed {
            let choice = st.groups[gi].choice();
            st.groups[gi].options = st.options_for(choice);
        }
        let asks =
            matches!(st.groups[gi].choice(), Choice::Tool(t) if t.ask() != joey_core::Ask::Nothing);
        (st.single(), asks)
    };
    if asks {
        ask::show(ui, gi);
    } else if adjust_first {
        adjust::show(ui);
    } else if single {
        start(ui);
    } else {
        render(ui);
    }
}

fn on_key(ui: &Rc<Ui>, key: gdk::Key, code: u32, mods: gdk::ModifierType) -> glib::Propagation {
    let page = ui.stack.visible_child_name();
    let page = page.as_deref();
    if key == gdk::Key::Escape {
        if page == Some("adjust") {
            render(ui);
        } else {
            ui.window.close();
        }
        return glib::Propagation::Stop;
    }
    if page == Some("done") && key == gdk::Key::BackSpace {
        back_to_menu(ui, false);
        return glib::Propagation::Stop;
    }
    if !matches!(page, Some("list") | Some("adjust")) {
        return glib::Propagation::Proceed;
    }
    if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
        start(ui);
        return glib::Propagation::Stop;
    }
    if page != Some("list") {
        return glib::Propagation::Proceed;
    }
    // Digits by physical key too, so Shift+1 works on any keyboard layout.
    let digit = key.to_unicode().and_then(|c| c.to_digit(10)).or_else(|| {
        // X11/Wayland keycodes: 10 = "1" … 18 = "9" on a standard row.
        (10..=18).contains(&code).then(|| code - 9)
    });
    if let Some(d) = digit.filter(|&d| d >= 1) {
        let target = ui
            .state
            .borrow()
            .numbered_rows()
            .get(d as usize - 1)
            .copied();
        if let Some((gi, i)) = target {
            let adjust = mods.contains(gdk::ModifierType::SHIFT_MASK)
                && matches!(ui.state.borrow().groups[gi].items[i], Choice::To(_));
            pick(ui, gi, i, adjust);
            return glib::Propagation::Stop;
        }
    }
    glib::Propagation::Proceed
}

enum Job {
    Convert(PathBuf, Format, Options),
    Tool(Tool, Vec<PathBuf>, ToolParams),
    Zip(Vec<PathBuf>),
    Copy(Vec<PathBuf>, PathBuf, bool),
}

pub fn start(ui: &Rc<Ui>) {
    // A tool still waiting for its answer: ask first.
    let waiting = ui.state.borrow().groups.iter().position(ask::missing);
    if let Some(gi) = waiting {
        ask::show(ui, gi);
        return;
    }
    // Copy/Move need a destination folder before anything runs.
    let wants_folder = {
        let st = ui.state.borrow();
        st.dest.is_none()
            && st
                .groups
                .iter()
                .any(|g| matches!(g.choice(), Choice::Action(a) if a.needs_folder()))
    };
    if wants_folder && !ui.state.borrow().busy {
        let dialog = gtk::FileDialog::builder()
            .title(t("Choose a folder"))
            .build();
        let ui2 = ui.clone();
        dialog.select_folder(
            Some(&ui.window.clone()),
            None::<&gio::Cancellable>,
            move |res| {
                if let Ok(folder) = res {
                    if let Some(path) = folder.path() {
                        ui2.state.borrow_mut().dest = Some(path);
                        start(&ui2);
                    }
                }
            },
        );
        return;
    }

    let mut notes = Vec::new();
    let jobs: Vec<Job> = {
        let mut st = ui.state.borrow_mut();
        if st.busy || st.groups.is_empty() {
            return;
        }
        st.busy = true;
        let picks: Vec<(Format, Choice)> = st
            .groups
            .iter()
            .filter(|g| !g.other)
            .map(|g| (g.from, g.choice()))
            .collect();
        for (from, c) in picks {
            st.choices.insert(from, c);
        }
        choices::save(&st.choices);
        if st.remember {
            let saved: Vec<(Format, Options)> = st
                .groups
                .iter()
                .filter_map(|g| match g.choice() {
                    Choice::To(to) => Some((to, g.options.clone())),
                    _ => None,
                })
                .collect();
            st.settings.extend(saved);
            choices::save_settings(&st.settings);
        }
        let dest = st.dest.take();
        let mut jobs = Vec::new();
        let mut paths_to_copy = Vec::new();
        for g in &st.groups {
            match g.choice() {
                Choice::To(to) => {
                    for f in &g.files {
                        jobs.push(Job::Convert(f.clone(), to, g.options.clone()));
                    }
                }
                Choice::Tool(tool) if tool.merges() => {
                    jobs.push(Job::Tool(tool, g.files.clone(), g.params.clone()))
                }
                Choice::Tool(tool) => {
                    for f in &g.files {
                        jobs.push(Job::Tool(tool, vec![f.clone()], g.params.clone()));
                    }
                }
                Choice::Action(Action::Zip) => jobs.push(Job::Zip(g.files.clone())),
                Choice::Action(a @ (Action::CopyTo | Action::MoveTo)) => {
                    if let Some(d) = &dest {
                        jobs.push(Job::Copy(g.files.clone(), d.clone(), a == Action::MoveTo));
                    }
                }
                Choice::Action(Action::CopyPath) => {
                    paths_to_copy.extend(g.files.iter().map(|f| f.display().to_string()));
                }
                Choice::Action(Action::Terminal) => match actions::open_terminal(&g.files[0]) {
                    Ok(()) => notes.push(t("Terminal opened")),
                    Err(e) => notes.push(e.to_string()),
                },
                Choice::Action(Action::Phone(i)) => match actions::send_to_phone(i, &g.files) {
                    Ok(m) | Err(m) => notes.push(m),
                },
                Choice::Action(Action::LocalSend) => match actions::open_localsend(&g.files) {
                    Ok(m) | Err(m) => notes.push(m),
                },
            }
        }
        if !paths_to_copy.is_empty() {
            ui.window.clipboard().set_text(&paths_to_copy.join("\n"));
            notes.push(t("Path copied"));
        }
        jobs
    };

    ui.stack.set_visible_child_name("progress");
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let total = jobs.len();
        let mut written = Vec::new();
        let mut errors = Vec::new();
        for (i, job) in jobs.into_iter().enumerate() {
            let first = match &job {
                Job::Convert(f, ..) => f.clone(),
                Job::Tool(_, fs, _) | Job::Zip(fs) | Job::Copy(fs, ..) => fs[0].clone(),
            };
            ui.progress_label.set_text(&if total == 1 {
                tf("Converting {name}…", &[("name", &file_name(&first))])
            } else {
                tf("Converting {i} of {n}…", &[("i", &(i + 1)), ("n", &total)])
            });
            ui.progress.set_fraction(i as f64 / total as f64);
            let joey = ui.joey.clone();
            let outcomes = gio::spawn_blocking(move || match job {
                Job::Convert(f, to, o) => vec![joey.convert_with(&f, to, None, &o)],
                Job::Tool(tool, fs, p) => joey.run_tool_with(tool, &fs, &p),
                Job::Zip(fs) => vec![joey.zip(&fs)],
                Job::Copy(fs, dest, moving) => vec![actions::copy_or_move(&fs, &dest, moving)
                    .map_err(|m| joey_core::Error::Tool {
                        tool: "joey",
                        message: m,
                    })],
            })
            .await;
            match outcomes {
                Ok(list) => {
                    for o in list {
                        match o {
                            Ok(out) => written.extend(out),
                            Err(e) => {
                                errors.push(format!("{}: {}", file_name(&first), friendly(&e)))
                            }
                        }
                    }
                }
                Err(_) => errors.push(format!("{}: {}", file_name(&first), t("unexpected error"))),
            }
        }
        ui.progress.set_fraction(1.0);
        finish(&ui, written, errors, notes);
    });
}

/// Error text for people, not developers.
fn friendly(e: &joey_core::Error) -> String {
    match e {
        joey_core::Error::Tool {
            tool: "joey",
            message,
        } => t(message),
        joey_core::Error::Tool { message, .. } if message.contains("no audio track") => {
            t("the file has no audio track")
        }
        joey_core::Error::Tool { tool, .. } => tf("{tool} couldn't convert it", &[("tool", tool)]),
        other => other.to_string(),
    }
}

fn finish(ui: &Rc<Ui>, written: Vec<PathBuf>, errors: Vec<String>, notes: Vec<String>) {
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
        0 if !notes.is_empty() => notes.join(" · "),
        0 => t("Nothing converted"),
        1 => t("1 file ready"),
        n => tf("{n} files ready", &[("n", &n)]),
    });
    let moved = ui
        .state
        .borrow()
        .groups
        .iter()
        .any(|g| matches!(g.choice(), Choice::Action(Action::CopyTo | Action::MoveTo)));
    ui.done_sub.set_text(&if moved {
        t("In the folder you chose")
    } else {
        t("Next to the originals")
    });
    ui.done_sub.set_visible(n > 0);
    ui.done_errors.set_text(&errors.join("\n"));
    ui.done_errors.set_visible(!ok);
    ui.show_folder.set_visible(n > 0);
    ui.stack.set_visible_child_name("done");

    if n > 0 {
        let note = gio::Notification::new(&ui.done_title.text());
        note.set_body(Some(&t("Next to the originals")));
        ui.app.send_notification(Some("done"), &note);
    }

    // All went well and nobody is looking: close by itself.
    if ok {
        let ui = ui.clone();
        let run = ui.run.get();
        glib::timeout_add_seconds_local_once(3, move || {
            let still_done = ui.stack.visible_child_name().as_deref() == Some("done");
            if !ui.hovered.get() && ui.run.get() == run && still_done {
                ui.window.close();
            }
        });
    }
}

/// Leaves the done screen: back to the same files, or empty for new ones.
fn back_to_menu(ui: &Rc<Ui>, clear: bool) {
    ui.run.set(ui.run.get() + 1);
    {
        let mut st = ui.state.borrow_mut();
        st.busy = false;
        if clear {
            st.groups.clear();
            st.skipped.clear();
        } else {
            // Moved files aren't here any more.
            for g in st.groups.iter_mut() {
                g.files.retain(|f| f.exists());
            }
            st.groups.retain(|g| !g.files.is_empty());
        }
    }
    render(ui);
}
