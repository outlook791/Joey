//! The "Adjust" screen: settings for the chosen conversions, in plain
//! words. Essentials first; everything else under "Advanced", closed by
//! default. The estimated size updates as settings change.

use std::rc::Rc;

use adw::prelude::*;
use joey_core::{estimate, Category, Format, Options, Quality};

use crate::choices::Choice;
use crate::i18n::{t, tf};
use crate::{convert_button, render, Group, Ui};

const IMAGE_SIZES: [Option<u32>; 5] = [None, Some(3840), Some(1920), Some(1280), Some(800)];
const VIDEO_SIZES: [Option<u32>; 5] = [None, Some(3840), Some(1920), Some(1280), Some(854)];
const GIF_WIDTHS: [u32; 3] = [320, 480, 720];
const CHANNELS: [Option<u8>; 3] = [None, Some(1), Some(2)];
const BITRATES: [Option<u32>; 6] = [None, Some(96), Some(128), Some(192), Some(256), Some(320)];
const SAMPLE_RATES: [Option<u32>; 3] = [None, Some(44100), Some(48000)];
const VIDEO_FPS: [Option<u32>; 4] = [None, Some(24), Some(30), Some(60)];
const GIF_FPS: [u32; 4] = [10, 12, 15, 24];
const DPIS: [u32; 4] = [72, 150, 300, 600];

fn index<T: PartialEq>(values: &[T], v: &T, default: usize) -> usize {
    values.iter().position(|x| x == v).unwrap_or(default)
}

fn lossy_image(f: Format) -> bool {
    matches!(f, Format::Jpg | Format::Webp | Format::Avif | Format::Heic)
}

fn lossless_audio(f: Format) -> bool {
    matches!(f, Format::Wav | Format::Flac | Format::Aiff)
}

fn estimate_text(g: &Group) -> String {
    let Choice::To(to) = g.choice() else {
        return String::new();
    };
    match estimate::estimate_with(g.bytes, g.from, to, &g.options) {
        Some(b) => tf("Estimated size: {size}", &[("size", &estimate::human(b))]),
        None => t("Size can't be estimated for this conversion"),
    }
}

/// Changes one group's settings and refreshes its size estimate.
fn setter<V: 'static>(
    ui: &Rc<Ui>,
    gi: usize,
    pg: &adw::PreferencesGroup,
    apply: impl Fn(&mut Options, V) + 'static,
) -> impl Fn(V) + 'static {
    let ui = ui.clone();
    let pg = pg.clone();
    move |v| {
        let desc = {
            let mut st = ui.state.borrow_mut();
            apply(&mut st.groups[gi].options, v);
            estimate_text(&st.groups[gi])
        };
        pg.set_description(Some(&desc));
    }
}

fn combo(
    title: &str,
    subtitle: &str,
    labels: &[String],
    selected: usize,
    on: impl Fn(usize) + 'static,
) -> adw::ComboRow {
    let strs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let row = adw::ComboRow::builder()
        .title(title)
        .subtitle(subtitle)
        .model(&gtk::StringList::new(&strs))
        .selected(selected as u32)
        .build();
    row.connect_selected_notify(move |r| on(r.selected() as usize));
    row
}

fn switch(
    title: &str,
    subtitle: &str,
    active: bool,
    on: impl Fn(bool) + 'static,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .active(active)
        .build();
    row.connect_active_notify(move |r| on(r.is_active()));
    row
}

fn entry(title: &str, subtitle: &str, text: &str, on: impl Fn(String) + 'static) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let e = gtk::Entry::builder()
        .text(text)
        .placeholder_text(t("all"))
        .width_chars(9)
        .valign(gtk::Align::Center)
        .build();
    e.connect_changed(move |e| on(e.text().to_string()));
    row.add_suffix(&e);
    row
}

fn quality_labels(audio: bool) -> Vec<String> {
    let names = [t("Low"), t("Medium"), t("High (recommended)"), t("Maximum")];
    Quality::ALL
        .iter()
        .zip(names)
        .map(|(q, n)| {
            if audio {
                format!("{n} · {} kbps", q.audio_kbps())
            } else {
                n
            }
        })
        .collect()
}

fn quality_row(
    ui: &Rc<Ui>,
    gi: usize,
    pg: &adw::PreferencesGroup,
    o: &Options,
    audio: bool,
) -> adw::ComboRow {
    combo(
        &t("Quality"),
        &t("Lower quality makes smaller files"),
        &quality_labels(audio),
        index(&Quality::ALL, &o.quality, 2),
        setter(ui, gi, pg, |o, i: usize| o.quality = Quality::ALL[i]),
    )
}

