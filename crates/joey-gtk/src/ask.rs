//! The one question some quick tools need: from when to when (trim),
//! which pages, which shape (crop), which password. One small screen,
//! then Convert.

use std::rc::Rc;

use adw::prelude::*;
use joey_core::options::{format_time, parse_time};
use joey_core::{Ask, Tool};

use crate::choices::Choice;
use crate::i18n::t;
use crate::{choice_label, convert_button, render, Ui};

/// Shapes offered by "Crop to a shape…": (label, width, height).
const SHAPES: [(&str, u32, u32); 6] = [
    ("Square · 1:1", 1, 1),
    ("Portrait · 4:5", 4, 5),
    ("Photo · 3:2", 3, 2),
    ("Classic · 4:3", 4, 3),
    ("Wide · 16:9", 16, 9),
    ("Story · 9:16", 9, 16),
];

/// Whether this group still needs an answer before it can run.
pub fn missing(g: &crate::Group) -> bool {
    let Choice::Tool(tool) = g.choice() else {
        return false;
    };
    let p = &g.params;
    match tool.ask() {
        Ask::Nothing => false,
        Ask::TimeRange => !matches!(p.trim, Some((a, b)) if b > a),
        Ask::Pages => p.pages.as_deref().is_none_or(|s| s.trim().is_empty()),
        Ask::Shape => p.shape.is_none(),
        Ask::Password => {
            tool == Tool::ProtectPdf && p.password.as_deref().is_none_or(str::is_empty)
        }
    }
}

/// Shows the question for group `gi`.
pub fn show(ui: &Rc<Ui>, gi: usize) {
    let page = &ui.adjust;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let (title, tool, params) = {
        let st = ui.state.borrow();
        let g = &st.groups[gi];
        let Choice::Tool(tool) = g.choice() else {
            return;
        };
        (
            format!("{} → {}", glib_escape(&g.name()), choice_label(g.choice())),
            tool,
            g.params.clone(),
        )
    };
    let pg = adw::PreferencesGroup::builder().title(title).build();

    let set = {
        let ui = ui.clone();
        move |f: &dyn Fn(&mut joey_core::ToolParams)| {
            f(&mut ui.state.borrow_mut().groups[gi].params);
        }
    };
    let set = Rc::new(set);

    match tool.ask() {
        Ask::TimeRange => {
            let (a, b) = params.trim.unwrap_or((0, 0));
            for (label, start) in [(t("From"), true), (t("To"), false)] {
                let row = adw::EntryRow::builder().title(label).build();
                let v = if start { a } else { b };
                if v > 0 || start {
                    row.set_text(&format_time(v));
                }
                let set = set.clone();
                row.connect_changed(move |r| {
                    let ms = parse_time(&r.text());
                    set(&|p| {
                        let (mut a, mut b) = p.trim.unwrap_or((0, 0));
                        if start {
                            a = ms.unwrap_or(0);
                        } else {
                            b = ms.unwrap_or(0);
                        }
                        p.trim = Some((a, b));
                    });
                });
                pg.add(&row);
            }
            pg.set_description(Some(&t(
                "For example 0:30 and 1:45. Only that part is kept.",
            )));
        }
        Ask::Pages => {
            let row = adw::EntryRow::builder().title(t("Pages")).build();
            if let Some(p) = &params.pages {
                row.set_text(p);
            }
            let set = set.clone();
            row.connect_changed(move |r| {
                let text = r.text().to_string();
                set(&|p| p.pages = Some(text.clone()));
            });
            pg.add(&row);
            pg.set_description(Some(&t("For example 1-3, 7. Only these pages are kept.")));
        }
        Ask::Shape => {
            let labels: Vec<String> = SHAPES.iter().map(|(l, ..)| t(l)).collect();
            let strs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let selected = SHAPES
                .iter()
                .position(|&(_, w, h)| params.shape == Some((w, h)))
                .unwrap_or(0);
            let row = adw::ComboRow::builder()
                .title(t("Shape"))
                .subtitle(t("The middle of the picture is kept"))
                .model(&gtk::StringList::new(&strs))
                .selected(selected as u32)
                .build();
            let (_, w, h) = SHAPES[selected];
            set(&|p| p.shape = Some((w, h)));
            let set = set.clone();
            row.connect_selected_notify(move |r| {
                let (_, w, h) = SHAPES[r.selected() as usize];
                set(&|p| p.shape = Some((w, h)));
            });
            pg.add(&row);
        }
        Ask::Password => {
            let row = adw::PasswordEntryRow::builder()
                .title(t("Password"))
                .build();
            let set = set.clone();
            row.connect_changed(move |r| {
                let text = r.text().to_string();
                set(&|p| p.password = Some(text.clone()));
            });
            pg.add(&row);
            pg.set_description(Some(&if tool == Tool::ProtectPdf {
                t("Needed to open the PDF. Keep it somewhere safe: it can't be recovered.")
            } else {
                t("The password that opens this PDF now.")
            }));
        }
        Ask::Nothing => {}
    }
    page.append(&pg);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.add_css_class("actions");
    let back = gtk::Button::with_label(&t("Back"));
    {
        let ui = ui.clone();
        back.connect_clicked(move |_| render(&ui));
    }
    let convert = convert_button(ui);
    convert.set_hexpand(true);
    buttons.append(&back);
    buttons.append(&convert);
    page.append(&buttons);
    ui.stack.set_visible_child_name("adjust");
}

fn glib_escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}
