//! Quick tools beyond conversion. Most need nothing but a click; a few ask
//! one thing (a time range, pages, a shape, a password). Results go next to
//! the originals with a descriptive name, e.g. `photo (compressed).jpg`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{run, which, Engine, Step};
use crate::engines::{archive::Archive, ffmpeg::Ffmpeg, imagemagick::ImageMagick};
use crate::format::{Category, Format};
use crate::options::{Options, Quality};
use crate::{move_file, stem_of, unique_path, Error, Outcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Compress,
    RemoveMetadata,
    RotateLeft,
    RotateRight,
    Grayscale,
    Crop,
    Trim,
    NormalizeAudio,
    RemoveAudio,
    SplitPdf,
    ExtractPages,
    ProtectPdf,
    UnlockPdf,
    ExtractArchive,
    MergePdf,
    MergeAudio,
    MergeVideo,
    MakeZip,
}

/// What a tool needs to ask before running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Nothing,
    TimeRange,
    Pages,
    Shape,
    Password,
}

/// Answers to `Ask`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolParams {
    pub trim: Option<(u64, u64)>,
    pub pages: Option<String>,
    pub shape: Option<(u32, u32)>,
    pub password: Option<String>,
}

impl Tool {
    pub const ALL: [Tool; 18] = [
        Tool::Compress,
        Tool::RemoveMetadata,
        Tool::RotateLeft,
        Tool::RotateRight,
        Tool::Grayscale,
        Tool::Crop,
        Tool::Trim,
        Tool::NormalizeAudio,
        Tool::RemoveAudio,
        Tool::SplitPdf,
        Tool::ExtractPages,
        Tool::ProtectPdf,
        Tool::UnlockPdf,
        Tool::ExtractArchive,
        Tool::MergePdf,
        Tool::MergeAudio,
        Tool::MergeVideo,
        Tool::MakeZip,
    ];

    /// English label; the app translates it.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Compress => "Compress",
            Tool::RemoveMetadata => "Remove metadata",
            Tool::RotateLeft => "Rotate left",
            Tool::RotateRight => "Rotate right",
            Tool::Grayscale => "Black and white",
            Tool::Crop => "Crop to a shape…",
            Tool::Trim => "Trim…",
            Tool::NormalizeAudio => "Even out the volume",
            Tool::RemoveAudio => "Remove the sound",
            Tool::SplitPdf => "Split into pages",
            Tool::ExtractPages => "Keep only some pages…",
            Tool::ProtectPdf => "Protect with a password…",
            Tool::UnlockPdf => "Remove the password…",
            Tool::ExtractArchive => "Extract here",
            Tool::MergePdf => "Merge into one PDF",
            Tool::MergeAudio => "Join into one track",
            Tool::MergeVideo => "Join into one video",
            Tool::MakeZip => "Put in a ZIP",
        }
    }

    /// Stable id, for remembering choices.
    pub fn key(self) -> &'static str {
        match self {
            Tool::Compress => "compress",
            Tool::RemoveMetadata => "strip",
            Tool::RotateLeft => "rotate-left",
            Tool::RotateRight => "rotate-right",
            Tool::Grayscale => "grayscale",
            Tool::Crop => "crop",
            Tool::Trim => "trim",
            Tool::NormalizeAudio => "normalize",
            Tool::RemoveAudio => "remove-audio",
            Tool::SplitPdf => "split-pdf",
            Tool::ExtractPages => "extract-pages",
            Tool::ProtectPdf => "protect-pdf",
            Tool::UnlockPdf => "unlock-pdf",
            Tool::ExtractArchive => "extract",
            Tool::MergePdf => "merge-pdf",
            Tool::MergeAudio => "merge-audio",
            Tool::MergeVideo => "merge-video",
            Tool::MakeZip => "zip",
        }
    }

    pub fn from_key(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.key() == s)
    }

    /// True when the tool makes one file out of many.
    pub fn merges(self) -> bool {
        matches!(
            self,
            Tool::MergePdf | Tool::MergeAudio | Tool::MergeVideo | Tool::MakeZip
        )
    }

    pub fn ask(self) -> Ask {
        match self {
            Tool::Trim => Ask::TimeRange,
            Tool::ExtractPages => Ask::Pages,
            Tool::Crop => Ask::Shape,
            Tool::ProtectPdf | Tool::UnlockPdf => Ask::Password,
            _ => Ask::Nothing,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Tool::Compress => "compressed",
            Tool::RemoveMetadata => "clean",
            Tool::RotateLeft | Tool::RotateRight => "rotated",
            Tool::Grayscale => "black and white",
            Tool::Crop => "cropped",
            Tool::Trim => "trimmed",
            Tool::NormalizeAudio => "normalized",
            Tool::RemoveAudio => "no sound",
            Tool::ExtractPages => "pages",
            Tool::ProtectPdf => "protected",
            Tool::UnlockPdf => "unlocked",
            Tool::MergePdf | Tool::MergeAudio | Tool::MergeVideo => "merged",
            Tool::SplitPdf | Tool::ExtractArchive | Tool::MakeZip => "",
        }
    }
}

