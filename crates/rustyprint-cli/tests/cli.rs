//! The `rustyprint` command, run as a user would run it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Exit codes, as `rustyprint --help` lists them.
const OK: i32 = 0;
const IO_ERROR: i32 = 1;
const INVALID: i32 = 2;
const LIMIT: i32 = 3;
const TIMEOUT: i32 = 4;
const WARNINGS: i32 = 5;
const INTERNAL: i32 = 70;

/// A fresh directory for one test.
fn dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rustyprint-cli-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn run(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustyprint"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

#[track_caller]
fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn pdf(bytes: &[u8]) -> lopdf::Document {
    lopdf::Document::load_mem(bytes).expect("a valid PDF")
}

/// The size of the first page in points.
fn page_size(bytes: &[u8]) -> (f32, f32) {
    let doc = pdf(bytes);
    let page = doc.get_pages()[&1];
    let media = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect::<Vec<_>>();
    (media[2] - media[0], media[3] - media[1])
}

fn image_count(bytes: &[u8]) -> usize {
    pdf(bytes)
        .objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(|s| s.as_name())
                    .is_ok_and(|name| name == b"Image")
            })
        })
        .count()
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

const A4: (f32, f32) = (595.3, 841.9);
const LETTER: (f32, f32) = (612.0, 792.0);

/// A 2×2 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a,
    0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x44, 0x0c, 0x10, 0x0a, 0x00, 0x1f, 0xee, 0x03, 0xfd, 0x8b, 0x5f, 0x14, 0xd4, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

// Input and output.

#[test]
fn renders_a_file_to_a_file() {
    let dir = dir("file");
    let input = write(&dir, "in.html", "<h1>Hello</h1>");
    let out = dir.join("out.pdf");
    let output = run(
        &[input.to_str().unwrap(), "-o", out.to_str().unwrap()],
        None,
    );
    assert_exit(&output, OK);
    assert!(output.stdout.is_empty());
    assert!(close(page_size(&fs::read(&out).unwrap()), A4));
}

#[test]
fn reads_standard_input_and_writes_standard_output() {
    let output = run(&["-", "-o", "-"], Some(b"<p>From a pipe</p>"));
    assert_exit(&output, OK);
    assert!(output.stdout.starts_with(b"%PDF-"));
    pdf(&output.stdout);
}

#[test]
fn warnings_go_to_standard_error_only() {
    let output = run(
        &["-", "-o", "-"],
        Some(b"<style>p { colr: red }</style><p>x</p>"),
    );
    assert_exit(&output, OK);
    pdf(&output.stdout);
    assert!(stderr(&output).contains("warning:"), "{}", stderr(&output));
}

#[test]
fn missing_input_is_an_io_error() {
    let dir = dir("missing");
    let output = run(&[dir.join("nope.html").to_str().unwrap(), "-o", "-"], None);
    assert_exit(&output, IO_ERROR);
    assert!(stderr(&output).contains("nope.html"), "{}", stderr(&output));
}

#[test]
fn unwritable_output_is_an_io_error() {
    let dir = dir("unwritable");
    let out = dir.join("no-such-dir").join("out.pdf");
    let output = run(&["-", "-o", out.to_str().unwrap()], Some(b"<p>x</p>"));
    assert_exit(&output, IO_ERROR);
}

// Options.

#[test]
fn options_come_from_a_json_file() {
    let dir = dir("options");
    let options = write(
        &dir,
        "options.json",
        r#"{"page_size": "Letter", "title": "From JSON"}"#,
    );
    let output = run(
        &["-", "-o", "-", "--options", options.to_str().unwrap()],
        Some(b"<p>x</p>"),
    );
    assert_exit(&output, OK);
    assert!(close(page_size(&output.stdout), LETTER));
    let doc = pdf(&output.stdout);
    let info = doc
        .trailer
        .get(b"Info")
        .and_then(|i| i.as_reference())
        .unwrap();
    let title = doc
        .get_dictionary(info)
        .unwrap()
        .get(b"Title")
        .unwrap()
        .as_str()
        .unwrap()
        .to_vec();
    assert!(String::from_utf8_lossy(&title).contains("From JSON"));
}

#[test]
fn flags_override_the_options_file() {
    let dir = dir("override");
    let options = write(
        &dir,
        "options.json",
        r#"{"page_size": "A4", "landscape": true}"#,
    );
    let output = run(
        &[
            "-",
            "-o",
            "-",
            "--options",
            options.to_str().unwrap(),
            "--page-size",
            "letter",
        ],
        Some(b"<p>x</p>"),
    );
    assert_exit(&output, OK);
    // Letter from the flag, still landscape from the file.
    assert!(close(page_size(&output.stdout), (LETTER.1, LETTER.0)));
}

#[test]
fn margins_take_the_css_shorthand() {
    let output = run(&["-", "-o", "-", "--margin", "1cm 2cm"], Some(b"<p>x</p>"));
    assert_exit(&output, OK);
}

