//! `rustyprint in.html -o out.pdf`

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use rustyprint::{Assets, Remote, RenderOptions, Size};

const EXIT_CODES: &str = "\
Exit codes:
  0   The PDF was written
  1   A file could not be read or written
  2   The arguments or options are invalid
  3   The document exceeds a limit
  4   Rendering took longer than the deadline
  5   There were warnings, with --fail-on-warnings
  70  An internal error in rustyprint";

/// Render an HTML file to PDF.
#[derive(Debug, Parser)]
#[command(name = "rustyprint", version, after_help = EXIT_CODES)]
struct Args {
    /// HTML file to render, or `-` for standard input.
    input: PathBuf,
    /// PDF file to write, or `-` for standard output.
    #[arg(short, long)]
    output: PathBuf,
    /// Render options as JSON, in the format of `schema/render-options.schema.json`. The
    /// flags below override what it sets.
    #[arg(long, value_name = "FILE")]
    options: Option<PathBuf>,
    /// Extra stylesheet applied after the document's own styles.
    #[arg(long, value_name = "FILE")]
    css: Option<PathBuf>,
    /// PDF document title.
    #[arg(long)]
    title: Option<String>,
    /// Page size: A3, A4, A5, Letter or Legal. Defaults to A4, in the orientation the options
    /// give.
    #[arg(long)]
    page_size: Option<String>,
    /// Landscape orientation.
    #[arg(long)]
    landscape: bool,
    /// Page margins like the CSS `margin` shorthand: one to four lengths such as `2cm`, `15mm`,
    /// `0.5in` or `40px`, for the top, right, bottom and left. Defaults to `2cm`.
    #[arg(long)]
    margin: Option<String>,
    /// Directory that relative URLs, such as images and linked stylesheets, resolve against.
    /// Defaults to the input file's directory; with standard input, there is none.
    #[arg(long, value_name = "DIR")]
    assets: Option<PathBuf>,
    /// A file the document refers to by `NAME`, such as `--asset img/logo.png=logo.png`.
    /// Repeat for more files. These come before the asset directory.
    #[arg(long = "asset", value_name = "NAME=FILE", value_parser = parse_asset)]
    asset: Vec<(String, PathBuf)>,
    /// Host that absolute `https` URLs may be fetched from, such as `cdn.example.com`, or
    /// `*.example.com` for its subdomains. Repeat for more hosts; redirects must stay on
    /// them. Without it, nothing is fetched from the network.
    #[arg(long = "allow-host", value_name = "HOST")]
    allow_hosts: Vec<String>,
    /// Also fetch plain `http` URLs from the allowed hosts.
    #[arg(long)]
    allow_http: bool,
    /// Fail, without writing the PDF, if rendering gives any warnings.
    #[arg(long)]
    fail_on_warnings: bool,
}

fn parse_asset(text: &str) -> Result<(String, PathBuf), String> {
    match text.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => {
            Ok((name.to_owned(), PathBuf::from(path)))
        }
        _ => Err(format!("expected NAME=FILE, not `{text}`")),
    }
}

/// Why the command failed, which decides its exit code.
enum Failure {
    Io(String),
    Render(rustyprint::Error),
    Warnings(usize),
}

impl Failure {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Io(_) => 1,
            Self::Render(rustyprint::Error::InvalidOptions(_)) => 2,
            Self::Render(rustyprint::Error::LimitExceeded { .. }) => 3,
            Self::Render(rustyprint::Error::Timeout) => 4,
            Self::Warnings(_) => 5,
            Self::Render(_) => 70,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => f.write_str(message),
            Self::Render(error) => error.fmt(f),
            Self::Warnings(count) => write!(f, "{count} warning(s), with --fail-on-warnings"),
        }
    }
}

impl From<rustyprint::Error> for Failure {
    fn from(error: rustyprint::Error) -> Self {
        Self::Render(error)
    }
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("error: {failure}");
            ExitCode::from(failure.exit_code())
        }
    }
}

fn is_std(path: &Path) -> bool {
    path.as_os_str() == "-"
}

fn read(path: &Path) -> Result<Vec<u8>, Failure> {
    fs::read(path).map_err(|e| Failure::Io(format!("reading {}: {e}", path.display())))
}

fn read_text(path: &Path) -> Result<String, Failure> {
    let bytes = read(path)?;
    String::from_utf8(bytes)
        .map_err(|_| Failure::Io(format!("reading {}: not UTF-8 text", path.display())))
}

fn run(args: Args) -> Result<(), Failure> {
    let html = if is_std(&args.input) {
        let mut html = String::new();
        io::stdin()
            .read_to_string(&mut html)
            .map_err(|e| Failure::Io(format!("reading standard input: {e}")))?;
        html
    } else {
        read_text(&args.input)?
    };
    let options = options(&args)?;
    let output = rustyprint::render(&html, &options)?;
    for warning in &output.warnings {
        eprintln!("warning: {warning}");
    }
    if args.fail_on_warnings && !output.warnings.is_empty() {
        return Err(Failure::Warnings(output.warnings.len()));
    }
    if is_std(&args.output) {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(&output.pdf)
            .and_then(|()| stdout.flush())
            .map_err(|e| Failure::Io(format!("writing standard output: {e}")))
    } else {
        fs::write(&args.output, &output.pdf)
            .map_err(|e| Failure::Io(format!("writing {}: {e}", args.output.display())))
    }
}

/// The options file, if any, with the flags applied over it.
fn options(args: &Args) -> Result<RenderOptions, Failure> {
    let mut options = match &args.options {
        Some(path) => RenderOptions::from_json(&read_text(path)?)?,
        None => RenderOptions::default(),
    };
    if let Some(path) = &args.css {
        options.stylesheet = Some(read_text(path)?);
    }
    if let Some(title) = &args.title {
        options.title = Some(title.clone());
    }
    let landscape = args.landscape || options.page_size.width > options.page_size.height;
    if let Some(name) = &args.page_size {
        options.page_size = rustyprint::page_size(name)?;
    }
    if landscape {
        let Size { width, height } = options.page_size;
        options.page_size = Size::new(width.max(height), width.min(height));
    }
    if let Some(margin) = &args.margin {
        options.margin = rustyprint::parse_margin(margin)?;
    }
    if !args.allow_hosts.is_empty() {
        options.assets.remote = Some(Remote::allowing(args.allow_hosts.iter().cloned()));
    }
    if args.allow_http
        && let Some(remote) = &mut options.assets.remote
    {
        remote.allow_http = true;
    }
    let directory = args.assets.clone().or_else(|| {
        (!is_std(&args.input)).then(|| {
            args.input
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        })
    });
    let mut files = std::collections::BTreeMap::new();
    for (name, path) in &args.asset {
        files.insert(name.clone(), read(path)?);
    }
    options.assets = Assets {
        files,
        directory,
        ..options.assets
    };
    Ok(options)
}