pub struct Toolbox {
    imagemagick: Option<ImageMagick>,
    ffmpeg: Option<Ffmpeg>,
    archive: Option<Archive>,
    qpdf: Option<PathBuf>,
    gs: Option<PathBuf>,
    exiftool: Option<PathBuf>,
}

impl Toolbox {
    pub fn probe() -> Self {
        Self {
            imagemagick: ImageMagick::probe(),
            ffmpeg: Ffmpeg::probe(),
            archive: Archive::probe(),
            qpdf: which("qpdf"),
            gs: which("gs"),
            exiftool: which("exiftool"),
        }
    }

    /// True if any file (even unknown ones, even folders) can be zipped.
    pub fn can_zip(&self) -> bool {
        self.archive.is_some()
    }

    /// Tools that make sense for `count` files of this format here.
    pub fn tools_for(&self, from: Format, count: usize) -> Vec<Tool> {
        let cat = from.category();
        let im = self.imagemagick.is_some();
        let ff = self.ffmpeg.is_some();
        let qpdf = self.qpdf.is_some();
        let image = cat == Category::Image && from != Format::Ico;
        let video = cat == Category::Video;
        let audio = cat == Category::Audio;
        let pdf = from == Format::Pdf;
        Tool::ALL
            .into_iter()
            .filter(|t| match t {
                Tool::Compress => (image && im) || (video && ff) || (pdf && self.gs.is_some()),
                Tool::RemoveMetadata => {
                    (image && (im || self.exiftool.is_some())) || ((audio || video) && ff)
                }
                Tool::RotateLeft | Tool::RotateRight => {
                    (image && im) || (video && ff) || (pdf && qpdf)
                }
                Tool::Grayscale | Tool::Crop => image && im,
                Tool::Trim => (audio || video) && ff,
                Tool::NormalizeAudio => audio && ff,
                Tool::RemoveAudio => video && ff,
                Tool::SplitPdf | Tool::ExtractPages | Tool::ProtectPdf | Tool::UnlockPdf => {
                    pdf && qpdf
                }
                Tool::ExtractArchive => cat == Category::Archive && self.archive.is_some(),
                Tool::MergePdf => pdf && count >= 2 && qpdf,
                Tool::MergeAudio => audio && count >= 2 && ff,
                Tool::MergeVideo => video && count >= 2 && ff,
                Tool::MakeZip => self.archive.is_some() && cat != Category::Archive,
            })
            .collect()
    }

    /// Runs a tool. Per-file tools give one outcome per input; merging
    /// tools give a single outcome.
    pub fn run(
        &self,
        tool: Tool,
        files: &[PathBuf],
        formats: &[Format],
        params: &ToolParams,
    ) -> Vec<Outcome> {
        if tool.merges() {
            return vec![self.merge(tool, files, formats)];
        }
        files
            .iter()
            .zip(formats)
            .map(|(f, &fmt)| self.one(tool, f, fmt, params))
            .collect()
    }

    fn ffmpeg(&self) -> Result<&Ffmpeg, Error> {
        self.ffmpeg.as_ref().ok_or_else(|| no_tool("FFmpeg"))
    }

    fn im(&self) -> Result<&ImageMagick, Error> {
        self.imagemagick
            .as_ref()
            .ok_or_else(|| no_tool("ImageMagick"))
    }

    fn qpdf(&self) -> Result<&PathBuf, Error> {
        self.qpdf.as_ref().ok_or_else(|| no_tool("qpdf"))
    }

