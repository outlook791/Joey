//! Conversion settings. Every field has a sensible default, so most people
//! never see them; the "Adjust" screen exposes them in plain words.

/// Quality in words instead of codec numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Quality {
    /// Smallest files, visible loss.
    Low,
    /// Good for sharing online.
    Medium,
    /// Hard to tell from the original. The default.
    #[default]
    High,
    /// As close to the original as the format allows; biggest files.
    Max,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Max];

    /// Image quality, 0–100 (JPG, WebP, AVIF).
    pub fn image(self) -> u8 {
        match self {
            Quality::Low => 60,
            Quality::Medium => 75,
            Quality::High => 88,
            Quality::Max => 97,
        }
    }

    /// Audio bitrate in kbit/s for lossy formats.
    pub fn audio_kbps(self) -> u32 {
        match self {
            Quality::Low => 96,
            Quality::Medium => 128,
            Quality::High => 192,
            Quality::Max => 320,
        }
    }

    /// x264/x265 constant rate factor (lower is better).
    pub fn h264_crf(self) -> u8 {
        match self {
            Quality::Low => 30,
            Quality::Medium => 26,
            Quality::High => 22,
            Quality::Max => 17,
        }
    }

    /// VP9 constant rate factor (lower is better).
    pub fn vp9_crf(self) -> u8 {
        match self {
            Quality::Low => 42,
            Quality::Medium => 36,
            Quality::High => 31,
            Quality::Max => 24,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "high",
            Quality::Max => "max",
        }
    }

    pub fn from_key(s: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.key() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub quality: Quality,
    /// Longest side in pixels for images and video. None keeps the size.
    pub max_size: Option<u32>,
    /// Keep camera, location and author details. Default: keep.
    pub strip_metadata: bool,
    /// 1 = mono, 2 = stereo. None keeps the original.
    pub audio_channels: Option<u8>,
    /// Exact audio bitrate in kbit/s; overrides `quality` for audio.
    pub audio_kbps: Option<u32>,
    /// Samples per second (e.g. 44100). None keeps the original.
    pub sample_rate: Option<u32>,
    /// Frames per second for video. None keeps the original.
    pub fps: Option<u32>,
    /// Drop the sound from a video.
    pub no_audio: bool,
    /// PDF pages to use, e.g. "1-3, 7". None means all.
    pub pages: Option<String>,
    /// Sharpness of PDF pages turned into images. None means 150.
    pub dpi: Option<u32>,
}

impl Options {
    pub fn audio_kbps(&self) -> u32 {
        self.audio_kbps.unwrap_or_else(|| self.quality.audio_kbps())
    }

    pub fn dpi(&self) -> u32 {
        self.dpi.unwrap_or(150).clamp(36, 1200)
    }

    /// Parses `pages` into inclusive 1-based ranges. Invalid parts are
    /// ignored; an empty result means all pages.
    pub fn page_ranges(&self) -> Vec<(u32, u32)> {
        let Some(spec) = &self.pages else {
            return Vec::new();
        };
        spec.split([',', ';'])
            .filter_map(|part| {
                let part = part.trim();
                if part.is_empty() {
                    return None;
                }
                match part.split_once('-') {
                    Some((a, b)) => {
                        let a: u32 = a.trim().parse().ok()?;
                        let b: u32 = b.trim().parse().ok()?;
                        (a >= 1 && b >= a).then_some((a, b))
                    }
                    None => {
                        let n: u32 = part.parse().ok()?;
                        (n >= 1).then_some((n, n))
                    }
                }
            })
            .collect()
    }

    /// Serialises to `key=value` lines, for remembering settings.
    pub fn to_text(&self) -> String {
        let mut v = vec![format!("quality={}", self.quality.key())];
        let mut put = |k: &str, val: Option<String>| {
            if let Some(val) = val {
                v.push(format!("{k}={val}"));
            }
        };
        put("max_size", self.max_size.map(|x| x.to_string()));
        put("strip_metadata", self.strip_metadata.then(|| "1".into()));
        put("audio_channels", self.audio_channels.map(|x| x.to_string()));
        put("audio_kbps", self.audio_kbps.map(|x| x.to_string()));
        put("sample_rate", self.sample_rate.map(|x| x.to_string()));
        put("fps", self.fps.map(|x| x.to_string()));
        put("no_audio", self.no_audio.then(|| "1".into()));
        put("pages", self.pages.clone());
        put("dpi", self.dpi.map(|x| x.to_string()));
        v.join(";")
    }

    pub fn from_text(s: &str) -> Options {
        let mut o = Options::default();
        for pair in s.split(';') {
            let Some((k, v)) = pair.split_once('=') else {
                continue;
            };
            match k {
                "quality" => o.quality = Quality::from_key(v).unwrap_or_default(),
                "max_size" => o.max_size = v.parse().ok(),
                "strip_metadata" => o.strip_metadata = v == "1",
                "audio_channels" => o.audio_channels = v.parse().ok(),
                "audio_kbps" => o.audio_kbps = v.parse().ok(),
                "sample_rate" => o.sample_rate = v.parse().ok(),
                "fps" => o.fps = v.parse().ok(),
                "no_audio" => o.no_audio = v == "1",
                "pages" => o.pages = Some(v.to_string()),
                "dpi" => o.dpi = v.parse().ok(),
                _ => {}
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges_parse_forgivingly() {
        let o = Options {
            pages: Some("1-3, 7 ,x, 9-8".into()),
            ..Default::default()
        };
        assert_eq!(o.page_ranges(), vec![(1, 3), (7, 7)]);
    }

    #[test]
    fn text_round_trip() {
        let o = Options {
            quality: Quality::Low,
            max_size: Some(1920),
            strip_metadata: true,
            no_audio: true,
            pages: Some("2-4".into()),
            ..Default::default()
        };
        assert_eq!(Options::from_text(&o.to_text()), o);
        assert_eq!(Options::from_text(""), Options::default());
    }
}
