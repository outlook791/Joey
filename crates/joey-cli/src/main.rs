//! `joey` — converts files. That's it, really.

use std::path::PathBuf;
use std::process::ExitCode;

mod mcp;

use clap::Parser;
use joey_core::{Format, Joey};

#[derive(Parser)]
#[command(
    name = "joey",
    version,
    about = "Converts files. That's it, really.",
    after_help = "Examples:\n  joey photo.heic --to jpg\n  joey *.png --to webp\n  joey scan.pdf --to png --out-dir pages/\n  joey --targets photo.heic"
)]
struct Cli {
    /// Files to convert.
    files: Vec<PathBuf>,

    /// Format to convert to (jpg, png, webp, pdf…).
    #[arg(short, long, value_name = "FORMAT")]
    to: Option<String>,

    /// Write results here instead of next to each file.
    #[arg(short, long, value_name = "DIR")]
    out_dir: Option<PathBuf>,

    /// Show what each file is and what it can become, then exit.
    #[arg(long)]
    targets: bool,

    /// Show the tools Joey found on this computer, then exit.
    #[arg(long)]
    engines: bool,

    /// Serve AI apps over the Model Context Protocol (stdin/stdout).
    #[arg(long)]
    mcp: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    joey_core::host::bridge();
    if cli.mcp {
        return match mcp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    let joey = Joey::new();

    if cli.engines {
        let names = joey.engine_names();
        if names.is_empty() {
            eprintln!(
                "No conversion tools found. Install ImageMagick, Poppler, img2pdf or librsvg."
            );
            return ExitCode::FAILURE;
        }
        for name in names {
            println!("{name}");
        }
        return ExitCode::SUCCESS;
    }

    if cli.files.is_empty() {
        eprintln!("Give me a file: joey photo.heic --to jpg   (joey --help for more)");
        return ExitCode::from(2);
    }

    if cli.targets {
        let mut ok = true;
        for file in &cli.files {
            match joey.detect(file) {
                Ok(from) => {
                    let list: Vec<_> = joey.targets(from).iter().map(|f| f.extension()).collect();
                    let list = if list.is_empty() {
                        "nothing on this computer".into()
                    } else {
                        list.join(" ")
                    };
                    println!("{}: {from} → {list}", file.display());
                }
                Err(e) => {
                    eprintln!("{e}");
                    ok = false;
                }
            }
        }
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let Some(to_name) = cli.to else {
        eprintln!(
            "Which format? Add --to, e.g. --to jpg   (joey --targets FILE shows the options)"
        );
        return ExitCode::from(2);
    };
    let Some(to) = Format::parse(&to_name) else {
        let known: Vec<_> = Format::ALL.iter().map(|f| f.extension()).collect();
        eprintln!("Unknown format \"{to_name}\". Known: {}", known.join(" "));
        return ExitCode::from(2);
    };

    let results = joey.convert_all(&cli.files, to, cli.out_dir.as_deref(), &Default::default());
    let mut failed = 0;
    for (file, result) in cli.files.iter().zip(results) {
        match result {
            Ok(written) => {
                for w in written {
                    println!("✓ {} → {}", file.display(), w.display());
                }
            }
            Err(e) => {
                failed += 1;
                let code = joey_core::diag::classify(&e, &[]);
                eprintln!(
                    "✗ {}: {} {} — {e}\n  {}",
                    file.display(),
                    code.id,
                    code.title,
                    code.hint
                );
            }
        }
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        print_report(&joey, &cli.files);
        ExitCode::FAILURE
    }
}

/// What to paste into a bug report, with personal details hidden.
fn print_report(joey: &Joey, files: &[PathBuf]) {
    use joey_core::diag;
    let names: Vec<&std::path::Path> = files.iter().map(PathBuf::as_path).collect();
    let mut text = String::from("\nIf this looks like a bug, please report it with the lines below:\nhttps://github.com/outlook791/Joey/issues/new\n\n");
    for (k, v) in diag::environment(&joey.engine_names()) {
        text.push_str(&format!("{k}: {v}\n"));
    }
    for f in diag::take_failures() {
        text.push_str(&format!(
            "\n$ {}\n[{} · {}]\n{}\n",
            f.command, f.tool, f.status, f.output
        ));
    }
    eprint!("{}", diag::scrub(&text, &names));
}
