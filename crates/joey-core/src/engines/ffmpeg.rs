//! Audio and video through FFmpeg. Formats are offered only when the
//! installed FFmpeg has the encoder they need.

use std::path::PathBuf;
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::format::{Category, Format};
use crate::options::Options;
use crate::Error;

pub struct Ffmpeg {
    bin: PathBuf,
    encoders: Vec<String>,
    edges: Vec<(Format, Format)>,
}

impl Ffmpeg {
    pub fn probe() -> Option<Self> {
        let bin = which("ffmpeg")?;
        let list = run(
            "ffmpeg",
            Command::new(&bin).args(["-hide_banner", "-encoders"]),
        )
        .ok()?;
        let encoders: Vec<String> = list
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
            .collect();
        let mut me = Self {
            bin,
            encoders,
            edges: Vec::new(),
        };

        let audio: Vec<Format> = Format::ALL
            .iter()
            .copied()
            .filter(|f| f.category() == Category::Audio)
            .collect();
        let video: Vec<Format> = Format::ALL
            .iter()
            .copied()
            .filter(|f| f.category() == Category::Video)
            .collect();
        let mut edges = Vec::new();
        for &to in audio.iter().filter(|&&t| me.audio_codec(t).is_some()) {
            for &from in audio.iter().chain(&video) {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        for &to in video.iter().filter(|&&t| me.video_codec(t).is_some()) {
            for &from in video.iter().chain([&Format::Gif]) {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        let subs: Vec<Format> = [Format::Srt, Format::Vtt, Format::Ass]
            .into_iter()
            .filter(|&f| me.subtitle_codec(f).is_some())
            .collect();
        for &to in &subs {
            for &from in &[Format::Srt, Format::Vtt, Format::Ass] {
                if from != to {
                    edges.push((from, to));
                }
            }
        }
        if me.has("gif") {
            for &from in &video {
                edges.push((from, Format::Gif));
            }
        }
        me.edges = edges;
        Some(me)
    }

    pub fn bin(&self) -> &std::path::Path {
        &self.bin
    }

    fn has(&self, encoder: &str) -> bool {
        self.encoders.iter().any(|e| e == encoder)
    }

    fn first<'a>(&self, options: &[&'a str]) -> Option<&'a str> {
        options.iter().copied().find(|e| self.has(e))
    }

    pub(crate) fn audio_codec(&self, to: Format) -> Option<&'static str> {
        match to {
            Format::Mp3 => self.first(&["libmp3lame"]),
            Format::M4a => self.first(&["aac", "libfdk_aac"]),
            Format::Ogg => self.first(&["libvorbis", "vorbis"]),
            Format::Opus => self.first(&["libopus", "opus"]),
            Format::Flac => self.first(&["flac"]),
            Format::Wav => self.first(&["pcm_s16le"]),
            Format::Aiff => self.first(&["pcm_s16be"]),
            Format::Wma => self.first(&["wmav2"]),
            Format::Wmv => self.first(&["wmav2"]),
            Format::Avi => self.first(&["libmp3lame", "aac"]),
            Format::Webm => self.first(&["libopus", "libvorbis"]),
            Format::Mkv => self.first(&["libopus", "aac"]),
            Format::Mp4 | Format::Mov => self.first(&["aac"]),
            _ => None,
        }
    }

    fn subtitle_codec(&self, to: Format) -> Option<&'static str> {
        match to {
            Format::Srt => self.first(&["srt", "subrip"]),
            Format::Vtt => self.first(&["webvtt"]),
            Format::Ass => self.first(&["ass", "ssa"]),
            _ => None,
        }
    }

    fn video_codec(&self, to: Format) -> Option<&'static str> {
        match to {
            Format::Mp4 | Format::Mov | Format::Mkv => {
                self.first(&["libx264", "libopenh264", "mpeg4"])
            }
            Format::Webm => self.first(&["libvpx-vp9", "libvpx"]),
            Format::Avi => self.first(&["mpeg4", "libx264"]),
            Format::Wmv => self.first(&["wmv2"]),
            _ => None,
        }
    }
}

fn lossless(codec: &str) -> bool {
    codec.starts_with("pcm_") || codec == "flac"
}

fn audio_args(cmd: &mut Command, codec: &str, to: Format, o: &Options) {
    cmd.args(["-c:a", codec]);
    if codec == "libvorbis" {
        // Vorbis rejects bitrates its sample rate can't carry; its quality
        // scale always works.
        let q = match o.audio_kbps() {
            0..=100 => 3,
            101..=140 => 4,
            141..=200 => 6,
            201..=260 => 8,
            _ => 9,
        };
        cmd.args(["-q:a", &q.to_string()]);
    } else if !lossless(codec) {
        let mut kbps = o.audio_kbps();
        if to == Format::Opus || codec == "libopus" {
            kbps = kbps.min(256);
        }
        cmd.args(["-b:a", &format!("{kbps}k")]);
    }
    if let Some(ch) = o.audio_channels {
        cmd.args(["-ac", &ch.to_string()]);
    }
    if let Some(sr) = o.sample_rate {
        cmd.args(["-ar", &sr.to_string()]);
    }
}

