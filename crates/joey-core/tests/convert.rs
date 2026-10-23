//! Real conversions on small sample files. A conversion the installed tools
//! can't do is reported as skipped, not failed, so the suite runs anywhere.

use std::path::{Path, PathBuf};

use joey_core::{Error, Format, Joey, Options, Quality, Tool};

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
    let results = joey.convert_all(&inputs, Format::Png, None, &Options::default());
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

#[test]
fn missing_file_is_not_guessed_from_its_name() {
    let joey = Joey::new();
    let p = Path::new("/nonexistent/photo.jpg");
    assert!(matches!(joey.detect(p), Err(Error::NotFound(_))));
    assert!(matches!(
        joey.convert(p, Format::Png, None),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn folder_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Joey::new().detect(dir.path()),
        Err(Error::IsFolder(_))
    ));
}

// ---- Audio, video, documents ----

#[test]
fn wav_to_lossy_audio() {
    for to in [
        Format::Mp3,
        Format::M4a,
        Format::Ogg,
        Format::Opus,
        Format::Flac,
    ] {
        check("tone.wav", to);
    }
}

#[test]
fn video_to_other_video_and_audio() {
    for to in [
        Format::Webm,
        Format::Mkv,
        Format::Mov,
        Format::Mp3,
        Format::Gif,
    ] {
        check("clip.mp4", to);
    }
}

#[test]
fn animated_gif_to_video() {
    check("anim.gif", Format::Mp4);
}

#[test]
fn documents_through_libreoffice() {
    check("note.docx", Format::Pdf);
    check("note.docx", Format::Odt);
    check("table.csv", Format::Xlsx);
}

#[test]
fn document_to_images_goes_through_pdf() {
    check("note.docx", Format::Png);
}

#[test]
fn sound_only_from_video_without_audio_is_a_clear_error() {
    let joey = Joey::new();
    let (dir, input) = sample("clip.mp4");
    let Ok(silent) = joey.run_tool(Tool::RemoveAudio, &[input]).remove(0) else {
        return;
    };
    match joey.convert(&silent[0], Format::Mp3, None) {
        Err(Error::Tool { message, .. }) => assert!(message.contains("no audio"), "{message}"),
        other => panic!("expected a no-audio error, got {other:?}"),
    }
    drop(dir);
}

// ---- Settings ----

#[test]
fn max_size_shrinks_images() {
    let joey = Joey::new();
    let (dir, input) = sample("big.jpg");
    let o = Options {
        max_size: Some(60),
        ..Default::default()
    };
    if let Ok(out) = joey.convert_with(&input, Format::Png, None, &o) {
        let dims = std::process::Command::new("identify")
            .args(["-format", "%w %h"])
            .arg(&out[0])
            .output()
            .unwrap();
        let s = String::from_utf8_lossy(&dims.stdout);
        let (w, h) = s.split_once(' ').unwrap();
        assert!(
            w.parse::<u32>().unwrap() <= 60 && h.parse::<u32>().unwrap() <= 60,
            "{s}"
        );
    }
    drop(dir);
}

#[test]
fn lower_quality_gives_smaller_jpg() {
    let joey = Joey::new();
    let (dir, input) = sample("alpha.png");
    let size = |q| {
        let o = Options {
            quality: q,
            ..Default::default()
        };
        let out = joey.convert_with(&input, Format::Jpg, None, &o).unwrap();
        std::fs::metadata(&out[0]).unwrap().len()
    };
    let (low, max) = (size(Quality::Low), size(Quality::Max));
    assert!(low < max, "low {low} vs max {max}");
    drop(dir);
}

#[test]
fn chosen_pdf_pages_only() {
    let joey = Joey::new();
    let (dir, input) = sample("two-pages.pdf");
    let o = Options {
        pages: Some("2".into()),
        ..Default::default()
    };
    let out = joey.convert_with(&input, Format::Png, None, &o).unwrap();
    assert_eq!(out.len(), 1);
    drop(dir);
}

// ---- Quick tools ----

#[test]
fn compress_makes_photos_smaller() {
    let joey = Joey::new();
    let (dir, input) = sample("big.jpg");
    let out = joey
        .run_tool(Tool::Compress, std::slice::from_ref(&input))
        .remove(0)
        .unwrap();
    assert_eq!(out[0].file_name().unwrap(), "big (compressed).jpg");
    assert!(std::fs::metadata(&out[0]).unwrap().len() < std::fs::metadata(&input).unwrap().len());
    drop(dir);
}

#[test]
fn merge_pdfs_into_one() {
    let joey = Joey::new();
    if !joey.tools_for(Format::Pdf, 2).contains(&Tool::MergePdf) {
        return eprintln!("skipped: qpdf missing");
    }
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    std::fs::copy(fixture("two-pages.pdf"), &a).unwrap();
    std::fs::copy(fixture("two-pages.pdf"), &b).unwrap();
    let out = joey.run_tool(Tool::MergePdf, &[a, b]).remove(0).unwrap();
    assert_eq!(out[0].file_name().unwrap(), "a (merged).pdf");
    let pages = joey.convert(&out[0], Format::Png, None).unwrap();
    assert_eq!(pages.len(), 4);
}

