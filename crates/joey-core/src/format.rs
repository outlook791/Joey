//! File formats Joey knows about, and detection of a file's real type.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Broad family of a format. The selector groups files by format and
/// shows this as the kind of file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Image,
    Vector,
    Audio,
    Video,
    Pdf,
    Document,
    Spreadsheet,
    Presentation,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Image => "image",
            Category::Vector => "drawing",
            Category::Audio => "audio",
            Category::Video => "video",
            Category::Pdf => "PDF",
            Category::Document => "document",
            Category::Spreadsheet => "spreadsheet",
            Category::Presentation => "presentation",
        }
    }
}

macro_rules! formats {
    ($( $v:ident => $ext:literal, $name:literal, $cat:ident, [$($alias:literal),*] ;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Format { $($v),* }

        impl Format {
            pub const ALL: &'static [Format] = &[$(Format::$v),*];

            /// Extension used for output files.
            pub fn extension(self) -> &'static str {
                match self { $(Format::$v => $ext),* }
            }

            /// Name shown to people.
            pub fn display_name(self) -> &'static str {
                match self { $(Format::$v => $name),* }
            }

            pub fn category(self) -> Category {
                match self { $(Format::$v => Category::$cat),* }
            }

            /// Parses a user-typed name or extension ("jpeg", ".PNG", "tif").
            pub fn parse(name: &str) -> Option<Format> {
                let n = name.trim().trim_start_matches('.').to_ascii_lowercase();
                match n.as_str() {
                    $($ext $(| $alias)* => Some(Format::$v),)*
                    _ => None,
                }
            }
        }
    };
}

formats! {
    Jpg  => "jpg",  "JPG",  Image, ["jpeg", "jpe", "jfif"];
    Png  => "png",  "PNG",  Image, [];
    Webp => "webp", "WebP", Image, [];
    Avif => "avif", "AVIF", Image, [];
    Heic => "heic", "HEIC", Image, ["heif", "hif"];
    Gif  => "gif",  "GIF",  Image, [];
    Tiff => "tiff", "TIFF", Image, ["tif"];
    Bmp  => "bmp",  "BMP",  Image, ["dib"];
    Ico  => "ico",  "ICO",  Image, [];
    Svg  => "svg",  "SVG",  Vector, [];
    Mp3  => "mp3",  "MP3",  Audio, [];
    M4a  => "m4a",  "M4A (AAC)", Audio, ["aac", "m4b"];
    Ogg  => "ogg",  "OGG",  Audio, ["oga"];
    Opus => "opus", "Opus", Audio, [];
    Flac => "flac", "FLAC", Audio, [];
    Wav  => "wav",  "WAV",  Audio, ["wave"];
    Aiff => "aiff", "AIFF", Audio, ["aif", "aifc"];
    Mp4  => "mp4",  "MP4",  Video, ["m4v"];
    Mkv  => "mkv",  "MKV",  Video, [];
    Webm => "webm", "WebM", Video, [];
    Mov  => "mov",  "MOV",  Video, ["qt"];
    Avi  => "avi",  "AVI",  Video, [];
    Pdf  => "pdf",  "PDF",  Pdf, [];
    Docx => "docx", "Word (DOCX)", Document, [];
    Doc  => "doc",  "Word 97 (DOC)", Document, [];
    Odt  => "odt",  "ODT",  Document, [];
    Rtf  => "rtf",  "RTF",  Document, [];
    Txt  => "txt",  "Text (TXT)", Document, ["text"];
    Xlsx => "xlsx", "Excel (XLSX)", Spreadsheet, [];
    Xls  => "xls",  "Excel 97 (XLS)", Spreadsheet, [];
    Ods  => "ods",  "ODS",  Spreadsheet, [];
    Csv  => "csv",  "CSV",  Spreadsheet, [];
    Pptx => "pptx", "PowerPoint (PPTX)", Presentation, [];
    Ppt  => "ppt",  "PowerPoint 97 (PPT)", Presentation, [];
    Odp  => "odp",  "ODP",  Presentation, [];
}

