//! File formats Joey knows about, and detection of a file's real type.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Broad family of a format. The selector groups files by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Image,
    Vector,
    Document,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Image => "image",
            Category::Vector => "vector",
            Category::Document => "document",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    Jpg,
    Png,
    Webp,
    Avif,
    Heic,
    Gif,
    Tiff,
    Bmp,
    Ico,
    Svg,
    Pdf,
}

impl Format {
    pub const ALL: [Format; 11] = [
        Format::Jpg,
        Format::Png,
        Format::Webp,
        Format::Avif,
        Format::Heic,
        Format::Gif,
        Format::Tiff,
        Format::Bmp,
        Format::Ico,
        Format::Svg,
        Format::Pdf,
    ];

    /// Extension used for output files.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpg => "jpg",
            Format::Png => "png",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Heic => "heic",
            Format::Gif => "gif",
            Format::Tiff => "tiff",
            Format::Bmp => "bmp",
            Format::Ico => "ico",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
        }
    }

    /// Name shown to people.
    pub fn display_name(self) -> &'static str {
        match self {
            Format::Jpg => "JPG",
            Format::Png => "PNG",
            Format::Webp => "WebP",
            Format::Avif => "AVIF",
            Format::Heic => "HEIC",
            Format::Gif => "GIF",
            Format::Tiff => "TIFF",
            Format::Bmp => "BMP",
            Format::Ico => "ICO",
            Format::Svg => "SVG",
            Format::Pdf => "PDF",
        }
    }

    pub fn category(self) -> Category {
        match self {
            Format::Svg => Category::Vector,
            Format::Pdf => Category::Document,
            _ => Category::Image,
        }
    }

    /// Parses a user-typed name or extension ("jpeg", ".PNG", "tif").
    pub fn parse(name: &str) -> Option<Format> {
        let n = name.trim().trim_start_matches('.').to_ascii_lowercase();
        let f = match n.as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => Format::Jpg,
            "png" => Format::Png,
            "webp" => Format::Webp,
            "avif" => Format::Avif,
            "heic" | "heif" | "hif" => Format::Heic,
            "gif" => Format::Gif,
            "tif" | "tiff" => Format::Tiff,
            "bmp" | "dib" => Format::Bmp,
            "ico" => Format::Ico,
            "svg" => Format::Svg,
            "pdf" => Format::Pdf,
            _ => return None,
        };
        Some(f)
    }

    /// Detects the real format from the file's first bytes; falls back to
    /// the extension only when the content says nothing. A file that can't
    /// be read is never guessed from its name.
    pub fn detect(path: &Path) -> Option<Format> {
        let mut head = [0u8; 512];
        let n = File::open(path).and_then(|mut f| f.read(&mut head)).ok()?;
        from_magic(&head[..n]).or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .and_then(Format::parse)
        })
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

fn from_magic(b: &[u8]) -> Option<Format> {
    let starts = |sig: &[u8]| b.len() >= sig.len() && &b[..sig.len()] == sig;
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
    if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Some(Format::Webp);
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        // ISO-BMFF: the brand tells AVIF from HEIC.
        let brands = &b[8..b.len().min(64)];
        let has = |brand: &[u8]| brands.chunks(4).any(|c| c == brand);
        if has(b"avif") || has(b"avis") {
            return Some(Format::Avif);
        }
        if has(b"heic") || has(b"heix") || has(b"heim") || has(b"heis") || has(b"mif1") {
            return Some(Format::Heic);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aliases() {
        assert_eq!(Format::parse("JPEG"), Some(Format::Jpg));
        assert_eq!(Format::parse(".tif"), Some(Format::Tiff));
        assert_eq!(Format::parse("heif"), Some(Format::Heic));
        assert_eq!(Format::parse("docx"), None);
    }

    #[test]
    fn magic_beats_extension() {
        assert_eq!(from_magic(b"%PDF-1.7\n"), Some(Format::Pdf));
        assert_eq!(from_magic(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::Webp));
        assert_eq!(
            from_magic(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Some(Format::Heic)
        );
        assert_eq!(
            from_magic(b"\0\0\0\x1cftypavif\0\0\0\0avifmif1"),
            Some(Format::Avif)
        );
        assert_eq!(
            from_magic(b"<?xml version=\"1.0\"?><svg></svg>"),
            Some(Format::Svg)
        );
        assert_eq!(from_magic(b"hello"), None);
    }
}