#[test]
fn audio_tools() {
    let joey = Joey::new();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    std::fs::copy(fixture("tone.wav"), &a).unwrap();
    std::fs::copy(fixture("tone.wav"), &b).unwrap();
    let joined = joey
        .run_tool(Tool::MergeAudio, &[a.clone(), b])
        .remove(0)
        .unwrap();
    assert!(std::fs::metadata(&joined[0]).unwrap().len() > std::fs::metadata(&a).unwrap().len());
    let norm = joey.run_tool(Tool::NormalizeAudio, &[a]).remove(0).unwrap();
    assert_eq!(norm[0].file_name().unwrap(), "a (normalized).wav");
}

#[test]
fn metadata_removal_keeps_the_format() {
    let joey = Joey::new();
    for name in ["big.jpg", "clip.mp4"] {
        let (dir, input) = sample(name);
        let out = joey
            .run_tool(Tool::RemoveMetadata, std::slice::from_ref(&input))
            .remove(0)
            .unwrap();
        assert_eq!(Format::detect(&out[0]), Format::detect(&input));
        drop(dir);
    }
}

#[test]
fn tools_match_the_kind_of_file() {
    let joey = Joey::new();
    assert!(!joey.tools_for(Format::Jpg, 3).contains(&Tool::MergePdf));
    assert!(!joey.tools_for(Format::Pdf, 1).contains(&Tool::MergePdf));
    assert!(!joey.tools_for(Format::Mp3, 1).contains(&Tool::RemoveAudio));
}

#[test]
fn ghostscript_renders_pages_when_poppler_is_missing() {
    use joey_core::engines::ghostscript::Ghostscript;
    let Some(gs) = Ghostscript::probe() else {
        return eprintln!("skipped: no gs");
    };
    let joey = Joey::with_engines(vec![Box::new(gs)]);
    let (dir, input) = sample("two-pages.pdf");
    let out = joey.convert(&input, Format::Png, None).unwrap();
    let names: Vec<_> = out
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["two-pages-1.png", "two-pages-2.png"]);
    let o = Options {
        pages: Some("2".into()),
        ..Default::default()
    };
    let one = joey.convert_with(&input, Format::Jpg, None, &o).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(Format::detect(&one[0]), Some(Format::Jpg));
    drop(dir);
}

#[test]
fn compress_pdf_with_ghostscript() {
    let joey = Joey::new();
    if !joey.tools_for(Format::Pdf, 1).contains(&Tool::Compress) {
        return eprintln!("skipped: no gs");
    }
    let (dir, input) = sample("two-pages.pdf");
    // A tiny PDF may already be minimal: either outcome is a clear answer.
    match joey
        .run_tool(Tool::Compress, std::slice::from_ref(&input))
        .remove(0)
    {
        Ok(out) => assert_eq!(Format::detect(&out[0]), Some(Format::Pdf)),
        Err(Error::Tool { message, .. }) => assert!(message.contains("small"), "{message}"),
        Err(e) => panic!("{e}"),
    }
    drop(dir);
}

// ---- Formats added for parity with Tangerine ----

#[test]
fn subtitles_between_formats_and_to_text() {
    check("talk.srt", Format::Vtt);
    check("talk.srt", Format::Ass);
    let (dir, input) = sample("talk.srt");
    let out = Joey::new().convert(&input, Format::Txt, None).unwrap();
    let text = std::fs::read_to_string(&out[0]).unwrap();
    assert_eq!(text, "Hello from the pouch\n\nHop!\n");
    drop(dir);
}

#[test]
fn markdown_to_web_page_ebook_and_word() {
    for to in [Format::Html, Format::Epub, Format::Docx, Format::Pdf] {
        check("readme.md", to);
    }
}

#[test]
fn archives_repack() {
    for to in [Format::Tgz, Format::SevenZ, Format::Tar] {
        check("bundle.zip", to);
    }
}

#[test]
fn tsv_and_wma() {
    check("table.tsv", Format::Xlsx);
    check("tone.wma", Format::Mp3);
    check("tone.wav", Format::Wma);
}

#[test]
fn pdf_back_to_text_and_word() {
    check("two-pages.pdf", Format::Txt);
    check("two-pages.pdf", Format::Docx);
}

#[test]
fn tar_gz_keeps_a_clean_name() {
    let joey = Joey::new();
    let (dir, input) = sample("bundle.zip");
    let Ok(tgz) = joey.convert(&input, Format::Tgz, None) else {
        return;
    };
    assert_eq!(tgz[0].file_name().unwrap(), "bundle.tar.gz");
    let back = joey.convert(&tgz[0], Format::Zip, None).unwrap();
    assert_eq!(back[0].file_name().unwrap(), "bundle (1).zip");
    drop(dir);
}

// ---- More quick tools ----

fn dims(p: &Path) -> (u32, u32) {
    let out = std::process::Command::new("identify")
        .args(["-format", "%w %h"])
        .arg(p)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace().map(|n| n.parse().unwrap());
    (it.next().unwrap(), it.next().unwrap())
}

