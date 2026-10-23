//! Quick tools beyond conversion, all zero-parameter so they fit in the
//! selector: compress, remove metadata, merge PDFs or audio, normalise
//! volume, remove a video's sound. Results go next to the originals with a
//! descriptive name, e.g. `photo (compressed).jpg`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::engines::{ffmpeg::Ffmpeg, imagemagick::ImageMagick};
use crate::format::{Category, Format};
use crate::options::{Options, Quality};
use crate::{move_file, unique_path, Error, Outcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Compress,
    RemoveMetadata,
    MergePdf,
    MergeAudio,
    NormalizeAudio,
    RemoveAudio,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::Compress,
        Tool::RemoveMetadata,
        Tool::MergePdf,
        Tool::MergeAudio,
        Tool::NormalizeAudio,
        Tool::RemoveAudio,
    ];

    /// English label; the app translates it.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Compress => "Compress",
            Tool::RemoveMetadata => "Remove metadata",
            Tool::MergePdf => "Merge into one PDF",
            Tool::MergeAudio => "Join into one track",
            Tool::NormalizeAudio => "Even out the volume",
            Tool::RemoveAudio => "Remove the sound",
        }
    }

    /// Stable id, for remembering choices.
    pub fn key(self) -> &'static str {
        match self {
            Tool::Compress => "compress",
            Tool::RemoveMetadata => "strip",
            Tool::MergePdf => "merge-pdf",
            Tool::MergeAudio => "merge-audio",
            Tool::NormalizeAudio => "normalize",
            Tool::RemoveAudio => "remove-audio",
        }
    }

    pub fn from_key(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.key() == s)
    }

    /// True when the tool makes one file out of many.
    pub fn merges(self) -> bool {
        matches!(self, Tool::MergePdf | Tool::MergeAudio)
    }

    fn suffix(self) -> &'static str {
        match self {
            Tool::Compress => "compressed",
            Tool::RemoveMetadata => "clean",
            Tool::NormalizeAudio => "normalized",
            Tool::RemoveAudio => "no sound",
            Tool::MergePdf | Tool::MergeAudio => "merged",
        }
    }
}

pub struct Toolbox {
    imagemagick: Option<ImageMagick>,
    ffmpeg: Option<Ffmpeg>,
    qpdf: Option<PathBuf>,
    gs: Option<PathBuf>,
    exiftool: Option<PathBuf>,
}

impl Toolbox {
    pub fn probe() -> Self {
        Self {
            imagemagick: ImageMagick::probe(),
            ffmpeg: Ffmpeg::probe(),
            qpdf: which("qpdf"),
            gs: which("gs"),
            exiftool: which("exiftool"),
        }
    }

    /// Tools that make sense for `count` files of this format here.
    pub fn tools_for(&self, from: Format, count: usize) -> Vec<Tool> {
        let cat = from.category();
        let im = self.imagemagick.is_some();
        let ff = self.ffmpeg.is_some();
        let image = cat == Category::Image && from != Format::Ico;
        Tool::ALL
            .into_iter()
            .filter(|t| match t {
                Tool::Compress => {
                    (image && im)
                        || (cat == Category::Video && ff)
                        || (from == Format::Pdf && self.gs.is_some())
                }
                Tool::RemoveMetadata => {
                    (image && (im || self.exiftool.is_some()))
                        || (matches!(cat, Category::Audio | Category::Video) && ff)
                }
                Tool::MergePdf => from == Format::Pdf && count >= 2 && self.qpdf.is_some(),
                Tool::MergeAudio => cat == Category::Audio && count >= 2 && ff,
                Tool::NormalizeAudio => cat == Category::Audio && ff,
                Tool::RemoveAudio => cat == Category::Video && ff,
            })
            .collect()
    }

    /// Runs a tool. Per-file tools give one outcome per input; merging
    /// tools give a single outcome.
    pub fn run(&self, tool: Tool, files: &[PathBuf], formats: &[Format]) -> Vec<Outcome> {
        if tool.merges() {
            return vec![self.merge(tool, files, formats)];
        }
        files
            .iter()
            .zip(formats)
            .map(|(f, &fmt)| self.one(tool, f, fmt))
            .collect()
    }

