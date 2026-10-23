//! Rough size of a result before converting, so people can choose
//! knowing what they'll get. Deliberately simple: a typical weight per
//! format for the same content, scaled from the source size and the chosen
//! quality. Always shown with a "~". No guess where no honest one exists
//! (video, documents, PDF pages, icons).

use crate::format::Format;
use crate::options::{Options, Quality};

/// How heavy an image format typically is for the same picture vs PNG.
fn image_weight(f: Format) -> Option<f64> {
    Some(match f {
        Format::Png => 1.0,
        Format::Jpg => 0.30,
        Format::Webp => 0.20,
        Format::Avif => 0.13,
        Format::Heic => 0.16,
        Format::Gif => 0.55,
        Format::Tiff => 2.6,
        Format::Bmp => 3.0,
        _ => return None,
    })
}

/// kbit/s of a typical file in this audio format.
fn audio_kbps(f: Format) -> Option<f64> {
    Some(match f {
        Format::Wav | Format::Aiff => 1411.0,
        Format::Flac => 800.0,
        Format::Mp3 | Format::M4a | Format::Ogg => 192.0,
        Format::Opus => 128.0,
        _ => return None,
    })
}

fn lossy_image(f: Format) -> bool {
    matches!(f, Format::Jpg | Format::Webp | Format::Avif | Format::Heic)
}

fn quality_factor(q: Quality) -> f64 {
    match q {
        Quality::Low => 0.45,
        Quality::Medium => 0.7,
        Quality::High => 1.0,
        Quality::Max => 1.7,
    }
}

/// Estimated size with default settings.
pub fn estimate(source_bytes: u64, from: Format, to: Format) -> Option<u64> {
    estimate_with(source_bytes, from, to, &Options::default())
}

/// Estimated size in bytes of converting a `source_bytes` file with these
/// settings, or None when no honest guess is possible.
pub fn estimate_with(source_bytes: u64, from: Format, to: Format, o: &Options) -> Option<u64> {
    let src = source_bytes as f64;
    // An image placed in a PDF keeps roughly its own size.
    if to == Format::Pdf && image_weight(from).is_some() {
        return Some(source_bytes + 2_000);
    }
    if let (Some(wf), Some(wt)) = (image_weight(from), image_weight(to)) {
        let mut est = src * wt / wf;
        if lossy_image(to) {
            est *= quality_factor(o.quality);
        }
        return Some(est.round() as u64);
    }
    if let (Some(kf), Some(kt)) = (audio_kbps(from), audio_kbps(to)) {
        let kt = if matches!(to, Format::Wav | Format::Aiff | Format::Flac) {
            kt
        } else {
            o.audio_kbps() as f64
        };
        let channels = match o.audio_channels {
            Some(1) => 0.5,
            _ => 1.0,
        };
        let lossless_target = matches!(to, Format::Wav | Format::Aiff | Format::Flac);
        let factor = if lossless_target { channels } else { 1.0 };
        return Some((src * kt / kf * factor).round() as u64);
    }
    None
}

/// "~1.2 MB", "~340 KB".
pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_000_000_000.0 {
        format!("~{:.1} GB", b / 1_000_000_000.0)
    } else if b >= 1_000_000.0 {
        format!("~{:.1} MB", b / 1_000_000.0)
    } else if b >= 1_000.0 {
        format!("~{:.0} KB", b / 1_000.0)
    } else {
        format!("~{bytes} B")
    }
}

/// Usual order to offer targets in: most wanted first.
pub fn popularity(f: Format) -> u16 {
    const ORDER: &[Format] = &[
        Format::Jpg,
        Format::Png,
        Format::Pdf,
        Format::Mp3,
        Format::Mp4,
        Format::Docx,
        Format::Webp,
        Format::M4a,
        Format::Xlsx,
        Format::Pptx,
        Format::Gif,
        Format::Avif,
        Format::Wav,
        Format::Flac,
        Format::Ogg,
        Format::Opus,
        Format::Webm,
        Format::Mkv,
        Format::Mov,
        Format::Odt,
        Format::Ods,
        Format::Odp,
        Format::Txt,
        Format::Csv,
        Format::Rtf,
        Format::Heic,
        Format::Tiff,
        Format::Aiff,
        Format::Avi,
        Format::Bmp,
        Format::Ico,
    ];
    ORDER
        .iter()
        .position(|&o| o == f)
        .map(|p| p as u16)
        .unwrap_or(999)
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
    fn lower_quality_is_smaller() {
        let low = Options {
            quality: Quality::Low,
            ..Default::default()
        };
        let a = estimate_with(1_000_000, Format::Png, Format::Jpg, &low).unwrap();
        let b = estimate(1_000_000, Format::Png, Format::Jpg).unwrap();
        assert!(a < b);
    }

    #[test]
    fn wav_to_mp3_shrinks_a_lot() {
        let e = estimate(50_000_000, Format::Wav, Format::Mp3).unwrap();
        assert!(e < 8_000_000);
    }

    #[test]
    fn no_guess_where_unknowable() {
        assert_eq!(estimate(1_000_000, Format::Pdf, Format::Png), None);
        assert_eq!(estimate(1_000_000, Format::Mp4, Format::Webm), None);
        assert_eq!(estimate(1_000_000, Format::Docx, Format::Pdf), None);
    }

    #[test]
    fn formats_human_sizes() {
        assert_eq!(human(1_250_000), "~1.2 MB");
        assert_eq!(human(340_000), "~340 KB");
    }
}