    /// Re-encodes a file in its own format through an engine, with settings.
    fn same_format(
        &self,
        engine: &dyn Engine,
        input: &Path,
        fmt: Format,
        work: &Path,
        opts: &Options,
    ) -> Result<(), Error> {
        engine.convert(&Step {
            input,
            from: fmt,
            to: fmt,
            work_dir: work,
            stem: "result",
            options: opts,
            last: true,
        })?;
        Ok(())
    }

    fn one(&self, tool: Tool, input: &Path, fmt: Format, params: &ToolParams) -> Outcome {
        let (dir, stem) = place(input);
        let work = tempfile::Builder::new().prefix(".joey-").tempdir_in(&dir)?;
        let tmp = work.path().join(format!("result.{}", fmt.extension()));
        let cat = fmt.category();
        let turn = match tool {
            Tool::RotateLeft => 270,
            Tool::RotateRight => 90,
            _ => 0,
        };

        match (tool, cat) {
            (Tool::Compress, Category::Image) => {
                let im = self.im()?;
                let opts = Options {
                    quality: Quality::Medium,
                    strip_metadata: true,
                    ..Default::default()
                };
                self.same_format(im, input, fmt, work.path(), &opts)?;
                if fmt == Format::Png {
                    // PNG is lossless: squeeze it harder instead.
                    let mut cmd = Command::new(im.bin());
                    cmd.arg(&tmp)
                        .args(["-define", "png:compression-level=9", "-strip"])
                        .arg(&tmp);
                    let _ = run("imagemagick", &mut cmd);
                }
            }
            (Tool::Compress, Category::Video) => {
                let opts = Options {
                    quality: Quality::Medium,
                    ..Default::default()
                };
                self.same_format(self.ffmpeg()?, input, fmt, work.path(), &opts)?;
            }
            (Tool::Compress, Category::Pdf) => {
                let gs = self.gs.as_ref().ok_or_else(|| no_tool("Ghostscript"))?;
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
                    let opts = Options {
                        quality: Quality::Max,
                        strip_metadata: true,
                        ..Default::default()
                    };
                    self.same_format(self.im()?, input, fmt, work.path(), &opts)?;
                }
            }
            (Tool::RemoveMetadata, _) => {
                run(
                    "ffmpeg",
                    Command::new(self.ffmpeg()?.bin())
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
            (Tool::RotateLeft | Tool::RotateRight, Category::Image) => {
                let opts = Options {
                    quality: Quality::Max,
                    rotate: turn,
                    ..Default::default()
                };
                self.same_format(self.im()?, input, fmt, work.path(), &opts)?;
            }
            (Tool::RotateLeft | Tool::RotateRight, Category::Video) => {
                let opts = Options {
                    rotate: turn,
                    ..Default::default()
                };
                self.same_format(self.ffmpeg()?, input, fmt, work.path(), &opts)?;
            }
            (Tool::RotateLeft | Tool::RotateRight, Category::Pdf) => {
                let angle = if turn == 90 { "+90" } else { "-90" };
                run(
                    "qpdf",
                    Command::new(self.qpdf()?)
                        .arg(format!("--rotate={angle}"))
                        .arg(input)
                        .arg(&tmp),
                )?;
            }
            (Tool::Grayscale, _) => {
                let opts = Options {
                    quality: Quality::Max,
                    grayscale: true,
                    ..Default::default()
                };
                self.same_format(self.im()?, input, fmt, work.path(), &opts)?;
            }
            (Tool::Crop, _) => {
                let (rw, rh) = params.shape.unwrap_or((1, 1));
                let im = self.im()?;
                let (w, h) = image_size(im, input)?;
                // Largest centred area with that shape.
                let (cw, ch) = if w as u64 * rh as u64 > h as u64 * rw as u64 {
                    ((h as u64 * rw as u64 / rh as u64) as u32, h)
                } else {
                    (w, (w as u64 * rh as u64 / rw as u64) as u32)
                };
                let opts = Options {
                    quality: Quality::Max,
                    crop: Some((cw.max(1), ch.max(1))),
                    ..Default::default()
                };
                self.same_format(im, input, fmt, work.path(), &opts)?;
            }
            (Tool::Trim, _) => {
                let trim = params.trim.ok_or_else(|| ask_err("a start and an end"))?;
                let opts = Options {
                    trim: Some(trim),
                    quality: Quality::High,
                    ..Default::default()
                };
                self.same_format(self.ffmpeg()?, input, fmt, work.path(), &opts)?;
            }
            (Tool::NormalizeAudio, _) => {
                let ff = self.ffmpeg()?;
                let codec = ff
                    .audio_codec(fmt)
                    .ok_or_else(|| no_tool("an audio encoder"))?;
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
                run(
                    "ffmpeg",
                    Command::new(self.ffmpeg()?.bin())
                        .args(["-hide_banner", "-nostdin", "-y", "-i"])
                        .arg(input)
                        .args(["-map", "0", "-map", "-0:a", "-c", "copy"])
                        .arg(&tmp),
                )?;
            }
            (Tool::SplitPdf, _) => {
                let parts = work.path().join("parts");
                std::fs::create_dir_all(&parts)?;
                run(
                    "qpdf",
                    Command::new(self.qpdf()?)
                        .arg("--split-pages")
                        .arg(input)
                        .arg(parts.join("page.pdf")),
                )?;
                let mut pages: Vec<PathBuf> = std::fs::read_dir(&parts)?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .collect();
                pages.sort();
                let mut written = Vec::new();
                for (i, page) in pages.iter().enumerate() {
                    let dest = unique_path(&dir, &format!("{stem}-{}.pdf", i + 1));
                    move_file(page, &dest)?;
                    written.push(dest);
                }
                return Ok(written);
            }
            (Tool::ExtractPages, _) => {
                let pages = params
                    .pages
                    .as_deref()
                    .map(|p| p.replace(' ', ""))
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| ask_err("the pages"))?;
                run(
                    "qpdf",
                    Command::new(self.qpdf()?)
                        .arg(input)
                        .args(["--pages", ".", &pages, "--"])
                        .arg(&tmp),
                )?;
            }
            (Tool::ProtectPdf, _) => {
                let pw = params
                    .password
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| ask_err("a password"))?;
                run(
                    "qpdf",
                    Command::new(self.qpdf()?)
                        .args(["--encrypt", pw, pw, "256", "--"])
                        .arg(input)
                        .arg(&tmp),
                )?;
            }
            (Tool::UnlockPdf, _) => {
                let pw = params.password.as_deref().unwrap_or("");
                run(
                    "qpdf",
                    Command::new(self.qpdf()?)
                        .arg(format!("--password={pw}"))
                        .arg("--decrypt")
                        .arg(input)
                        .arg(&tmp),
                )
                .map_err(|_| Error::Tool {
                    tool: "joey",
                    message: "wrong password".into(),
                })?;
            }
            (Tool::ExtractArchive, _) => {
                let archive = self.archive.as_ref().ok_or_else(|| no_tool("bsdtar"))?;
                let dest = unique_path(&dir, &stem);
                let inner = work.path().join("out");
                archive.extract(input, &inner)?;
                std::fs::rename(&inner, &dest).or_else(|_| copy_dir(&inner, &dest))?;
                return Ok(vec![dest]);
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
        let first = files.first().ok_or_else(|| ask_err("files"))?;
        if tool == Tool::MakeZip {
            return self.zip(files);
        }
        let fmt = formats[0];
        let (dir, stem) = place(first);
        let work = tempfile::Builder::new().prefix(".joey-").tempdir_in(&dir)?;
        let tmp = work.path().join(format!("merged.{}", fmt.extension()));
        match tool {
            Tool::MergePdf => {
                let mut cmd = Command::new(self.qpdf()?);
                cmd.args(["--empty", "--pages"])
                    .args(files)
                    .arg("--")
                    .arg(&tmp);
                run("qpdf", &mut cmd)?;
            }
            Tool::MergeVideo => {
                let ff = self.ffmpeg()?;
                // Same camera, same settings: join without re-encoding.
                let list = work.path().join("list.txt");
                let lines: String = files
                    .iter()
                    .map(|f| {
                        let abs = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
                        format!(
                            "file '{}'\n",
                            abs.display().to_string().replace('\'', "'\\''")
                        )
                    })
                    .collect();
                std::fs::write(&list, lines)?;
                let fast = run(
                    "ffmpeg",
                    Command::new(ff.bin())
                        .args([
                            "-hide_banner",
                            "-nostdin",
                            "-y",
                            "-f",
                            "concat",
                            "-safe",
                            "0",
                            "-i",
                        ])
                        .arg(&list)
                        .args(["-c", "copy"])
                        .arg(&tmp),
                );
                if fast.is_err() {
                    // Different videos: re-encode them into one.
                    let mut cmd = Command::new(ff.bin());
                    cmd.args(["-hide_banner", "-nostdin", "-y"]);
                    for f in files {
                        cmd.arg("-i").arg(f);
                    }
                    let n = files.len();
                    let parts: String = (0..n)
                        .map(|i| format!("[{i}:v]scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,setsar=1[v{i}];"))
                        .collect();
                    let joined: String = (0..n).map(|i| format!("[v{i}][{i}:a]")).collect();
                    cmd.args([
                        "-filter_complex",
                        &format!("{parts}{joined}concat=n={n}:v=1:a=1[v][a]"),
                        "-map",
                        "[v]",
                        "-map",
                        "[a]",
                    ]);
                    cmd.arg(&tmp);
                    run("ffmpeg", &mut cmd)?;
                }
            }
            _ => {
                let ff = self.ffmpeg()?;
                let codec = ff
                    .audio_codec(fmt)
                    .ok_or_else(|| no_tool("an audio encoder"))?;
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

    /// Puts files or folders (of any kind) into one ZIP next to the first.
    pub fn zip(&self, paths: &[PathBuf]) -> Outcome {
        let archive = self.archive.as_ref().ok_or_else(|| no_tool("bsdtar"))?;
        let first = paths.first().ok_or_else(|| ask_err("files"))?;
        let (dir, stem) = place(first);
        let work = tempfile::Builder::new().prefix(".joey-").tempdir_in(&dir)?;
        let tmp = work.path().join("archive.zip");
        // Entries are named relative to their own folder.
        let mut cmd_groups: Vec<(PathBuf, Vec<String>)> = Vec::new();
        for p in paths {
            let parent = p
                .parent()
                .filter(|x| !x.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match cmd_groups.iter_mut().find(|(d, _)| *d == parent) {
                Some((_, names)) => names.push(name),
                None => cmd_groups.push((parent, vec![name])),
            }
        }
        if cmd_groups.len() == 1 {
            let (base, names) = &cmd_groups[0];
            archive.create(&tmp, Format::Zip, base, names)?;
        } else {
            // Files from different folders: gather links first.
            let gather = work.path().join("gather");
            std::fs::create_dir_all(&gather)?;
            let mut names = Vec::new();
            for p in paths {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let link = unique_path(&gather, &name);
                #[cfg(unix)]
                std::os::unix::fs::symlink(std::fs::canonicalize(p)?, &link)?;
                names.push(link.file_name().unwrap().to_string_lossy().into_owned());
            }
            let mut cmd = Command::new("bsdtar");
            cmd.args(["-c", "-L", "--format", "zip", "-f"])
                .arg(&tmp)
                .arg("-C")
                .arg(&gather)
                .arg("--")
                .args(&names);
            run("bsdtar", &mut cmd)?;
        }
        let dest = unique_path(&dir, &format!("{stem}.zip"));
        move_file(&tmp, &dest)?;
        Ok(vec![dest])
    }
}

/// Width and height of an image.
fn image_size(im: &ImageMagick, input: &Path) -> Result<(u32, u32), Error> {
    let mut src = input.as_os_str().to_owned();
    src.push("[0]");
    let out = run(
        "imagemagick",
        Command::new(im.bin())
            .arg(&src)
            .args(["-auto-orient", "-format", "%w %h", "info:"]),
    )?;
    let mut it = out.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
    match (it.next(), it.next()) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(Error::Tool {
            tool: "imagemagick",
            message: "couldn't read the image size".into(),
        }),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

fn place(input: &Path) -> (PathBuf, String) {
    let dir = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    (dir, stem_of(input))
}

fn no_tool(name: &str) -> Error {
    Error::Tool {
        tool: "joey",
        message: format!("{name} isn't installed"),
    }
}

fn ask_err(what: &str) -> Error {
    Error::Tool {
        tool: "joey",
        message: format!("missing {what}"),
    }
}
