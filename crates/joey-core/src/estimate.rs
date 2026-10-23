//! Rough size of a result before converting, so people can choose
//! knowing what they'll get. Deliberately simple: a typical bytes-per-pixel
//! weight per format, scaled from the source size. Shown with a "~".

use crate::format::Format;

/// How heavy a format typically is for the same picture, relative to PNG.
fn weight(f: Format) -> Option<f64> {
    Some(match f {
        Format::Png => 1.0,
        Format::Jpg => 0.30,
        Format::Webp => 0.20,
        Format::Avif => 0.13,
        Format::Heic => 0.16,
        Format::Gif => 0.55,
        Format::Tiff => 2.6,
        Format::Bmp => 3.0,
        // Formats whose size doesn't follow the picture: no estimate.
        Format::Ico | Format::Svg | Format::Pdf => return None,
    })
}

/// Estimated size in bytes of converting a `source_bytes` file, or None
/// when no honest guess is possible (PDF pages, icons, drawings).
pub fn estimate(source_bytes: u64, from: Format, to: Format) -> Option<u64> {
    // An image placed in a PDF keeps roughly its own size.
    if to == Format::Pdf && weight(from).is_some() {
        return Some(source_bytes + 2_000);
    }
    let (wf, wt) = (weight(from)?, weight(to)?);
    Some(((source_bytes as f64) * wt / wf).round() as u64)
}

/// "~1.2 MB", "~340 KB".
pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_000_000.0 {
        format!("~{:.1} MB", b / 1_000_000.0)
    } else if b >= 1_000.0 {
        format!("~{:.0} KB", b / 1_000.0)
    } else {
        format!("~{bytes} B")
    }
}

/// Usual order to offer targets in: most wanted first.
pub fn popularity(f: Format) -> u8 {
    match f {
        Format::Jpg => 0,
        Format::Png => 1,
        Format::Pdf => 2,
        Format::Webp => 3,
        Format::Avif => 4,
        Format::Gif => 5,
        Format::Heic => 6,
        Format::Tiff => 7,
        Format::Bmp => 8,
        Format::Ico => 9,
        Format::Svg => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_to_jpg_is_smaller() {
        let e = estimate(1_000_000, Format::Png, Format::Jpg).unwrap();
        assert!(e < 1_000_000);
    }

    #[test]
    fn no_guess_for_pdf_pages() {
        assert_eq!(estimate(1_000_000, Format::Pdf, Format::Png), None);
    }

    #[test]
    fn formats_human_sizes() {
        assert_eq!(human(1_250_000), "~1.2 MB");
        assert_eq!(human(340_000), "~340 KB");
    }
}