impl Format {
    /// Detects the real format from the file's content; falls back to the
    /// extension only when the content says nothing (or, for old Office
    /// files and plain text, can't tell the variants apart). A file that
    /// can't be read is never guessed from its name.
    pub fn detect(path: &Path) -> Option<Format> {
        let mut head = Vec::with_capacity(4096);
        File::open(path)
            .ok()?
            .take(64 * 1024)
            .read_to_end(&mut head)
            .ok()?;
        let by_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Format::parse);
        if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
            // Old Office container: same signature for DOC, XLS and PPT.
            return by_ext.filter(|f| matches!(f, Format::Doc | Format::Xls | Format::Ppt));
        }
        from_magic(&head).or_else(|| {
            // Text-only formats have no signature: trust the name only if
            // the content really is text.
            let is_text = std::str::from_utf8(&head).is_ok() && !head.contains(&0);
            by_ext.filter(|f| !matches!(f, Format::Txt | Format::Csv) || is_text)
        })
    }
}

/// Whether turning `from` into `to` is something a person would want.
/// The graph can chain tools in technically valid but silly ways (a photo
/// into a song via a GIF and a video); this keeps the offer sensible.
pub fn sensible(from: Format, to: Format) -> bool {
    use Category::*;
    if from == to {
        return false;
    }
    let pages_as_images = matches!(to, Format::Png | Format::Jpg | Format::Webp | Format::Tiff);
    match (from.category(), to.category()) {
        (Image, Image) | (Image, Pdf) => true,
        // An animated GIF can become a real video.
        (Image, Video) => from == Format::Gif,
        (Vector, Image) | (Vector, Pdf) => true,
        (Audio, Audio) => true,
        (Video, Video) | (Video, Audio) => true,
        (Video, Image) => to == Format::Gif,
        (Pdf, Image) => pages_as_images,
        (Document, Document) | (Spreadsheet, Spreadsheet) | (Presentation, Presentation) => true,
        (Document | Spreadsheet | Presentation, Pdf) => true,
        (Document | Spreadsheet | Presentation, Image) => matches!(to, Format::Png | Format::Jpg),
        _ => false,
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn from_magic(b: &[u8]) -> Option<Format> {
    let starts = |sig: &[u8]| b.starts_with(sig);
    let at = |off: usize, sig: &[u8]| b.len() >= off + sig.len() && &b[off..off + sig.len()] == sig;

    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpg);
    }
    if starts(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(Format::Png);
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(Format::Gif);
    }
    if starts(b"%PDF-") {
        return Some(Format::Pdf);
    }
    if starts(b"II*\0") || starts(b"MM\0*") {
        return Some(Format::Tiff);
    }
    if starts(b"BM") && b.len() >= 14 {
        return Some(Format::Bmp);
    }
    if starts(&[0, 0, 1, 0]) && b.len() >= 6 && b[4] > 0 {
        return Some(Format::Ico);
    }
    if starts(b"RIFF") {
        if at(8, b"WEBP") {
            return Some(Format::Webp);
        }
        if at(8, b"WAVE") {
            return Some(Format::Wav);
        }
        if at(8, b"AVI ") {
            return Some(Format::Avi);
        }
    }
    if starts(b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        return Some(Format::Aiff);
    }
    if starts(b"fLaC") {
        return Some(Format::Flac);
    }
    if starts(b"OggS") {
        let head = &b[..b.len().min(128)];
        return Some(if contains(head, b"OpusHead") {
            Format::Opus
        } else {
            Format::Ogg
        });
    }
    if starts(b"ID3") || (b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE6) == 0xE2) {
        // ID3 tag, or an MPEG audio layer III frame header.
        return Some(Format::Mp3);
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let head = &b[..b.len().min(64)];
        return Some(if contains(head, b"webm") {
            Format::Webm
        } else {
            Format::Mkv
        });
    }
    if at(4, b"ftyp") {
        // ISO-BMFF: the brands tell images, audio and video apart.
        let brands = &b[8..b.len().min(64)];
        let has = |brand: &[u8]| brands.chunks(4).any(|c| c == brand);
        if has(b"avif") || has(b"avis") {
            return Some(Format::Avif);
        }
        if has(b"heic") || has(b"heix") || has(b"heim") || has(b"heis") || has(b"mif1") {
            return Some(Format::Heic);
        }
        if has(b"M4A ") || has(b"M4B ") {
            return Some(Format::M4a);
        }
        if has(b"qt  ") {
            return Some(Format::Mov);
        }
        return Some(Format::Mp4);
    }
    if starts(b"{\\rtf") {
        return Some(Format::Rtf);
    }
    if starts(b"PK\x03\x04") {
        return zip_kind(b);
    }
    if let Ok(text) = std::str::from_utf8(&b[..b.len().min(512)]) {
        let t = text.trim_start_matches('\u{feff}').trim_start();
        if (t.starts_with("<?xml") || t.starts_with("<svg") || t.starts_with("<!--"))
            && text.contains("<svg")
        {
            return Some(Format::Svg);
        }
    }
    None
}

