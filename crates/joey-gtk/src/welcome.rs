//! The first start: which keys open Joey. The usual ones are already
//! chosen, so one click ("Turn on") is enough; "Change" records another
//! combination. Then Joey sets up the file managers and the desktop by
//! itself, so nobody needs a terminal. Reachable later from the start page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};

use crate::i18n::t;
use crate::integration::{self, Keys, Shortcut};
use crate::Ui;

#[derive(Clone, Copy, PartialEq)]
enum Which {
    Files,
    Anywhere,
}

thread_local! {
    static KEYS: RefCell<Option<Keys>> = const { RefCell::new(None) };
    static RECORDING: Cell<Option<Which>> = const { Cell::new(None) };
    static PROBLEM: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn done_file() -> std::path::PathBuf {
    glib::user_config_dir().join("joey").join("welcome-done")
}

/// The first time Joey opens on its own and nothing is set up yet.
pub fn should_show() -> bool {
    !done_file().exists() && !integration::is_set_up()
}

fn mark_done() {
    let f = done_file();
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(f, "");
}

fn keys() -> Keys {
    KEYS.with(|k| {
        k.borrow_mut()
            .get_or_insert_with(integration::load_keys)
            .clone()
    })
}

/// True while "Change" waits for a key combination.
pub fn recording() -> bool {
    RECORDING.with(Cell::get).is_some()
}

/// A key pressed while recording. Esc cancels.
pub fn key(ui: &Rc<Ui>, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
    let Some(which) = RECORDING.with(Cell::get) else {
        return glib::Propagation::Proceed;
    };
    if key == gdk::Key::Escape {
        RECORDING.with(|r| r.set(None));
        show(ui);
        return glib::Propagation::Stop;
    }
    // Modifier keys alone: wait for the rest of the combination.
    let Some(c) = key
        .to_lower()
        .to_unicode()
        .filter(|c| c.is_ascii_alphanumeric())
    else {
        return glib::Propagation::Stop;
    };
    let sc = Shortcut {
        ctrl: mods.contains(gdk::ModifierType::CONTROL_MASK),
        alt: mods.contains(gdk::ModifierType::ALT_MASK),
        shift: mods.contains(gdk::ModifierType::SHIFT_MASK),
        meta: mods.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK),
        key: c,
    };
    let other = match which {
        Which::Files => keys().anywhere,
        Which::Anywhere => keys().files,
    };
    let problem = if !sc.usable() {
        Some(t(
            "Use Alt or Meta (the Windows key) with a letter or a number",
        ))
    } else if sc == other {
        Some(t("That's already the other shortcut"))
    } else {
        None
    };
    if problem.is_none() {
        KEYS.with(|k| {
            let mut k = k.borrow_mut();
            let keys = k.get_or_insert_with(integration::load_keys);
            match which {
                Which::Files => keys.files = sc,
                Which::Anywhere => keys.anywhere = sc,
            }
        });
        RECORDING.with(|r| r.set(None));
    }
    PROBLEM.with(|p| *p.borrow_mut() = problem);
    show(ui);
    glib::Propagation::Stop
}

fn shortcut_row(ui: &Rc<Ui>, title: &str, subtitle: &str, which: Which) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    if RECORDING.with(Cell::get) == Some(which) {
        let l = gtk::Label::new(Some(&t("Press the keys… (Esc cancels)")));
        l.add_css_class("accent");
        row.add_suffix(&l);
    } else {
        let sc = match which {
            Which::Files => keys().files,
            Which::Anywhere => keys().anywhere,
        };
        // The keys themselves are the button: click to change them.
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inner.append(&gtk::ShortcutLabel::new(&sc.gtk()));
        inner.append(&gtk::Image::from_icon_name("document-edit-symbolic"));
        let change = gtk::Button::builder()
            .child(&inner)
            .tooltip_text(t("Change"))
            .valign(gtk::Align::Center)
            .build();
        change.add_css_class("flat");
        let ui = ui.clone();
        change.connect_clicked(move |_| {
            RECORDING.with(|r| r.set(Some(which)));
            PROBLEM.with(|p| *p.borrow_mut() = None);
            show(&ui);
        });
        row.add_suffix(&change);
    }
    row
}

/// Shows the welcome page (also to change the shortcuts later).
pub fn show(ui: &Rc<Ui>) {
    let page = &ui.welcome;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let icon = gtk::Image::from_icon_name(crate::APP_ID);
    icon.set_pixel_size(48);
    page.append(&icon);
    let title = gtk::Label::new(Some(&t("Welcome to Joey")));
    title.add_css_class("title-3");
    page.append(&title);
    let intro = gtk::Label::builder()
        .label(t(
            "Two shortcuts open Joey. Keep these, or choose your own.",
        ))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .css_classes(["dim-label"])
        .build();
    page.append(&intro);

    let group = adw::PreferencesGroup::new();
    group.add(&shortcut_row(
        ui,
        &t("In the file manager"),
        &t("Select files, then press"),
        Which::Files,
    ));
    group.add(&shortcut_row(
        ui,
        &t("Anywhere, desktop included"),
        &t("Copy files with Ctrl+C, then press"),
        Which::Anywhere,
    ));
    page.append(&group);

    if let Some(problem) = PROBLEM.with(|p| p.borrow().clone()) {
        let l = gtk::Label::builder()
            .label(problem)
            .wrap(true)
            .css_classes(["error", "skipped"])
            .build();
        page.append(&l);
    }

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.add_css_class("actions");
    let later = gtk::Button::with_label(&t("Later"));
    let on = gtk::Button::with_label(&t("Turn on"));
    on.add_css_class("suggested-action");
    on.add_css_class("convert");
    on.set_hexpand(true);
    later.add_css_class("adjust-button");
    buttons.append(&later);
    buttons.append(&on);
    page.append(&buttons);
    page.append(&crate::hint(&t("Esc to close")));
    {
        let ui = ui.clone();
        later.connect_clicked(move |_| {
            mark_done();
            RECORDING.with(|r| r.set(None));
            crate::show_start(&ui);
        });
    }
    {
        let ui = ui.clone();
        on.connect_clicked(move |_| turn_on(&ui));
    }
    ui.stack.set_visible_child_name("welcome");
}

/// Saves the shortcuts and sets everything up, then says what's ready.
fn turn_on(ui: &Rc<Ui>) {
    let keys = keys();
    integration::save_keys(&keys);
    let report = integration::setup_with(&keys);
    mark_done();
    let page = &ui.welcome;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let icon = gtk::Image::from_icon_name("object-select-symbolic");
    icon.set_pixel_size(28);
    icon.add_css_class("success");
    page.append(&icon);
    let title = gtk::Label::new(Some(&t("Ready")));
    title.add_css_class("title-4");
    page.append(&title);
    let lines = crate::report_lines(&report);
    let body = gtk::Label::builder()
        .label(lines.join("\n"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    page.append(&body);
    let start = gtk::Button::with_label(&t("Start"));
    start.add_css_class("pill");
    start.add_css_class("suggested-action");
    start.set_halign(gtk::Align::Center);
    start.set_margin_top(10);
    {
        let ui = ui.clone();
        start.connect_clicked(move |_| crate::show_start(&ui));
    }
    page.append(&start);
}
