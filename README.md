# rustyprint

rustyprint turns HTML and CSS into PDF. It is written in Rust with a layout engine of its own, and needs no browser, no external runtime and no network access.

- **HTML:** the elements of the HTML5 specification, rendered as static, paginated PDF.
- **CSS:** the properties, selectors, at-rules and values of the MDN CSS reference that apply to HTML, including flexbox, grid, multi-column, tables, floats, positioning, paged media (`@page`, margin boxes, running headers, page counters), web fonts, variable and color fonts, transforms, filters and masks.
- **Coverage:** of the 1,080 elements and CSS features tracked, 1,049 are complete and the other 31 work in part, with their gaps listed.
- **Deterministic:** the same input always gives a byte-identical PDF.
- **Built for untrusted input:** nothing is fetched from the network unless you allow the host, and every render runs within limits on size, page count and time.

The trackers ([html-elements.md](html-elements.md) and the `css-*.md` files) list the status of every element and CSS feature, with notes on what is missing, and [progress.html](progress.html) summarises them. [plan.md](plan.md) is the plan and records which milestone the work is on.

| Use it as | Get it |
| --- | --- |
| [A command-line tool](#use-the-command-line) | An archive from the [Releases page](https://github.com/vryan95/rustyprint/releases), or `cargo install` |
| [An HTTP service](#run-the-http-service) | `docker run -p 8080:8080 ghcr.io/vryan95/rustyprint-server` |
| [A Ruby gem](#use-it-from-ruby) | `gem install rustyprint` |
| [A NuGet package](#use-it-from-net) | `dotnet add package Rustyprint` |
| [A Rust library](#use-it-from-rust) | A Git dependency |
| [A C library](#use-it-from-c) | An archive from the [Releases page](https://github.com/vryan95/rustyprint/releases) |

## Install

Each release on the [Releases page](https://github.com/vryan95/rustyprint/releases) has the program ready to run, for Linux, macOS and Windows on x64 and arm64:

| Platform | Archive |
| --- | --- |
| Linux (glibc 2.17 or newer) | `rustyprint-<version>-x86_64-unknown-linux-gnu.tar.gz`, `…-aarch64-unknown-linux-gnu.tar.gz` |
| Linux, static (Alpine and other musl distributions) | `rustyprint-<version>-x86_64-unknown-linux-musl.tar.gz`, `…-aarch64-unknown-linux-musl.tar.gz` |
| macOS | `rustyprint-<version>-aarch64-apple-darwin.tar.gz` (Apple silicon), `…-x86_64-apple-darwin.tar.gz` (Intel) |
| Windows | `rustyprint-<version>-x86_64-pc-windows-msvc.zip`, `…-aarch64-pc-windows-msvc.zip` |

Each archive holds the program, a single file with the fonts it needs built in, with the README and the licenses. `SHA256SUMS` lists the checksums of every archive.

Or build it from source, with Rust 1.98 or newer ([rustup.rs](https://rustup.rs)):

```sh
git clone https://github.com/vryan95/rustyprint.git
cd rustyprint
cargo build --release
```

The program is `target/release/rustyprint` (`rustyprint.exe` on Windows), a single file with the fonts it needs built in. Copy it anywhere on your `PATH`, or install it into Cargo's bin directory with:

```sh
cargo install --path crates/rustyprint-cli
```

## Use the command line

Render a file:

```sh
rustyprint invoice.html -o invoice.pdf
```

Images, stylesheets and fonts the HTML refers to by relative or root-relative URL are read from the HTML file's directory: `img/logo.png` and `/img/logo.png` are both `img/logo.png` there.

Choose the page and margins, which follow the CSS `margin` shorthand:

```sh
rustyprint report.html -o report.pdf --page-size Letter --landscape --margin "15mm 20mm"
```

Read the HTML from standard input and write the PDF to standard output with `-`:

```sh
cat page.html | rustyprint - -o - > page.pdf
```

HTML from standard input has no directory, so it reads no local files unless you give it some:

```sh
generate-invoice | rustyprint - -o invoice.pdf --assets ./templates --asset logo.png=./brand/logo-2026.png
```

`--asset NAME=FILE` serves a file under the name the HTML uses, such as `<img src="logo.png">`, and can be repeated.

### Options

| Option | What it does |
| --- | --- |
| `-o, --output FILE` | The PDF to write, or `-` for standard output. Required. |
| `--page-size SIZE` | `A3`, `A4`, `A5`, `Letter` or `Legal`. Default `A4`. A document's own `@page { size }` rule takes precedence. |
| `--landscape` | Turns the page so it is wider than it is tall. |
| `--margin MARGIN` | One to four lengths, as in CSS: `2cm`, `15mm 20mm`, `0.5in 1in 0.5in 1in`. Units: `px`, `mm`, `cm`, `in`, `pt`. Default `2cm`. |
| `--title TITLE` | The PDF's title. Defaults to the document's `<title>`. |
| `--css FILE` | A stylesheet applied after the document's own. |
| `--assets DIR` | The directory relative and root-relative URLs resolve against. Defaults to the input file's directory. |
| `--asset NAME=FILE` | A file the document refers to as `NAME`. Repeatable. |
| `--allow-host HOST` | A host that absolute `https` URLs may be fetched from, such as `cdn.example.com` or `*.example.com`. Repeatable. Redirects must stay on allowed hosts. |
| `--allow-http` | Also fetch plain `http` URLs from the allowed hosts. |
| `--options FILE` | Options as JSON (below). The other flags override what it sets. |
| `--fail-on-warnings` | Fail, without writing the PDF, if there are any warnings. |

Run `rustyprint --help` for the full list.

### Options as JSON

`--options` takes the same JSON options the HTTP service and the Ruby and .NET packages take. [schema/render-options.schema.json](schema/render-options.schema.json) describes every field. All fields are optional:

```json
{
  "page_size": "A4",
  "landscape": false,
  "margin": "15mm",
  "title": "Invoice 42",
  "stylesheet": "body { font-family: sans-serif }",
  "remote": { "allowed_hosts": ["cdn.example.com"] },
  "limits": { "max_pages": 50, "timeout_ms": 10000 }
}
```

`page_size` also takes an exact size, such as `{ "width": "100mm", "height": "150mm" }`.

### Remote images and stylesheets

By default rustyprint never touches the network: absolute `http` and `https` URLs are skipped with a warning. Allow each host to fetch from, including the hosts that redirects lead to:

```sh
rustyprint invoice.html -o invoice.pdf \
  --allow-host app.example.com \
  --allow-host storage.example.net
```

Each fetch is limited to 10 seconds and 10 MB, with at most 5 redirects and 100 fetches per document. The `remote` JSON option changes these limits.

### Limits

Every render runs within limits, so a hostile or broken document cannot run forever or exhaust memory. Change them with the `limits` JSON option.

| Limit | Default | JSON field |
| --- | --- | --- |
| HTML size | 20 MB | `max_input_bytes` |
| DOM nodes | 500,000 | `max_nodes` |
| Element nesting depth | 512 | `max_depth` |
| Pages | 2,000 | `max_pages` |
| Image size (larger images are skipped with a warning) | 100 megapixels | `max_image_pixels` |
| Time | 60 seconds | `timeout_ms` (`null` for no limit) |

### Warnings and exit codes

Problems that do not stop the render, such as unknown CSS or an image that cannot be loaded, are printed to standard error as warnings, and the PDF is still written. With `--fail-on-warnings` they fail the run instead.

| Exit code | Meaning |
| --- | --- |
| 0 | The PDF was written |
| 1 | A file could not be read or written |
| 2 | The arguments or options are invalid |
| 3 | The document exceeds a limit |
| 4 | Rendering took longer than the deadline |
| 5 | There were warnings, with `--fail-on-warnings` |
| 70 | An internal error in rustyprint |

## Run the HTTP service

`rustyprint-server` serves the same renderer over HTTP. Each release publishes it as a container image for `linux/amd64` and `linux/arm64`:

```sh
docker run -p 8080:8080 ghcr.io/vryan95/rustyprint-server:latest
```

Send the HTML as JSON, with the same options as `--options` takes, or as `multipart/form-data` with the files it refers to:

```sh
curl -H 'Content-Type: application/json' \
  -d '{"html": "<h1>Hello</h1>", "options": {"page_size": "A4", "margin": "15mm"}}' \
  -o hello.pdf http://localhost:8080/v1/pdf

curl -F index.html=@invoice.html -F img/logo.png=@brand/logo.png -o invoice.pdf http://localhost:8080/v1/pdf
```

The response is the PDF, with the warnings in its headers. [crates/rustyprint-server/README.md](crates/rustyprint-server/README.md) is the API reference: both request formats, the response headers, the errors, the metrics, and the environment variables that configure the server and set what requests may do.

## Use it from Ruby

The `rustyprint` gem, for Ruby 3.2 and newer, with the native library built into a gem for each platform. In Rails it adds `render pdf:` to controllers.

```ruby
output = Rustyprint.render(html, page_size: "A4", margin: "15mm",
                                 assets: { "logo.png" => Pathname("app/assets/images/logo.png") })
File.binwrite("invoice.pdf", output.pdf)
```

Install it with `gem install rustyprint`, or add `gem "rustyprint"` to your Gemfile. [bindings/ruby/README.md](bindings/ruby/README.md) describes the options, errors and Rails support, and [demos/rails](demos/rails) is a Rails app that uses it.

## Use it from .NET

The `Rustyprint` NuGet package, for .NET 8 and newer, with the native library for every platform inside it.

```csharp
var options = new RenderOptions { PageSize = PageSize.A4, Margin = "15mm" };
options.Assets.AddFile("logo.png", "wwwroot/logo.png");
RenderOutput output = await PdfRenderer.RenderAsync(html, options, cancellationToken);
File.WriteAllBytes("invoice.pdf", output.Pdf);
```

Install it with `dotnet add package Rustyprint`. [bindings/dotnet/README.md](bindings/dotnet/README.md) describes the options, errors and use in ASP.NET Core, and [demos/dotnet](demos/dotnet) is an ASP.NET Core app that uses it.

## Use it from Rust

The `rustyprint` crate is the engine itself. It is not published on crates.io, so depend on it through Git, at a release's tag:

```toml
[dependencies]
rustyprint = { git = "https://github.com/vryan95/rustyprint", tag = "v1.0.0", features = ["serde"] }
```

The `serde` feature adds `RenderOptions::from_json`, for options as JSON; without it, build `RenderOptions` in code.

```rust
use rustyprint::{RenderOptions, render};

let options = RenderOptions::from_json(r#"{"page_size": "Letter", "margin": "1in"}"#)?; // `serde` feature
let output = render("<h1>Hello</h1>", &options)?;
std::fs::write("hello.pdf", &output.pdf)?;
for warning in &output.warnings {
    eprintln!("warning: {warning}");
}
```

`render` is safe to call from several threads at once. Each render runs on a thread of its own, so deeply nested documents cannot overflow the caller's stack, and a panic inside it comes back as an error.

## Use it from C

`rustyprint-ffi` is rustyprint as a shared library with a C ABI, for C and any language that can call C; the Ruby gem and the NuGet package are built on it. Each release has it for every platform as `rustyprint-native-<version>-<target>.tar.gz` (`.zip` for Windows), with the header, or build it with `cargo build --release -p rustyprint-ffi`.

```c
rp_request *request = rp_request_new(options_json, strlen(options_json));
rp_request_add_asset(request, "img/logo.png", logo, logo_len);
rp_result *result = NULL;
if (rp_render(request, html, strlen(html), &result) == RP_STATUS_OK) {
    size_t len;
    const uint8_t *pdf = rp_result_pdf(result, &len);
    /* ... */
}
rp_result_free(result);
rp_request_free(request);
```

The options are the same JSON as `--options` takes. [crates/rustyprint-ffi/README.md](crates/rustyprint-ffi/README.md) describes the whole API and how to link it.

## Performance

rustyprint 1.0.3 compared with [WeasyPrint](https://weasyprint.org) 70.0, a widely used HTML-to-PDF converter, rendering the same documents with the same Noto fonts. Each run starts a new process, as a command-line conversion does, so start-up time counts. Times are medians; memory is the most the process was seen to use, sampled every 5 ms.

| Document | Pages | rustyprint | WeasyPrint | |
| --- | --- | --- | --- | --- |
| Invoice ([demos/samples/invoice](demos/samples/invoice/index.html)) | 3 | 43 ms, 22 MB | 1.44 s, 72 MB | 33× faster |
| One-page report ([bench/report-one.html](bench/report-one.html)) | 1 | 22 ms, 18 MB | 1.08 s, 72 MB | 48× faster |
| Long report ([bench/report-long.html](bench/report-long.html)) | 498 / 500 | 2.8 s, 454 MB | 43.3 s, 653 MB | 15× faster |

Used as a library, without starting a process, rendering takes:

| Document | Pages | Time |
| --- | --- | --- |
| Letter | 1 | 4 ms |
| Report | 21 | 45 ms |
| Invoice with a 200-row table | 6 | 55 ms |

Measured on an AMD Ryzen 7 5700U laptop in October 2026: the comparison on Linux in Docker on WSL 2, the library times on Linux on WSL 2. To run the comparison yourself, with WeasyPrint and the Python package `psutil` installed:

```sh
python bench/run.py target/release/rustyprint weasyprint bench-out
```

and the library benchmarks with `cargo bench -p rustyprint --bench render`. CI runs them every night and compares each run with the night before.

## Fonts

rustyprint bundles Noto Sans, Noto Serif and Noto Sans Mono, under the SIL Open Font License ([fonts/OFL.txt](fonts/OFL.txt)): `serif`, the initial font, is Noto Serif. Other fonts come from `@font-face` rules, in TrueType, OpenType, WOFF or WOFF2. System fonts are not used, so a document renders the same on every machine.

## Development

The crates in [crates/](crates/):

| Crate | What it does |
| --- | --- |
| `rustyprint-dom` | Parses HTML into a DOM |
| `rustyprint-css` | Parses CSS and computes styles: selectors, the cascade, values |
| `rustyprint-layout` | Box tree, layout, text, pagination and painting |
| `rustyprint-pdf` | Writes the PDF |
| `rustyprint` | The public API: `render(html, options)` |
| `rustyprint-cli` | The `rustyprint` command |
| `rustyprint-server` | The HTTP service |
| `rustyprint-ffi` | The native library: a C ABI over `rustyprint` |
| `rustyprint-tests` | The fixture suite |

`master` is protected: changes arrive through pull requests, which merge once CI passes. CI runs these on every pull request, so run them before you push:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo deny check
```

and, for the packages, `bundle exec rake` in [bindings/ruby](bindings/ruby) (the tests and the documentation check) and `dotnet test` in [bindings/dotnet](bindings/dotnet).

The fixture suite has one HTML file in [tests/fixtures/](tests/fixtures/) for every row of the trackers, with its expectations inside it. After changing a tracker or a milestone in the plan, regenerate the progress page with `cargo run -p rustyprint-tests --bin progress`. [CLAUDE.md](CLAUDE.md) has more about how the work is organised.

## License

rustyprint is licensed under either of

- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- the MIT license ([LICENSE-MIT](LICENSE-MIT)),

at your option. Unless you say otherwise, any contribution you submit for inclusion in rustyprint is licensed as above, without any additional terms or conditions.

The bundled Noto fonts are under the SIL Open Font License ([fonts/OFL.txt](fonts/OFL.txt)). Every package and archive carries the three licenses.
