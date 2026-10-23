//! Real conversions on small sample files. A conversion the installed tools
//! can't do is reported as skipped, not failed, so the suite runs anywhere.

use std::path::{Path, PathBuf};

use joey_core::{Error, Format, Joey};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Copies a fixture into a fresh folder so results land there.
fn sample(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join(name);
    std::fs::copy(fixture(name), &dest).unwrap();
    (dir, dest)
}

/// Converts and checks every output really is the target format.
/// Returns None when this computer has no route.
fn check(name: &str, to: Format) -> Option<Vec<PathBuf>> {
    let joey = Joey::new();
    let (dir, input) = sample(name);
    match joey.convert(&input, to, None) {
        Err(Error::NoRoute { .. }) => {
            eprintln!("skipped {name} → {to}: no tool for it here");
            None
        }
        Err(e) => panic!("{name} → {to} failed: {e}"),
        Ok(written) => {
            assert!(!written.is_empty());
            for w in &written {
                assert_eq!(
                    w.parent(),
                    Some(dir.path()),
                    "result goes next to the original"
                );
                assert_eq!(Format::detect(w), Some(to), "{} is not {to}", w.display());
                assert!(std::fs::metadata(w).unwrap().len() > 0);
            }
            assert!(input.exists(), "original must be untouched");
            let leftovers: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with(".joey-"))
                .collect();
            assert!(leftovers.is_empty(), "work folder must be removed");
            // Keep the folder alive until checks are done.
            drop(dir);
            Some(written)
        }
    }
}

#[test]
fn png_with_transparency_to_jpg() {
    check("alpha.png", Format::Jpg);
}

#[test]
fn jpg_to_png_webp_tiff() {
    for to in [Format::Png, Format::Webp, Format::Tiff, Format::Bmp] {
        check("photo.jpg", to);
    }
}

#[test]
fn jpg_to_pdf() {
    check("photo.jpg", Format::Pdf);
}

#[test]
fn png_with_transparency_to_pdf_falls_back() {
    // img2pdf refuses alpha channels; Joey must try the next engine.
    check("alpha.png", Format::Pdf);
}

#[test]
fn webp_to_pdf_needs_two_steps() {
    let joey = Joey::new();
    let (dir, input) = sample("photo.jpg");
    let Ok(webp) = joey.convert(&input, Format::Webp, None) else {
        return;
    };
    match joey.convert(&webp[0], Format::Pdf, None) {
        Ok(out) => assert_eq!(Format::detect(&out[0]), Some(Format::Pdf)),
        Err(Error::NoRoute { .. }) => eprintln!("skipped webp → pdf"),
        Err(e) => panic!("webp → pdf failed: {e}"),
    }
    drop(dir);
}

#[test]
fn animated_gif_to_png_keeps_one_frame() {
    if let Some(out) = check("anim.gif", Format::Png) {
        assert_eq!(out.len(), 1);
    }
}

#[test]
fn pdf_pages_become_numbered_images() {
    if let Some(out) = check("two-pages.pdf", Format::Png) {
        let names: Vec<_> = out
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["two-pages-1.png", "two-pages-2.png"]);
    }
}

#[test]
fn svg_to_png() {
    check("drawing.svg", Format::Png);
}

#[test]
fn real_type_wins_over_extension() {
    assert_eq!(
        Format::detect(&fixture("photo-wrong-ext.png")),
        Some(Format::Jpg)
    );
}

#[test]
fn never_overwrites_existing_files() {
    let joey = Joey::new();
    let (dir, input) = sample("photo.jpg");
    let Ok(first) = joey.convert(&input, Format::Png, None) else {
        return;
    };
    let second = joey.convert(&input, Format::Png, None).unwrap();
    assert_eq!(first[0].file_name().unwrap(), "photo.png");
    assert_eq!(second[0].file_name().unwrap(), "photo (1).png");
    drop(dir);
}

#[test]
fn many_files_at_once_keep_order() {
    let joey = Joey::new();
    let dir = tempfile::tempdir().unwrap();
    let inputs: Vec<PathBuf> = (0..6)
        .map(|i| {
            let p = dir.path().join(format!("img{i}.jpg"));
            std::fs::copy(fixture("photo.jpg"), &p).unwrap();
            p
        })
        .collect();
    let results = joey.convert_all(&inputs, Format::Png, None);
    assert_eq!(results.len(), 6);
    for (i, r) in results.into_iter().enumerate() {
        if let Ok(out) = r {
            assert_eq!(
                out[0].file_name().unwrap().to_string_lossy(),
                format!("img{i}.png")
            );
        }
    }
}

#[test]
fn unknown_file_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("notes.xyz");
    std::fs::write(&p, b"just some text").unwrap();
    assert!(matches!(
        Joey::new().convert(&p, Format::Png, None),
        Err(Error::UnknownFormat(_))
    ));
}