impl Engine for Ffmpeg {
    fn name(&self) -> &'static str {
        "ffmpeg"
    }

    fn edges(&self) -> &[(Format, Format)] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> Result<Vec<PathBuf>, Error> {
        let o = step.opts();
        let out = step.output();
        let mut cmd = Command::new(&self.bin);
        cmd.args(["-hide_banner", "-nostdin", "-y", "-i"])
            .arg(step.input);
        if let Some((from_ms, to_ms)) = o.trim {
            // After -i: slower but exact to the frame.
            cmd.args(["-ss", &format!("{:.3}", from_ms as f64 / 1000.0)]);
            if to_ms > from_ms {
                cmd.args(["-to", &format!("{:.3}", to_ms as f64 / 1000.0)]);
            }
        }
        if o.strip_metadata {
            cmd.args(["-map_metadata", "-1", "-map_chapters", "-1"]);
        }

        match step.to.category() {
            Category::Subtitle => {
                let codec = self.subtitle_codec(step.to).ok_or(Error::NoRoute {
                    from: step.from,
                    to: step.to,
                })?;
                cmd.args(["-c:s", codec]);
            }
            Category::Audio => {
                let codec = self.audio_codec(step.to).ok_or(Error::NoRoute {
                    from: step.from,
                    to: step.to,
                })?;
                cmd.arg("-vn");
                audio_args(&mut cmd, codec, step.to, &o);
                if step.to == Format::Mp3 {
                    cmd.args(["-id3v2_version", "3"]);
                }
            }
            Category::Video => {
                let codec = self.video_codec(step.to).ok_or(Error::NoRoute {
                    from: step.from,
                    to: step.to,
                })?;
                // Even sizes are required by most video codecs.
                let turn = match o.rotate % 360 {
                    90 => "transpose=1,",
                    180 => "transpose=1,transpose=1,",
                    270 => "transpose=2,",
                    _ => "",
                };
                let scale = match o.max_size {
                    Some(px) => format!(
                        "scale=w='min(iw,{px})':h='min(ih,{px})':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2"
                    ),
                    None => "scale=trunc(iw/2)*2:trunc(ih/2)*2".into(),
                };
                cmd.args(["-vf", &format!("{turn}{scale}"), "-c:v", codec]);
                match codec {
                    "libx264" => {
                        cmd.args(["-crf", &o.quality.h264_crf().to_string()]);
                        cmd.args(["-preset", "medium", "-pix_fmt", "yuv420p"]);
                    }
                    "libvpx-vp9" | "libvpx" => {
                        cmd.args(["-crf", &o.quality.vp9_crf().to_string(), "-b:v", "0"]);
                        cmd.args(["-row-mt", "1", "-deadline", "good", "-cpu-used", "4"]);
                    }
                    _ => {
                        let q = match o.quality {
                            crate::options::Quality::Low => "9",
                            crate::options::Quality::Medium => "6",
                            crate::options::Quality::High => "4",
                            crate::options::Quality::Max => "2",
                        };
                        cmd.args(["-q:v", q]);
                    }
                }
                if let Some(fps) = o.fps {
                    cmd.args(["-r", &fps.to_string()]);
                }
                if o.no_audio || step.from == Format::Gif {
                    cmd.arg("-an");
                } else if let Some(ac) = self.audio_codec(step.to) {
                    audio_args(&mut cmd, ac, step.to, &o);
                }
                if matches!(step.to, Format::Mp4 | Format::Mov) {
                    cmd.args(["-movflags", "+faststart"]);
                }
            }
            _ => {
                // Video to animated GIF, with a proper palette.
                let fps = o.fps.unwrap_or(12).min(30);
                let width = o.max_size.unwrap_or(480);
                let filter = format!(
                    "fps={fps},scale='min(iw,{width})':-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse"
                );
                cmd.args(["-vf", &filter, "-loop", "0"]);
            }
        }
        cmd.arg(&out);
        run("ffmpeg", &mut cmd).map_err(|e| match e {
            Error::Tool { message, .. }
                if message.contains("does not contain any stream")
                    || message.contains("Output file #0 does not contain")
                    || message.contains("matches no streams") =>
            {
                Error::Tool {
                    tool: "ffmpeg",
                    message: "the file has no audio track".into(),
                }
            }
            other => other,
        })?;
        Ok(vec![out])
    }
}