fn pages_rows(ui: &Rc<Ui>, gi: usize, pg: &adw::PreferencesGroup, o: &Options, sharpness: bool) {
    pg.add(&entry(
        &t("Pages"),
        &t("For example 1-3, 7. Empty means all."),
        o.pages.as_deref().unwrap_or(""),
        setter(ui, gi, pg, |o, s: String| {
            o.pages = (!s.trim().is_empty()).then(|| s.trim().to_string())
        }),
    ));
    if sharpness {
        let labels = [
            t("Screen · 72 dpi"),
            t("Normal · 150 dpi"),
            t("Print · 300 dpi"),
            t("Very sharp · 600 dpi"),
        ];
        pg.add(&combo(
            &t("Sharpness"),
            &t("Dots per inch: higher is sharper and bigger"),
            &labels,
            index(&DPIS, &o.dpi(), 1),
            setter(ui, gi, pg, |o, i: usize| o.dpi = Some(DPIS[i])),
        ));
    }
}

/// Builds the settings for one group. Returns false when there's nothing
/// to set for this conversion.
fn group_rows(ui: &Rc<Ui>, gi: usize, g: &Group, to: Format, pg: &adw::PreferencesGroup) -> bool {
    let o = &g.options;
    let (from_cat, to_cat) = (g.from.category(), to.category());
    let paged = matches!(
        from_cat,
        Category::Pdf | Category::Document | Category::Spreadsheet | Category::Presentation
    );
    let advanced = adw::ExpanderRow::builder().title(t("Advanced")).build();
    let mut has_advanced = false;

    match to_cat {
        Category::Image if from_cat == Category::Video => {
            // Video to animated GIF.
            let labels = [
                t("Small · 320 px"),
                t("Medium · 480 px"),
                t("Large · 720 px"),
            ];
            pg.add(&combo(
                &t("Width"),
                &t("Longest side. Never enlarges."),
                &labels,
                index(&GIF_WIDTHS, &o.max_size.unwrap_or(480), 1),
                setter(ui, gi, pg, |o, i: usize| o.max_size = Some(GIF_WIDTHS[i])),
            ));
            let labels: Vec<String> = GIF_FPS.iter().map(|f| f.to_string()).collect();
            advanced.add_row(&combo(
                &t("Frames per second"),
                &t("How smooth the motion is. 30 is fine for most videos."),
                &labels,
                index(&GIF_FPS, &o.fps.unwrap_or(12), 1),
                setter(ui, gi, pg, |o, i: usize| o.fps = Some(GIF_FPS[i])),
            ));
            has_advanced = true;
        }
        Category::Image => {
            if paged {
                pages_rows(ui, gi, pg, o, true);
            }
            if lossy_image(to) {
                pg.add(&quality_row(ui, gi, pg, o, false));
            }
            let labels = [
                t("Original"),
                t("Very large · 3840 px"),
                t("Large · 1920 px"),
                t("Medium · 1280 px"),
                t("Small · 800 px"),
            ];
            pg.add(&combo(
                &t("Size"),
                &t("Longest side. Never enlarges."),
                &labels,
                index(&IMAGE_SIZES, &o.max_size, 0),
                setter(ui, gi, pg, |o, i: usize| o.max_size = IMAGE_SIZES[i]),
            ));
            if !paged {
                pg.add(&switch(
                    &t("Remove camera and location info"),
                    &t("Date, place and camera model saved inside the photo"),
                    o.strip_metadata,
                    setter(ui, gi, pg, |o, v: bool| o.strip_metadata = v),
                ));
            }
        }
        Category::Pdf if paged => pages_rows(ui, gi, pg, o, false),
        Category::Pdf if from_cat == Category::Image => {
            let labels = [
                t("Original"),
                t("Very large · 3840 px"),
                t("Large · 1920 px"),
                t("Medium · 1280 px"),
                t("Small · 800 px"),
            ];
            pg.add(&combo(
                &t("Size"),
                &t("Longest side. Never enlarges."),
                &labels,
                index(&IMAGE_SIZES, &o.max_size, 0),
                setter(ui, gi, pg, |o, i: usize| o.max_size = IMAGE_SIZES[i]),
            ));
        }
        Category::Audio => {
            if !lossless_audio(to) {
                pg.add(&quality_row(ui, gi, pg, o, true));
            }
            let labels = [t("Original"), t("Mono"), t("Stereo")];
            pg.add(&combo(
                &t("Channels"),
                &t("Mono halves the size; fine for voice"),
                &labels,
                index(&CHANNELS, &o.audio_channels, 0),
                setter(ui, gi, pg, |o, i: usize| o.audio_channels = CHANNELS[i]),
            ));
            if !lossless_audio(to) {
                advanced.add_row(&bitrate_row(ui, gi, pg, o));
            }
            let labels = [t("Original"), t("44.1 kHz · CD"), t("48 kHz · video")];
            advanced.add_row(&combo(
                &t("Sample rate"),
                &t("How often the sound is measured each second. Change it only if a device asks."),
                &labels,
                index(&SAMPLE_RATES, &o.sample_rate, 0),
                setter(ui, gi, pg, |o, i: usize| o.sample_rate = SAMPLE_RATES[i]),
            ));
            advanced.add_row(&switch(
                &t("Remove song info"),
                &t("Title, artist, album and similar tags"),
                o.strip_metadata,
                setter(ui, gi, pg, |o, v: bool| o.strip_metadata = v),
            ));
            has_advanced = true;
        }
        Category::Video => {
            pg.add(&quality_row(ui, gi, pg, o, false));
            let labels = [
                t("Original"),
                t("4K · 2160p"),
                t("Full HD · 1080p"),
                t("HD · 720p"),
                t("Small · 480p"),
            ];
            pg.add(&combo(
                &t("Resolution"),
                &t("Smaller resolution, much smaller file"),
                &labels,
                index(&VIDEO_SIZES, &o.max_size, 0),
                setter(ui, gi, pg, |o, i: usize| o.max_size = VIDEO_SIZES[i]),
            ));
            if g.from != Format::Gif {
                pg.add(&switch(
                    &t("Keep the sound"),
                    "",
                    !o.no_audio,
                    setter(ui, gi, pg, |o, v: bool| o.no_audio = !v),
                ));
            }
            let labels = [
                t("Original"),
                "24".to_string(),
                "30".to_string(),
                "60".to_string(),
            ];
            advanced.add_row(&combo(
                &t("Frames per second"),
                &t("How smooth the motion is. 30 is fine for most videos."),
                &labels,
                index(&VIDEO_FPS, &o.fps, 0),
                setter(ui, gi, pg, |o, i: usize| o.fps = VIDEO_FPS[i]),
            ));
            if g.from != Format::Gif {
                advanced.add_row(&bitrate_row(ui, gi, pg, o));
            }
            advanced.add_row(&switch(
                &t("Remove video info"),
                &t("Date, place and device saved inside the video"),
                o.strip_metadata,
                setter(ui, gi, pg, |o, v: bool| o.strip_metadata = v),
            ));
            has_advanced = true;
        }
        _ => return false,
    }
    if has_advanced {
        pg.add(&advanced);
    }
    true
}