#[test]
fn invalid_options_file() {
    let dir = dir("invalid-options");
    let options = write(&dir, "options.json", r#"{"page_size": "B9"}"#);
    let output = run(
        &["-", "-o", "-", "--options", options.to_str().unwrap()],
        Some(b"<p>x</p>"),
    );
    assert_exit(&output, INVALID);
    assert!(stderr(&output).contains("B9"), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

#[test]
fn invalid_flags() {
    for args in [
        &["--page-size", "B9"][..],
        &["--margin", "2parsecs"],
        &["--asset", "no-equals-sign"],
        &["--no-such-flag"],
    ] {
        let mut all = vec!["-", "-o", "-"];
        all.extend_from_slice(args);
        let output = run(&all, Some(b"<p>x</p>"));
        assert_exit(&output, INVALID);
    }
}

#[test]
fn missing_options_file_is_an_io_error() {
    let dir = dir("missing-options");
    let output = run(
        &[
            "-",
            "-o",
            "-",
            "--options",
            dir.join("nope.json").to_str().unwrap(),
        ],
        Some(b"<p>x</p>"),
    );
    assert_exit(&output, IO_ERROR);
}

// Assets.

#[test]
fn assets_are_given_by_name() {
    let dir = dir("asset");
    let logo = write(&dir, "logo-file.png", PNG);
    let html = b"<p><img src='img/logo.png' width=20></p>";
    let asset = format!("img/logo.png={}", logo.to_str().unwrap());
    let output = run(&["-", "-o", "-", "--asset", &asset], Some(html));
    assert_exit(&output, OK);
    assert_eq!(image_count(&output.stdout), 1, "{}", stderr(&output));
}

#[test]
fn files_beside_the_input_are_assets() {
    let dir = dir("beside");
    write(&dir, "logo.png", PNG);
    let input = write(&dir, "in.html", "<img src='logo.png' width=20>");
    let output = run(&[input.to_str().unwrap(), "-o", "-"], None);
    assert_exit(&output, OK);
    assert_eq!(image_count(&output.stdout), 1);
}

#[test]
fn standard_input_reads_no_files_by_default() {
    // Relative URLs have nothing to resolve against, so nothing is read from disk.
    let dir = dir("stdin-assets");
    write(&dir, "logo.png", PNG);
    let output = run(&["-", "-o", "-"], Some(b"<img src='logo.png' width=20>"));
    assert_exit(&output, OK);
    assert_eq!(image_count(&output.stdout), 0);
    // An asset directory gives it one.
    let output = run(
        &["-", "-o", "-", "--assets", dir.to_str().unwrap()],
        Some(b"<img src='logo.png' width=20>"),
    );
    assert_exit(&output, OK);
    assert_eq!(image_count(&output.stdout), 1);
}

#[test]
fn missing_asset_file_is_an_io_error() {
    let dir = dir("missing-asset");
    let asset = format!("logo.png={}", dir.join("nope.png").to_str().unwrap());
    let output = run(&["-", "-o", "-", "--asset", &asset], Some(b"<p>x</p>"));
    assert_exit(&output, IO_ERROR);
}

// Warnings and errors.

#[test]
fn warnings_can_fail_the_render() {
    let dir = dir("fail-on-warnings");
    let out = dir.join("out.pdf");
    let output = run(
        &["-", "-o", out.to_str().unwrap(), "--fail-on-warnings"],
        Some(b"<style>p { colr: red }</style><p>x</p>"),
    );
    assert_exit(&output, WARNINGS);
    assert!(stderr(&output).contains("colr"), "{}", stderr(&output));
    assert!(!out.exists(), "no PDF is written");
    // Without warnings the flag changes nothing.
    let output = run(&["-", "-o", "-", "--fail-on-warnings"], Some(b"<p>x</p>"));
    assert_exit(&output, OK);
}

#[test]
fn exceeded_limits() {
    let dir = dir("limit");
    let options = write(&dir, "options.json", r#"{"limits": {"max_pages": 1}}"#);
    let html = "<p>Line of text.</p>".repeat(300);
    let output = run(
        &["-", "-o", "-", "--options", options.to_str().unwrap()],
        Some(html.as_bytes()),
    );
    assert_exit(&output, LIMIT);
    assert!(
        stderr(&output).contains("page count"),
        "{}",
        stderr(&output)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn passed_deadline() {
    let dir = dir("deadline");
    let options = write(&dir, "options.json", r#"{"limits": {"timeout_ms": 1}}"#);
    let html = "<p>Line of text.</p>".repeat(2_000);
    let output = run(
        &["-", "-o", "-", "--options", options.to_str().unwrap()],
        Some(html.as_bytes()),
    );
    assert_exit(&output, TIMEOUT);
}

#[test]
fn internal_errors() {
    let output = run(&["-", "-o", "-"], Some(b"<!--rustyprint:panic-->"));
    assert_exit(&output, INTERNAL);
    assert!(
        stderr(&output).contains("internal error"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_the_exit_codes() {
    let output = run(&["--help"], None);
    assert_exit(&output, OK);
    let help = String::from_utf8_lossy(&output.stdout);
    for code in ["0", "1", "2", "3", "4", "5", "70"] {
        assert!(
            help.contains(&format!("  {code}  ")),
            "exit code {code} in:\n{help}"
        );
    }
}

#[test]
fn version_names_the_command() {
    let output = run(&["--version"], None);
    assert_exit(&output, OK);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("rustyprint {}", env!("CARGO_PKG_VERSION"))
    );
}