    fn one(&self, tool: Tool, input: &Path, fmt: Format) -> Outcome {
        let (dir, stem) = place(input);
        let work = tempfile::Builder::new().prefix(".joey-").tempdir_in(&dir)?;
        let tmp_stem = "result";
        let tmp = work.path().join(format!("{tmp_stem}.{}", fmt.extension()));
        let cat = fmt.category();

        match (tool, cat) {
            (Tool::Compress, Category::Image) => {
                let im = self.imagemagick.as_ref().ok_or(no_tool("ImageMagick"))?;
                let opts = Options {
                    quality: Quality::Medium,
                    strip_metadata: true,
                    ..Default::default()
                };
                im_same_format(im, input, fmt, work.path(), tmp_stem, &opts)?;
                let mut cmd = Command::new(im.bin());
                if fmt == Format::Png {
                    // PNG is lossless: squeeze it harder instead.
                    cmd.arg(&tmp)
                        .args(["-define", "png:compression-level=9", "-strip"])
                        .arg(&tmp);
                    let _ = run("imagemagick", &mut cmd);
                }
            }
            (Tool::Compress, Category::Video) => {
                let ff = self.ffmpeg.as_ref().ok_or(no_tool("FFmpeg"))?;
                let opts = Options {
                    quality: Quality::Medium,
                    ..Default::default()
                };
                ff.convert(&Step {
                    input,
                    from: fmt,
                    to: fmt,
                    work_dir: work.path(),
                    stem: tmp_stem,
                    options: &opts,
                    last: true,
                })?;
            }
            (Tool::Compress, Category::Pdf) => {
                let gs = self.gs.as_ref().ok_or(no_tool("Ghostscript"))?;
                run(
                    "ghostscript",
                    Command::new(gs)
                        .args([
                            "-sDEVICE=pdfwrite",
                            "-dPDFSETTINGS=/ebook",
                            "-dNOPAUSE",
                            "-dBATCH",
                            "-dQUIET",
                        ])
                        .arg(format!("-sOutputFile={}", tmp.display()))
                        .arg(input),
                )?;
            }
            (Tool::RemoveMetadata, Category::Image) => {
                if let Some(exif) = &self.exiftool {
                    // Lossless: the picture itself isn't re-saved.
                    run(
                        "exiftool",
                        Command::new(exif)
                            .args(["-all=", "-q", "-o"])
                            .arg(&tmp)
                            .arg(input),
                    )?;
                } else {
                    let im = self.imagemagick.as_ref().ok_or(no_tool("ImageMagick"))?;
                    let opts = Options {
                        quality: Quality::Max,
                        strip_metadata: true,
                        ..Default::default()
                    };
                    im_same_format(im, input, fmt, work.path(), tmp_stem, &opts)?;
                }
            }
            (Tool::RemoveMetadata, _) => {
                let ff = self.ffmpeg.as_ref().ok_or(no_tool("FFmpeg"))?;
                run(
                    "ffmpeg",
                    Command::new(ff.bin())
                        .args(["-hide_banner", "-nostdin", "-y", "-i"])
                        .arg(input)
                        .args([
                            "-map",
                            "0",
                            "-map_metadata",
                            "-1",
                            "-map_chapters",
                            "-1",
                            "-c",
                            "copy",
                        ])
                        .arg(&tmp),
                )?;
            }
            (Tool::NormalizeAudio, _) => {
                let ff = self.ffmpeg.as_ref().ok_or(no_tool("FFmpeg"))?;
                let codec = ff.audio_codec(fmt).ok_or(no_tool("an audio encoder"))?;
                let mut cmd = Command::new(ff.bin());
                cmd.args(["-hide_banner", "-nostdin", "-y", "-i"])
                    .arg(input);
                cmd.args(["-vn", "-af", "loudnorm=I=-16:TP=-1.5:LRA=11", "-c:a", codec]);
                if !(codec.starts_with("pcm_") || codec == "flac") {
                    cmd.args(["-b:a", "192k"]);
                }
                cmd.arg(&tmp);
                run("ffmpeg", &mut cmd)?;
            }
            (Tool::RemoveAudio, _) => {
                let ff = self.ffmpeg.as_ref().ok_or(no_tool("FFmpeg"))?;
                run(
                    "ffmpeg",
                    Command::new(ff.bin())
                        .args(["-hide_banner", "-nostdin", "-y", "-i"])
                        .arg(input)
                        .args(["-map", "0", "-map", "-0:a", "-c", "copy"])
                        .arg(&tmp),
                )?;
            }
            _ => {
                return Err(Error::Tool {
                    tool: "joey",
                    message: format!("{} doesn't apply here", tool.label()),
                })
            }
        }

        if tool == Tool::Compress {
            let before = std::fs::metadata(input)?.len();
            let after = std::fs::metadata(&tmp)?.len();
            if after >= before {
                return Err(Error::Tool {
                    tool: "joey",
                    message: "already as small as it gets".into(),
                });
            }
        }
        let dest = unique_path(
            &dir,
            &format!("{stem} ({}).{}", tool.suffix(), fmt.extension()),
        );
        move_file(&tmp, &dest)?;
        Ok(vec![dest])
    }

