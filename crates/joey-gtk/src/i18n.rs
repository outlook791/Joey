//! Interface text in the system language. English is the source; the
//! translations live in `i18n/<language>.tsv` (one line per text:
//! English, a tab, the translation) and are built into the program.
//! Call sites: `t("…")` and `tf("… {n} …", &[("n", …)])`.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Built-in languages: the 15 most spoken in the world, plus Italian.
const CATALOGS: &[(&str, &str)] = &[
    ("it", include_str!("../i18n/it.tsv")),
    ("zh", include_str!("../i18n/zh.tsv")),
    ("hi", include_str!("../i18n/hi.tsv")),
    ("es", include_str!("../i18n/es.tsv")),
    ("ar", include_str!("../i18n/ar.tsv")),
    ("fr", include_str!("../i18n/fr.tsv")),
    ("bn", include_str!("../i18n/bn.tsv")),
    ("pt", include_str!("../i18n/pt.tsv")),
    ("ru", include_str!("../i18n/ru.tsv")),
    ("id", include_str!("../i18n/id.tsv")),
    ("ur", include_str!("../i18n/ur.tsv")),
    ("de", include_str!("../i18n/de.tsv")),
    ("ja", include_str!("../i18n/ja.tsv")),
    ("mr", include_str!("../i18n/mr.tsv")),
    ("vi", include_str!("../i18n/vi.tsv")),
];

/// The language code of the interface ("it", "zh"…), from the usual
/// variables, first one set wins (LANGUAGE may list several: "fr:en").
pub fn language() -> &'static str {
    static LANG: OnceLock<&'static str> = OnceLock::new();
    LANG.get_or_init(|| {
        for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            let Ok(v) = std::env::var(var) else { continue };
            if v.is_empty() {
                continue;
            }
            for wanted in v.split(':') {
                let code = wanted.split(['_', '.', '@']).next().unwrap_or("");
                if let Some((c, _)) = CATALOGS.iter().find(|(c, _)| *c == code) {
                    return c;
                }
                if code == "en" || code == "C" || code == "POSIX" {
                    return "en";
                }
            }
            return "en";
        }
        "en"
    })
}

/// True for languages written right to left.
pub fn right_to_left() -> bool {
    matches!(language(), "ar" | "ur")
}

fn catalog() -> Option<&'static HashMap<&'static str, &'static str>> {
    static MAP: OnceLock<Option<HashMap<&'static str, &'static str>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let text = CATALOGS.iter().find(|(c, _)| *c == language())?.1;
        Some(parse(text))
    })
    .as_ref()
}

fn parse(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, tr)| !tr.trim().is_empty())
        .collect()
}

/// Translates a fixed string.
pub fn t(en: &str) -> String {
    catalog()
        .and_then(|c| c.get(en))
        .map(|tr| tr.to_string())
        .unwrap_or_else(|| en.to_string())
}

/// Translates a template and fills `{name}` placeholders.
pub fn tf(en: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = t(en);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), &v.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every catalog: well-formed lines, the same placeholders as the
    /// English text, and no text missing compared with Italian.
    #[test]
    fn catalogs_are_complete_and_keep_placeholders() {
        let it = parse(CATALOGS[0].1);
        for (code, text) in CATALOGS {
            let map = parse(text);
            for en in it.keys() {
                assert!(map.contains_key(en), "{code}: missing {en:?}");
            }
            for (en, tr) in &map {
                let holes = |s: &str| {
                    let mut v: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}').map(|(h, _)| h.to_string()))
                        .collect();
                    v.sort();
                    v
                };
                assert_eq!(holes(en), holes(tr), "{code}: placeholders in {en:?}");
            }
        }
    }
}
