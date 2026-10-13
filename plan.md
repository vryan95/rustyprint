# rustyprint — Plan

A lightweight Rust engine that converts HTML and CSS to PDF with its own renderer. It ships as a command-line tool, an HTTP service, a Ruby gem and a NuGet package, all built on the same core.

## 1. Goals

- Support every element in the [HTML5 specification (W3C Working Draft, 5 April 2011)](https://www.w3.org/TR/2011/WD-html5-20110405/), rendered as static, paginated PDF.
- Support the CSS in the [MDN CSS reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference): the standard properties, selectors, at-rules, functions and value types that apply to HTML content.
- Ship a single static binary with fast startup, low memory and no external runtime, and the same engine as a native library for Ruby and .NET.
- Produce deterministic output: the same input always gives a byte-identical PDF.

## 2. Build vs. reuse

We build the engine ourselves and use existing crates only for low-level tasks.

| Stage | Approach | Crates |
| --- | --- | --- |
| HTML parsing | Reuse the parser; build our own DOM | `html5ever` |
| CSS parsing | Reuse the tokenizer; build the parser | `cssparser` |
| Selector matching | **Build** | — |
| Cascade and computed style | **Build** | — |
| Box tree and layout (block, inline, table, flex, grid, multi-column) | **Build** | — |
| Text shaping | Reuse | `harfrust` |
| Line breaking, bidi, hyphenation | Reuse the algorithms; build line layout | `unicode-linebreak`, `unicode-bidi`, `hyphenation` |
| Fonts (including variable and color fonts, WOFF and WOFF2) | Reuse | `skrifa`, `wuff` |
| Pagination | **Build** | — |
| PDF writing | Reuse | `pdf-writer`, `subsetter`, `miniz_oxide` |
| Color management (ICC profiles) | Reuse | `moxcms` |
| Rasterising filter effects | Reuse | `tiny-skia` |
| Images | Reuse | `image` |

## 3. Rendering pipeline

```text
HTML ──► [1] Parse ──► DOM
CSS  ──► [2] Parse ──► Stylesheets
                         │
          [3] Style: selector matching, cascade, inheritance, computed values
                         │
          [4] Box tree: display types, anonymous boxes, pseudo-elements
                         │
          [5] Layout: block, inline, table, flex, grid, multi-column
                         │
          [6] Fragmentation: split into pages (@page, breaks, orphans/widows)
                         │
          [7] Paint: per-page display list in stacking-context order
                         │
          [8] PDF backend: fonts subset + embedded, images, links, metadata
```

Layout supports page breaks from the start. Every layout mode gets the space remaining on the current page and can return a break token so layout continues on the next page.

`@container` queries and container units depend on layout results. A document with size containers is therefore styled, laid out, and styled again with its containers' sizes, until those sizes stop changing.

Animations render as they are at time zero: each animated property's keyframes are computed for the element and interpolated at the animation's progress, and the result applies at the animation origin of the cascade. Transitions never start.

Visual effects wrap what a box paints: filters, then the clip path, mask, opacity and blend mode as a transparency group, then the transform. PDF draws transparency groups, blend modes and affine transforms natively. Color filters change the colors of the display items, so text stays vector. What PDF cannot draw as vectors, such as blurs, masks and backdrops, is rasterised with `tiny-skia` at 192 dpi (less for very large areas) and embedded as images.

Every layout mode works in line-relative axes, where lines run left to right and stack from top to bottom. Boxes in a vertical writing mode get styles turned into those axes, are laid out by the same code as horizontal text, and have their fragments turned into their parent's axes afterwards. A box in another writing mode than its parent is laid out as a formatting context of its own.

### Key data structures

- `Dom`: arena of nodes (`NodeId`) with element names, attributes and text.
- `ComputedStyle`: resolved values per node, shared through `Arc`.
- `BoxTree`: block, inline, anonymous and replaced boxes.
- `Fragment`: the positioned output of layout. A box can produce several fragments across pages.
- `DisplayList`: `Vec<DisplayItem>` per page, with clips, transforms, opacity, blend and mask groups.

## 4. HTML support

All 107 elements in the spec's element index are supported. Default rendering follows the spec's [Rendering section (§10)](https://www.w3.org/TR/2011/WD-html5-20110405/rendering.html), implemented as a built-in UA stylesheet plus presentational attribute mapping (for example `width`/`height` on `img`, `border` on `table`, `align`, `bgcolor` and `hidden`). Scripting is always disabled, and interactive elements are drawn in their static state.

[html-elements.md](html-elements.md) tracks the implementation status of every element, along with the milestone that covers it.

## 5. CSS support

Coverage follows the categories of the MDN CSS reference. Any declaration that is unknown or invalid is dropped, as the CSS rules require, and reported as a warning.

The PDF is a static snapshot. Anything that depends on interaction, scrolling, playback or time resolves to its initial, static state. Each tracker says how this applies to every entry.

These files track implementation status, each with the milestone that covers every entry:

- [css-properties.md](css-properties.md): properties, grouped by module, with how each module maps to PDF.
- [css-selectors.md](css-selectors.md): selectors, combinators, pseudo-classes and pseudo-elements.
- [css-at-rules.md](css-at-rules.md): at-rules, descriptors, page margin boxes and media features.
- [css-values.md](css-values.md): CSS-wide keywords, data types, units and functions.

[progress.html](progress.html) summarises every tracker by status and milestone. Regenerate it with `cargo run -p rustyprint-tests --bin progress`; the fixture suite fails when it is out of date.

## 6. Front ends and packaging

Every front end is a thin layer over the `rustyprint` crate. The CLI and the server link it directly. Ruby and .NET load it through `rustyprint-ffi`, a native library with a C ABI, so both packages share one native build per platform. Everything lives in this repository and is released together under one version.

### Workspace layout

```text
rustyprint/
├─ .github/workflows/            # ci.yml, release.yml, nightly.yml
├─ Cargo.toml                    # workspace: crates/* only
├─ crates/
│  ├─ rustyprint-dom/            # HTML → arena DOM
│  ├─ rustyprint-css/            # CSS parsing, selectors, cascade, computed style, UA stylesheet
│  ├─ rustyprint-layout/         # box tree, layout modes, text, fragmentation, display list
│  ├─ rustyprint-pdf/            # display list → PDF (fonts, images, links, effects)
│  ├─ rustyprint/                # public API: render(html, options) -> Output
│  ├─ rustyprint-ffi/            # C ABI over the public API (cdylib), header from cbindgen
│  ├─ rustyprint-cli/            # `rustyprint in.html -o out.pdf`
│  ├─ rustyprint-server/         # axum HTTP service
│  └─ rustyprint-tests/          # fixture suite: one test per tracker row
├─ bindings/                     # not part of the Cargo workspace
│  ├─ ruby/                      # the `rustyprint` gem: gemspec, lib/, spec/, Rakefile
│  └─ dotnet/                    # the `Rustyprint` NuGet package: solution, src/, tests/
├─ demos/                        # demo projects that use the packages; not published
│  ├─ samples/                   # sample documents with their assets, shared by the demos
│  ├─ rails/                     # Rails app using the gem
│  └─ dotnet/                    # ASP.NET Core app and console command using the package
├─ schema/
│  └─ render-options.schema.json # the JSON render options, shared by every front end
├─ docker/                       # server image
├─ fonts/                        # bundled default fonts
└─ tests/
   ├─ fixtures/<tracker>/        # one HTML fixture per tracked element or CSS feature
   └─ snapshots/                 # expected layout trees / rasterised pages
```

### Embedding API

`rustyprint::render(html, &RenderOptions) -> Result<Output, Error>` is the only entry point. To serve every front end it needs:

- **Options as JSON:** `RenderOptions` deserialises (behind a `serde` feature) from the format in `schema/render-options.schema.json`: page size by name or as lengths, orientation, margins as CSS lengths, title, extra stylesheet, remote hosts and limits. Page-size names and length parsing move from the CLI into the core.
- **Limits:** the resource limits listed under [Security](#security), in `RenderOptions::limits`. They are checked while the render runs: the deadline for every element styled, block laid out, line broken and page written, the node count and depth for every document parsed, the page count for every page laid out. A check that fails unwinds the render thread to `render`, which returns the error.
- **Errors by kind:** invalid options, limit exceeded, deadline passed and internal error. A panic is caught inside `render` and returned as an internal error, so no front end can be brought down by one.
- **Assets by path:** `Assets` gives the files a document refers to by relative or root-relative URL, by the path the document uses: `img/logo.png`, and `/assets/app.css` as `assets/app.css`, so a web app's views work as they are. Paths cannot leave the asset root: `..` segments, protocol-relative URLs and URLs with a scheme are not assets.
- **Concurrency:** `render` is safe to call from several threads at once. Fonts, the UA stylesheet and hyphenation dictionaries are loaded once per process and shared.
- **Its own stack:** layout recurses, using about 26 KB of stack per nesting level in a release build, so a thread with a 1 MB stack (the Windows main thread, Ruby and .NET threads) overflows at about 40 levels and the process aborts. `render` therefore runs the pipeline on a thread of its own with a large stack (256 MB, reserved rather than committed), so the caller's stack size never matters. Reducing the stack used per level is worthwhile on its own.

### CLI

`rustyprint in.html -o out.pdf`:

- `-` reads the HTML from standard input or writes the PDF to standard output. Warnings go to standard error, so standard output carries only the PDF.
- `--options file.json` takes the shared JSON options; the flags (`--page-size`, `--landscape`, `--margin`, `--title`, `--css`, `--allow-host`, `--allow-http`) override what it sets. `--page-size` keeps the orientation the options give.
- Relative and root-relative URLs resolve against `--assets DIR`, else the input file's directory; HTML from standard input has no directory unless one is given. `--asset NAME=FILE`, repeatable, serves a file under the name the document uses, ahead of the directory.
- `--fail-on-warnings` fails without writing the PDF if rendering gives any warnings.
- Exit codes, listed in `--help`: 0 success, 1 a file could not be read or written, 2 invalid arguments or options, 3 a limit exceeded, 4 the deadline passed, 5 warnings with `--fail-on-warnings`, 70 an internal error.

Release archives are published for each platform (milestone 22).

### HTTP service

`rustyprint-server`, published as the image `ghcr.io/vryan95/rustyprint-server`.

**`POST /v1/pdf`** with a JSON body, where `options` takes the shared JSON options:

```json
{
  "html": "<html>...</html>",
  "options": { "page_size": "A4", "margin": "15mm", "title": "Invoice 42" }
}
```

**`POST /v1/pdf`** with a `multipart/form-data` body: an `index.html` part, an optional `options` part with the JSON options, and a part for each file the document refers to (images, fonts, CSS, iframe documents, ICC profiles), named by the path the HTML uses.

The response is `200 application/pdf`. Warnings are in the `X-Rustyprint-Warnings` header, as a JSON array with everything outside ASCII escaped (the first 50, each cut to 300 characters), and `X-Rustyprint-Warning-Count` has how many there were. Errors are JSON, `{"error": kind, "message": ...}`:

| Status | `error` | When |
| --- | --- | --- |
| 400 | `invalid_request` | The body is not valid JSON or multipart, or has no HTML |
| 400 | `invalid_options` | The options are invalid, or ask for what the server does not allow |
| 413 | `payload_too_large` | The body is larger than the server allows |
| 415 | `unsupported_media_type` | The body is neither JSON nor multipart |
| 422 | `limit_exceeded` | The document exceeds a limit |
| 504 | `timeout` | Rendering took longer than the deadline |
| 500 | `internal` | A bug in rustyprint |

**`GET /healthz`** answers `ok`. **`GET /metrics`** gives Prometheus metrics: requests by status, errors by kind, a render time histogram and the renders running.

**Configuration**, from environment variables: the address (`RUSTYPRINT_ADDR`, default `0.0.0.0:8080`), how many renders run at once (`RUSTYPRINT_CONCURRENCY`, default the number of CPUs; other requests wait), the largest body (`RUSTYPRINT_MAX_BODY_BYTES`, default 50 MB), the hosts requests may fetch from (`RUSTYPRINT_ALLOWED_HOSTS`, comma-separated, default none) and whether over plain `http` (`RUSTYPRINT_ALLOW_HTTP`), a directory relative and root-relative URLs resolve against (`RUSTYPRINT_ASSETS_DIR`), and the highest limits (`RUSTYPRINT_LIMITS`, as the `limits` JSON option).

**The server decides what requests may do.** A request's options can lower the limits but never raise them above the server's, and limits a request leaves out are the server's. A request may only fetch from hosts the server allows, so a caller cannot make the server fetch from internal addresses; plain `http` needs the server's permission too.

The stack:

- `axum` + `tokio`. Rendering runs on `spawn_blocking`, and a semaphore limits how many renders run at once.
- Middleware of its own logs every request and counts it for the metrics; logs have no colors unless written to a terminal.
- The server stops on Ctrl+C or SIGTERM, after finishing the requests in progress.
- `docker/server.Dockerfile` puts the static `musl` binary on `gcr.io/distroless/static-debian12:nonroot`: no shell, running as an unprivileged user. CI builds the image and renders a document through it; the release workflow publishes it for `linux/amd64` and `linux/arm64`, tagged with the version and, for a release that is not a prerelease, `latest`.

### Native library

`rustyprint-ffi` builds a shared library (`rustyprint_ffi.dll`, `librustyprint_ffi.so`, `librustyprint_ffi.dylib`) with a small C ABI. It is not named `rustyprint`, whose files would collide with the CLI's (`rustyprint.pdb` on Windows) and the core crate's.

```c
rp_request *rp_request_new(const char *options_json, size_t len);
rp_status   rp_request_add_asset(rp_request *, const char *path, const uint8_t *data, size_t len);
rp_status   rp_render(const rp_request *, const char *html, size_t len, rp_result **out);
rp_status   rp_result_status(const rp_result *);
const uint8_t *rp_result_pdf(const rp_result *, size_t *len);
const char *rp_result_warnings_json(const rp_result *, size_t *len);
const char *rp_result_error(const rp_result *, size_t *len);
void        rp_result_free(rp_result *);
void        rp_request_free(rp_request *);
const char *rp_version(void);
```

- Options cross the boundary as JSON, so new options never change the ABI. Invalid options still make a request; rendering with it fails with `RP_STATUS_INVALID_OPTIONS` and the reason.
- Requests and results are opaque handles. Memory allocated by the library is freed only by the library, through the `*_free` functions. `rp_render` gives a result even when it fails, so the error message can be read from it.
- `rp_status` is the error kind: ok, invalid argument (a null pointer or text that is not UTF-8), invalid options, limit exceeded, timeout and internal.
- A request can render on several threads at once, as long as no thread adds assets to it meanwhile.
- Every entry point catches panics and returns an error code with a message.
- It is the only crate allowed `unsafe` code: it opts out of the workspace lints and sets its own.
- `cbindgen` generates the C header, `include/rustyprint.h`, which is checked in; a test fails when it is out of date. A C smoke test, built with the platform's C compiler through the `cc` crate, runs in `cargo test` on every CI platform, and against each library the release workflow builds that can run on its runner.
- Built for Linux x64 and arm64 (glibc and musl), macOS x64 and arm64, and Windows x64 and arm64, and attached to each release as `rustyprint-native-<version>-<target>` archives with the header. The musl library links the C runtime dynamically, since a static one rules out a shared library.

Calling the library costs microseconds plus copying the input and the PDF, which is negligible next to a render. Starting the CLI for each document costs far more.

### Ruby gem

`rustyprint` on RubyGems, built on the `ffi` gem, so one native library per platform works with every Ruby version. `Rustyprint.render(html, page_size: "A4", margin: "15mm", assets: {...})` returns the PDF and warnings and raises `Rustyprint::Error` subclasses. Renders release the GVL. A prebuilt gem is published for each platform.

In a Rails app the gem adds `render pdf:` to controllers, so a view renders as a PDF from `respond_to` with no helper gem:

```ruby
format.pdf do
  render pdf: "invoice #{@invoice.id}", template: "invoices/show", layout: "pdf",
         disposition: "inline", formats: [:html]
end
```

It renders the view to HTML with `render_to_string`, taking `template`, `layout`, `formats` (default `[:html]`) and the other `render` options, and the render options as keywords. The stylesheets, images and fonts the HTML and its stylesheets refer to are found by path, such as the root-relative `/assets/...` URLs of `stylesheet_link_tag` and `image_tag`: read from `public/`, where precompiled assets are, or else fetched from the app itself, as Propshaft or Sprockets serves them in development. The PDF is sent with `Content-Disposition` from `pdf` (the file name) and `disposition`. Each render is instrumented as `render.rustyprint`, with the asset paths and warnings, and the warnings are logged.

### .NET package

`Rustyprint` on NuGet, targeting `net8.0`, for .NET 8 and newer; .NET Framework is not supported. It calls the native library through P/Invoke and wraps it in `PdfRenderer.Render(html, options)` and `RenderAsync(...)`, with typed options, a `SafeHandle` for results and a `RustyprintException` hierarchy. The native libraries ship in `runtimes/{rid}/native/`. The class is `PdfRenderer` rather than `Rustyprint`, since a class named like its namespace cannot be called as `Rustyprint.Render` after `using Rustyprint;`.

```csharp
var options = new RenderOptions { PageSize = PageSize.A4, Margin = "15mm" };
options.Assets.AddFile("logo.png", "wwwroot/logo.png");
RenderOutput output = await PdfRenderer.RenderAsync(html, options, cancellationToken);
```

- `RenderOptions` has a property for each JSON option, with `TimeSpan` for times (`Timeout.InfiniteTimeSpan` for no deadline), and `Assets`, the files the document uses by path. `ToJson()` gives the JSON options.
- `RenderAsync` copies the options and assets when called, and renders on the thread pool. Its cancellation token stops a render that has not started; the render deadline bounds one that has.
- The exceptions are `InvalidOptionsException`, `LimitExceededException`, `RenderTimeoutException` and `InternalRenderException`, each with a `Kind`.
- The package's version is read from `Cargo.toml`. Building the project builds the native library with Cargo for the current platform; the release workflow packs the libraries of every platform instead.

### Demo projects

Each package comes with a demo project that shows rustyprint at work, using the package as its users would:

- **Sample documents** in `demos/samples/`, shared by every demo: an invoice with a logo and a long table of lines, a report with headings, a table of contents, charts as images, page headers and footers and page numbers, and a letter. Each is an HTML file with its stylesheet, images and fonts beside it.
- **Ruby demo** in `demos/rails/`: a Rails app that uses the gem as a Rails app would, with the Gemfile using the gem from `bindings/ruby` by path. Its pages:
  - **Samples:** each sample rendered to PDF next to how the browser shows the same HTML, with the page count, render time, warnings and source. The PDF opens in the browser or downloads.
  - **Invoices:** an ERB view filled from data in the app, which the controller answers `.pdf` requests for through `respond_to` and `render pdf:`, with the stylesheet and logo from the asset pipeline.
  - **Playground:** HTML, CSS and the page options, rendered to PDF as they are edited, with the warnings and the page size shown.
  - `bin/rails samples:render` writes every sample to a PDF from the command line.

- **.NET demo** in `demos/dotnet/`: the same in an ASP.NET Core app, rendering with `RenderAsync`, and a console command, `dotnet run -- render-samples`, that writes every sample to a PDF. It references the package project in `bindings/dotnet`. Its invoice is a Razor component, rendered to HTML with `HtmlRenderer`, whose stylesheet and logo are found in the app's static files by the paths the HTML uses.

Each demo has a README with the commands to run it. CI starts each demo and renders every sample through it, so the demos keep working as the packages change. The demos are not published.

### Continuous integration

The repository is on GitHub, and GitHub Actions runs every check. `.github/workflows/ci.yml` runs on every pull request and every push to `master`:

- **Checks:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `progress.html` being up to date.
- **Tests:** `cargo test --workspace`, including the fixture suite, on Linux, Windows and macOS, with the toolchain pinned to the workspace's `rust-version`.
- **Bindings:** the Ruby and .NET test suites, each against a native library built in the same run. The demos are started and render every sample. Jobs run only when their part changed: a change under `bindings/ruby/`, `demos/samples/` or `demos/rails/` runs the Ruby jobs, one under `bindings/dotnet/`, `demos/samples/` or `demos/dotnet/` the .NET jobs, and a change under `crates/` runs everything.
- **Dependencies:** `cargo deny` (configured in `deny.toml`) for licences, security advisories, banned crates and crate sources. Unmaintained crates fail it too, except the known ones it lists with the reason for each.
- **Speed:** `Swatinem/rust-cache` caches builds, and a newer run on the same branch cancels an older one.

Until milestone 28, work is pushed straight to `master`, and CI checks every push. From milestone 28, `master` is protected: pull requests merge only when the `CI result` check passes, a job that passes only when every other CI job did, so jobs can be added without changing the protection rule.

`.github/workflows/nightly.yml` runs what takes too long for every pull request, each night and on demand:

- **Fuzzing:** the `cargo-fuzz` targets in `fuzz/` (the HTML parser, the CSS parser and the whole pipeline) for 10 minutes each, seeded with the fixtures. The corpus is kept from night to night, and an input that crashes is kept as an artifact. The pipeline target treats `Error::Internal`, a caught panic, as a crash.
- **Benchmarks:** `cargo bench -p rustyprint --bench render` (criterion): a 1-page letter, a 21-page report and an invoice with a 200-row table. Each run is compared with the previous night's, and the results are shown in the run's summary.

### Releases

- The workspace version is the version of every crate, the gem and the NuGet package.
- Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`. It first checks that the tag matches the workspace version, then runs the CI checks, then builds and publishes. A version with a suffix, such as `1.2.0-rc.1`, is published as a prerelease. Run by hand from the Actions tab, the workflow is a dry run: it builds every archive and keeps them as the run's artifacts, without a release.
- **Builds** run on the standard x64 runners (`ubuntu-latest`, `windows-latest`) and `macos-latest`, so they work in a private repository too; the arm64 Linux and Windows builds and the x64 macOS build are cross-compiled. The Linux builds use `cargo-zigbuild`: the glibc ones run on glibc 2.17 and later, and the `musl` ones are static. Each build that can run on its runner renders a document before it is packaged.
- **CLI archives:** `rustyprint-<version>-<target>.tar.gz` (`.zip` for Windows), holding the program, the README and the fonts' licence, each with a `.sha256` file, and `SHA256SUMS` for all of them. Targets: `x86_64` and `aarch64` for `unknown-linux-gnu`, `unknown-linux-musl`, `apple-darwin` and `pc-windows-msvc`.
- **Artifacts:** CLI archives with SHA-256 checksums on a GitHub Release, the server image on GitHub Container Registry, the native libraries attached to the release, the NuGet package on nuget.org and the platform gems on rubygems.org.
- **Credentials:** nuget.org and rubygems.org use trusted publishing (OIDC) from the release workflow, and GitHub Container Registry uses the workflow's own token, so no long-lived API keys are stored in the repository.
- Release notes come from the merged pull requests since the previous tag.
- Each front end's milestone adds its artifacts to the release workflow, so the CLI can ship before the packages exist.
- Locally, `rake compile` in `bindings/ruby` and the build of `bindings/dotnet` build the native library for the current platform only.

### Security

- **No network access by default:** only assets from the request or the configured asset directory are used. Remote images and stylesheets are fetched only from an allow-list of hosts (`Assets::remote`, `--allow-host` in the CLI), over `https` unless `http` is allowed too. Every redirect must stay on the allowed hosts, and each fetch is limited in time and size, as is the number of fetches per render.
- **Resource limits**, set per render, which fail the render when exceeded:
  - input size (default 20 MB)
  - DOM node count, across the document and its `iframe` documents (500,000), and element nesting depth (512)
  - page count (2,000)
  - image size (100 megapixels): larger images are skipped with a warning, without being decoded
  - a render deadline (60 seconds)
- **Fixed limits** in the engine, which degrade the output instead: `iframe` nesting (4 levels, deeper frames are empty), `@import` nesting (16), `@font-face` files (64), and filter rasters (16 megapixels, at a lower resolution beyond that).
- **Robustness:** fuzz the parsers and the full pipeline with `cargo-fuzz`. Panics are caught in the core and reported by each front end as an error: an exit code, a `422` response or an exception.

## 7. Testing

- **Unit tests** for each stage, for example selector matching, cascade, value parsing, line breaking and bidi.
- **Fixture suite** (`cargo test -p rustyprint-tests`): one test per row of every tracker file, each backed by an HTML fixture in `tests/fixtures/<tracker>/` with its expectations inside the fixture. Rows marked `todo` run as ignored tests; any other status needs a passing fixture. Fixtures without a tracker row fail.
- **Web Platform Tests:** run the applicable `css/` and `html/rendering/` reftests and track the pass rate per module.
- **Layout snapshot tests** (`insta`): a text dump of the positioned fragments for each fixture.
- **Visual reference tests:** each PDF is rasterised (`pdftoppm`) and compared against approved PNGs.
- **Determinism test:** the same input must produce a byte-identical PDF, from the core and from every front end: the CLI, the server, Ruby and .NET.
- **Options schema test:** `RenderOptions` accepts exactly what `schema/render-options.schema.json` describes, and the Ruby and .NET options are tested against the same schema.
- **Binding tests:** the gem and the NuGet package each have their own test suite, run on every platform they ship for.
- **Demo smoke tests:** CI starts each demo and renders every sample document through it.
- **Fuzzing** nightly in CI.
- **Benchmarks** (`criterion`): a 1-page letter, a 20-page report and an invoice with a 200-row table.

## 8. Milestones

The **Status** column is where the work stands: `todo` (not started), `partial` (in progress) or `done`. Update it when a milestone starts and when it is finished, then regenerate [progress.html](progress.html). The fixture suite checks it against the trackers: a `done` milestone has no `todo` rows, and a `todo` milestone has only `todo` rows.

| # | Milestone | Status | Outcome |
| --- | --- | --- | --- |
| 1 | **Hello PDF** | done | Workspace and CLI. Hard-coded text in a bundled font, subset and embedded with `pdf-writer`. |
| 2 | **DOM and CSS core** | done | Arena DOM, CSS parser, all selectors, the cascade (origins, layers, `@scope`, specificity), custom properties, `@media`, `@supports`, `@import`, `@namespace`, values, units, math and color functions, the §10 UA stylesheet and presentational attributes. |
| 3 | **Block layout** | done | Box tree, block formatting context, box model, sizing, logical properties, overflow clipping, backgrounds and borders. |
| 4 | **Inline layout and text** | done | Shaping, line breaking, bidi, hyphenation, every text and text-decoration property, and the font properties for installed fonts. |
| 5 | **Pagination** | done | `@page` with page selectors, margin boxes and named pages, fragmentation properties, `box-decoration-break`. |
| 6 | **Lists, generated content and links** | done | `::marker`, counters, `@counter-style`, `content`, `quotes`, `::first-letter`, `::first-line`, PDF links and bookmarks. |
| 7 | **Images and embedded content** | done | Image elements, `object-*`, gradients and other image functions, image maps, `iframe`, `video` posters, fallback content. |
| 8 | **Tables** | done | Table layout, captions, column groups, collapsed borders, header and footer rows repeated across pages. |
| 9 | **Forms and interactive** | done | Static form controls, form-state pseudo-classes, `accent-color`, `appearance`, `details`/`summary`, `menu`. |
| 10 | **Positioning and floats** | done | `position`, stacking contexts, `z-index`, floats, shapes, anchor positioning. |
| 11 | **Flexbox** | done | Flex layout and alignment, including fragmentation. |
| 12 | **Grid** | done | Grid layout, including subgrid and fragmentation. |
| 13 | **Multi-column** | done | Column layout, `column-span`, balancing across pages. |
| 14 | **Writing modes and ruby** | done | Vertical text, `text-orientation`, `text-combine-upright`, ruby layout. |
| 15 | **Visual effects** | done | Transforms, opacity, blend modes, `clip-path`, masks, shadows, filters. |
| 16 | **Advanced fonts** | done | `@font-face`, variable fonts, color fonts, `@font-feature-values`, `@font-palette-values`. |
| 17 | **Advanced cascade** | done | `@container`, `@property`, `@function`, `@color-profile`, containment, static rendering of animations and transitions, the properties with no visual effect. **All HTML elements and in-scope CSS are covered.** |
| 18 | **Embedding API** | done | JSON render options and their schema, page sizes and lengths in the core, resource limits and the render deadline, error kinds, panics caught in `render`, concurrent renders, rendering on a thread with its own large stack. |
| 19 | **Continuous integration** | done | The repository on GitHub, with a CI workflow on every pull request and push: formatting, clippy, the tests on Linux, Windows and macOS, dependency checks, and a `CI result` check that sums them up; nightly fuzzing and benchmarks. |
| 20 | **Dependency maintenance** | done | Every dependency maintained and up to date: `rustybuzz` replaced by `harfrust` and `ttf-parser` by `skrifa`, a maintained alternative to `hyphenation`'s use of `bincode` or a reason to keep it, the exceptions removed from `deny.toml`, and every dependency on its latest release, with the fixture suite and benchmarks showing no regressions. Dependabot opens pull requests for new Cargo and GitHub Actions releases from then on. |
| 21 | **CLI** | done | Standard input and output, `--options`, `--asset`, `--fail-on-warnings`, an exit code per error kind. |
| 22 | **Release pipeline** | done | One version across every package; a `vX.Y.Z` tag checks the version, builds the CLI for every platform and publishes it on a GitHub Release with checksums and release notes. |
| 23 | **HTTP service** | done | axum server, JSON and multipart APIs, limits, metrics, warnings; its image built in CI and published to GitHub Container Registry on release. |
| 24 | **Native library** | done | `rustyprint-ffi` with its C ABI and generated header, a C smoke test, built for every platform in CI and attached to releases. |
| 25 | **Ruby gem** | done | `rustyprint` gem on `ffi`, releasing the GVL during renders, with a prebuilt gem for every platform; tested in CI and published to rubygems.org on release. `render pdf:` for Rails controllers. The shared sample documents, and a Ruby demo: a Rails app that renders an ERB invoice view as a PDF through `respond_to` and `render pdf:`, with the samples next to the browser's rendering, an HTML playground and a rake task that renders every sample. |
| 26 | **.NET package** | partial | `Rustyprint` NuGet package with sync and async rendering, typed options and exceptions, native libraries for every platform; tested in CI and published to nuget.org on release. A .NET demo: an ASP.NET Core app with the samples and an HTML playground, and a console command that renders every sample. |
| 27 | **First release** | todo | The determinism test across every front end, documentation for each package, and version 1.0.0 released through the pipeline. |
| 28 | **Protected master** | todo | `master` protected on GitHub: changes arrive through pull requests, which merge only when the `CI result` check passes. Work moves from pushing to `master` to branches and pull requests. |

## 9. Open questions

1. **Fonts:** Noto Serif, Noto Sans (regular, bold, italic and bold italic) and Noto Sans Mono (regular and bold) are bundled under the SIL Open Font License; `serif`, the initial family, maps to Noto Serif. Should more scripts be bundled (CJK, Arabic, Hebrew), and should system fonts be used as well?
2. **Package size:** the bundled fonts make every binary and native library about 22 MB. Should the packages also come in a slim build that loads fonts from a directory?
3. **Platforms:** are all six platforms needed for the first release, or are Linux x64, Windows x64 and macOS arm64 enough to start?
4. **Package names:** `rustyprint` on RubyGems and `Rustyprint` on NuGet were both free on 13 October 2026. They are taken by the first publish.
5. **crates.io:** should the crates be published for Rust users? crates.io needs every engine crate published, or merged into one crate first; otherwise they are marked `publish = false`.
6. **Repository visibility:** the repository is private, so CI uses the account's GitHub Actions minutes, at higher rates on Windows and macOS runners. Before milestone 22, check whether the arm64 runners the release workflow needs are available to private repositories on the account's plan. Public repositories get the standard runners for free.
7. **Dependencies held back:** `hyphenation` reads its embedded dictionaries with the unmaintained `bincode` (RUSTSEC-2025-0141). It stays, listed in `deny.toml`: no document data reaches `bincode`, and the maintained `hypher` has 48 of its 78 dictionaries. `moxcms` stays on 0.8 to share one copy with `image`, and `subsetter` still uses an older `skrifa`, so two versions of the font-reading crates are built. Dependabot proposes each update when it becomes possible.
8. **Licence:** the workspace crates have no licence yet. One is needed before the packages are published.