/// Office files are ZIP archives; the names inside say which kind.
fn zip_kind(b: &[u8]) -> Option<Format> {
    if at_odf(b, b"application/vnd.oasis.opendocument.text") {
        return Some(Format::Odt);
    }
    if at_odf(b, b"application/vnd.oasis.opendocument.spreadsheet") {
        return Some(Format::Ods);
    }
    if at_odf(b, b"application/vnd.oasis.opendocument.presentation") {
        return Some(Format::Odp);
    }
    if contains(b, b"word/") {
        return Some(Format::Docx);
    }
    if contains(b, b"xl/") {
        return Some(Format::Xlsx);
    }
    if contains(b, b"ppt/") {
        return Some(Format::Pptx);
    }
    None
}

/// OpenDocument stores its type uncompressed in the first entry.
fn at_odf(b: &[u8], mime: &[u8]) -> bool {
    let head = &b[..b.len().min(200)];
    contains(head, b"mimetype") && contains(head, mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aliases() {
        assert_eq!(Format::parse("JPEG"), Some(Format::Jpg));
        assert_eq!(Format::parse(".tif"), Some(Format::Tiff));
        assert_eq!(Format::parse("heif"), Some(Format::Heic));
        assert_eq!(Format::parse("aac"), Some(Format::M4a));
        assert_eq!(Format::parse("exe"), None);
    }

    #[test]
    fn magic_beats_extension() {
        assert_eq!(from_magic(b"%PDF-1.7\n"), Some(Format::Pdf));
        assert_eq!(from_magic(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::Webp));
        assert_eq!(from_magic(b"RIFF\0\0\0\0WAVEfmt "), Some(Format::Wav));
        assert_eq!(
            from_magic(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Some(Format::Heic)
        );
        assert_eq!(
            from_magic(b"\0\0\0\x1cftypavif\0\0\0\0avifmif1"),
            Some(Format::Avif)
        );
        assert_eq!(
            from_magic(b"\0\0\0\x20ftypM4A \0\0\0\0M4A isom"),
            Some(Format::M4a)
        );
        assert_eq!(
            from_magic(b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2mp41"),
            Some(Format::Mp4)
        );
        assert_eq!(
            from_magic(b"\0\0\0\x14ftypqt  \0\0\0\0qt  "),
            Some(Format::Mov)
        );
        assert_eq!(
            from_magic(b"OggS\0\x02\0\0\0\0\0\0\0\0OpusHead"),
            Some(Format::Opus)
        );
        assert_eq!(from_magic(b"ID3\x04\0\0\0\0"), Some(Format::Mp3));
        assert_eq!(from_magic(b"{\\rtf1\\ansi"), Some(Format::Rtf));
        assert_eq!(
            from_magic(b"<?xml version=\"1.0\"?><svg></svg>"),
            Some(Format::Svg)
        );
        assert_eq!(from_magic(b"hello"), None);
    }

    #[test]
    fn keeps_offers_sensible() {
        assert!(sensible(Format::Mp4, Format::Mp3));
        assert!(sensible(Format::Gif, Format::Mp4));
        assert!(!sensible(Format::Jpg, Format::Mp4));
        assert!(!sensible(Format::Jpg, Format::Mp3));
        assert!(!sensible(Format::Mp4, Format::Png));
        assert!(!sensible(Format::Pdf, Format::Ico));
        assert!(sensible(Format::Docx, Format::Png));
    }

    #[test]
    fn every_format_round_trips_through_parse() {
        for &f in Format::ALL {
            assert_eq!(Format::parse(f.extension()), Some(f));
        }
    }
}