fn bitrate_row(ui: &Rc<Ui>, gi: usize, pg: &adw::PreferencesGroup, o: &Options) -> adw::ComboRow {
    let labels: Vec<String> = BITRATES
        .iter()
        .map(|b| match b {
            None => t("Automatic"),
            Some(k) => format!("{k} kbps"),
        })
        .collect();
    combo(
        &t("Bitrate"),
        &t("Data per second: higher means better sound and a bigger file. Replaces Quality."),
        &labels,
        index(&BITRATES, &o.audio_kbps, 0),
        setter(ui, gi, pg, |o, i: usize| o.audio_kbps = BITRATES[i]),
    )
}

/// Shows the Adjust screen for the current choices.
pub fn show(ui: &Rc<Ui>) {
    let page = &ui.adjust;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    {
        let st = ui.state.borrow();
        for (gi, g) in st.groups.iter().enumerate() {
            let Choice::To(to) = g.choice() else { continue };
            let pg = adw::PreferencesGroup::builder()
                .title(format!(
                    "{} → {}",
                    glib_escape(&g.name()),
                    to.display_name()
                ))
                .description(estimate_text(g))
                .build();
            if !group_rows(ui, gi, g, to, &pg) {
                let none = adw::ActionRow::builder()
                    .title(t("No settings for this one."))
                    .build();
                none.add_css_class("dim-label");
                pg.add(&none);
            }
            page.append(&pg);
        }
    }

    let remember = gtk::CheckButton::builder()
        .label(t("Remember these settings"))
        .active(ui.state.borrow().remember)
        .build();
    {
        let ui = ui.clone();
        remember.connect_toggled(move |c| ui.state.borrow_mut().remember = c.is_active());
    }
    page.append(&remember);

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

    let reset = gtk::Button::with_label(&t("Reset to defaults"));
    reset.add_css_class("flat");
    reset.add_css_class("more");
    {
        let ui = ui.clone();
        reset.connect_clicked(move |_| {
            for g in ui.state.borrow_mut().groups.iter_mut() {
                g.options = Options::default();
            }
            show(&ui);
        });
    }
    page.append(&reset);

    ui.stack.set_visible_child_name("adjust");
}

/// Group titles are Pango markup: file names may contain `&` or `<`.
fn glib_escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}