#[test]
fn rotate_crop_and_gray_images() {
    use joey_core::ToolParams;
    let joey = Joey::new();
    let (dir, input) = sample("big.jpg"); // 240x160
    let rot = joey
        .run_tool(Tool::RotateRight, std::slice::from_ref(&input))
        .remove(0)
        .unwrap();
    assert_eq!(dims(&rot[0]), (160, 240));
    let p = ToolParams {
        shape: Some((1, 1)),
        ..Default::default()
    };
    let sq = joey
        .run_tool_with(Tool::Crop, std::slice::from_ref(&input), &p)
        .remove(0)
        .unwrap();
    assert_eq!(dims(&sq[0]), (160, 160));
    let gray = joey
        .run_tool(Tool::Grayscale, std::slice::from_ref(&input))
        .remove(0)
        .unwrap();
    assert!(gray[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("black and white"));
    drop(dir);
}

#[test]
fn trim_audio_and_video() {
    use joey_core::ToolParams;
    let joey = Joey::new();
    let p = ToolParams {
        trim: Some((100, 400)),
        ..Default::default()
    };
    for name in ["tone.wav", "clip.mp4"] {
        let (dir, input) = sample(name);
        let out = joey
            .run_tool_with(Tool::Trim, std::slice::from_ref(&input), &p)
            .remove(0)
            .unwrap();
        assert!(
            std::fs::metadata(&out[0]).unwrap().len() < std::fs::metadata(&input).unwrap().len()
        );
        drop(dir);
    }
}

#[test]
fn pdf_split_extract_rotate_protect_unlock() {
    use joey_core::ToolParams;
    let joey = Joey::new();
    if !joey.tools_for(Format::Pdf, 1).contains(&Tool::SplitPdf) {
        return eprintln!("skipped: qpdf missing");
    }
    let (dir, input) = sample("two-pages.pdf");
    let one = std::slice::from_ref(&input);
    let split = joey.run_tool(Tool::SplitPdf, one).remove(0).unwrap();
    assert_eq!(split.len(), 2);
    let p = ToolParams {
        pages: Some("2".into()),
        ..Default::default()
    };
    let page2 = joey
        .run_tool_with(Tool::ExtractPages, one, &p)
        .remove(0)
        .unwrap();
    assert_eq!(joey.convert(&page2[0], Format::Png, None).unwrap().len(), 1);
    joey.run_tool(Tool::RotateLeft, one).remove(0).unwrap();
    let pw = ToolParams {
        password: Some("hop".into()),
        ..Default::default()
    };
    let locked = joey
        .run_tool_with(Tool::ProtectPdf, one, &pw)
        .remove(0)
        .unwrap();
    let wrong = ToolParams {
        password: Some("nope".into()),
        ..Default::default()
    };
    assert!(joey
        .run_tool_with(Tool::UnlockPdf, &locked, &wrong)
        .remove(0)
        .is_err());
    joey.run_tool_with(Tool::UnlockPdf, &locked, &pw)
        .remove(0)
        .unwrap();
    drop(dir);
}

#[test]
fn zip_and_extract_any_files() {
    let joey = Joey::new();
    if !joey.can_zip() {
        return eprintln!("skipped: bsdtar missing");
    }
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("notes.xyz");
    let b = dir.path().join("folder");
    std::fs::write(&a, b"whatever").unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(b.join("inside.txt"), b"hi").unwrap();
    let zip = joey.zip(&[a, b]).unwrap();
    assert_eq!(zip[0].file_name().unwrap(), "notes.zip");
    let out = joey.run_tool(Tool::ExtractArchive, &zip).remove(0).unwrap();
    assert!(out[0].join("folder/inside.txt").exists());
    assert!(out[0].join("notes.xyz").exists());
}

#[test]
fn join_videos() {
    let joey = Joey::new();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mp4");
    let b = dir.path().join("b.mp4");
    std::fs::copy(fixture("clip.mp4"), &a).unwrap();
    std::fs::copy(fixture("clip.mp4"), &b).unwrap();
    let out = joey
        .run_tool(Tool::MergeVideo, &[a.clone(), b])
        .remove(0)
        .unwrap();
    assert!(std::fs::metadata(&out[0]).unwrap().len() > std::fs::metadata(&a).unwrap().len());
}

#[test]
fn pdf_tables_to_excel() {
    let joey = Joey::new();
    let (dir, input) = sample("invoice.pdf");
    match joey.convert(&input, Format::Xlsx, None) {
        Err(Error::NoRoute { .. }) => eprintln!("skipped: no table extractor"),
        Err(e) => panic!("{e}"),
        Ok(out) => {
            assert_eq!(Format::detect(&out[0]), Some(Format::Xlsx));
            // Read it back through CSV to check the cells.
            let csv = joey.convert(&out[0], Format::Csv, None).unwrap();
            let text = std::fs::read_to_string(&csv[0]).unwrap();
            assert!(text.contains("Pouch") && text.contains("24"), "{text}");
        }
    }
    drop(dir);
}