    fn merge(&self, tool: Tool, files: &[PathBuf], formats: &[Format]) -> Outcome {
        let first = files.first().ok_or(no_tool("files"))?;
        let fmt = formats[0];
        let (dir, stem) = place(first);
        let work = tempfile::Builder::new().prefix(".joey-").tempdir_in(&dir)?;
        let tmp = work.path().join(format!("merged.{}", fmt.extension()));
        match tool {
            Tool::MergePdf => {
                let qpdf = self.qpdf.as_ref().ok_or(no_tool("qpdf"))?;
                let mut cmd = Command::new(qpdf);
                cmd.args(["--empty", "--pages"])
                    .args(files)
                    .arg("--")
                    .arg(&tmp);
                run("qpdf", &mut cmd)?;
            }
            _ => {
                let ff = self.ffmpeg.as_ref().ok_or(no_tool("FFmpeg"))?;
                let codec = ff.audio_codec(fmt).ok_or(no_tool("an audio encoder"))?;
                let mut cmd = Command::new(ff.bin());
                cmd.args(["-hide_banner", "-nostdin", "-y"]);
                for f in files {
                    cmd.arg("-i").arg(f);
                }
                let inputs: String = (0..files.len()).map(|i| format!("[{i}:a]")).collect();
                cmd.args([
                    "-filter_complex",
                    &format!("{inputs}concat=n={}:v=0:a=1[a]", files.len()),
                    "-map",
                    "[a]",
                    "-c:a",
                    codec,
                ]);
                if !(codec.starts_with("pcm_") || codec == "flac") {
                    cmd.args(["-b:a", "192k"]);
                }
                cmd.arg(&tmp);
                run("ffmpeg", &mut cmd)?;
            }
        }
        let dest = unique_path(
            &dir,
            &format!("{stem} ({}).{}", tool.suffix(), fmt.extension()),
        );
        move_file(&tmp, &dest)?;
        Ok(vec![dest])
    }
}

fn place(input: &Path) -> (PathBuf, String) {
    let dir = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    (dir, stem)
}

fn no_tool(name: &str) -> Error {
    Error::Tool {
        tool: "joey",
        message: format!("{name} isn't installed"),
    }
}

/// Re-saves an image in its own format with the given settings.
fn im_same_format(
    im: &ImageMagick,
    input: &Path,
    fmt: Format,
    work: &Path,
    stem: &str,
    opts: &Options,
) -> Result<(), Error> {
    im.convert(&Step {
        input,
        from: fmt,
        to: fmt,
        work_dir: work,
        stem,
        options: opts,
        last: true,
    })?;
    Ok(())
}
