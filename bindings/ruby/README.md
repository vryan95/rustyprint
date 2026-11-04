# rustyprint for Ruby

Renders HTML and CSS to PDF with [rustyprint](https://github.com/vryan95/rustyprint), a layout engine of its own written in Rust. It needs no browser, no external runtime and no network access, and the fonts it needs are built in.

```ruby
gem "rustyprint"
```

Ruby 3.2 or newer. Each platform has a gem with the native library built in: Linux (glibc and musl) and macOS on x64 and arm64, and Windows (RubyInstaller, UCRT) on x64 and arm64. Other platforms get the source gem; see [Platforms](#platforms).

## Render

The API documentation, built from the gem's doc comments, is at [rubydoc.info/gems/rustyprint](https://rubydoc.info/gems/rustyprint).

```ruby
require "rustyprint"

output = Rustyprint.render(html, page_size: "A4", margin: "15mm", title: "Invoice 42",
                                 assets: { "logo.png" => File.binread("logo.png"),
                                           "invoice.css" => Pathname("app/pdf/invoice.css") })
File.binwrite("invoice.pdf", output.pdf)
output.warnings # => ["invoice.css:12: unsupported property `colr`"]
```

`render` returns a `Rustyprint::Output` with `pdf`, a binary String, and `warnings`, problems found in the document's CSS and assets, as an Array of Strings.

**Options** are keywords, the JSON options every rustyprint front end shares ([schema/render-options.schema.json](../../schema/render-options.schema.json)):

| Option | Example | Default |
| --- | --- | --- |
| `page_size` | `"Letter"`, or `{ width: "100mm", height: "150mm" }` | `"A4"` |
| `landscape` | `true` | `false` |
| `margin` | `"15mm"`, `"10mm 20mm"` | `"2cm"` |
| `title` | `"Invoice 42"` | the document's `<title>` |
| `stylesheet` | `"body { font-family: sans-serif }"` | none |
| `remote` | `{ allowed_hosts: ["cdn.example.com"] }` | nothing fetched |
| `limits` | `{ max_pages: 50, timeout_ms: 10_000 }` | 20 MB of HTML, 500,000 nodes, 2,000 pages, 60 s |

**Assets** are the files the document refers to by relative or root-relative URL: stylesheets, images, fonts and `iframe` documents. Give each by the path the HTML uses, without a leading slash (`/assets/app.css` is the asset `assets/app.css`), as a String, a Pathname or an IO. Nothing else is read, and nothing is fetched from the network unless `remote` allows the host.

**Errors** are `Rustyprint::Error` subclasses, each with a `kind`:

| Class | `kind` | When |
| --- | --- | --- |
| `Rustyprint::InvalidOptionsError` | `:invalid_options` | An unknown option, or an invalid value |
| `Rustyprint::LimitExceededError` | `:limit_exceeded` | The document exceeds a limit |
| `Rustyprint::TimeoutError` | `:timeout` | Rendering took longer than the deadline |
| `Rustyprint::InternalError` | `:internal` | A bug in rustyprint |

HTML that is not valid in its encoding raises `ArgumentError`. HTML in another encoding is converted to UTF-8.

**Threads:** a render releases the GVL, so other threads run meanwhile, and renders on several threads run at once.

## In Rails

In a Rails app the gem adds `render pdf:` to controllers. It renders the view to HTML, as `render_to_string` does, and sends the PDF:

```ruby
def show
  @invoice = Invoice.find(params[:id])

  respond_to do |format|
    format.html { render layout: "pdf" }
    format.pdf do
      render pdf: "invoice #{@invoice.id}",
        template: "invoices/show",
        layout: "pdf",
        disposition: "inline",
        formats: [:html]
    end
  end
end
```

- `pdf` names the file; `.pdf` is added.
- `disposition` is `"inline"`, the default, or `"attachment"`.
- `formats` defaults to `[:html]`. `template`, `layout`, `locals` and the rest are passed on to `render_to_string`.
- The render options, such as `page_size: "Letter"` or `margin: "15mm"`, are the same as for `Rustyprint.render`.

The view uses the usual helpers, such as `stylesheet_link_tag` and `image_tag`. The stylesheets, images and fonts the HTML and its stylesheets refer to by path are read from `public/`, where precompiled assets are. Anything not found there is fetched from the app's asset pipeline, as Propshaft or Sprockets serves it in development. `assets:` adds files of your own, by the path the HTML uses.

Each render is instrumented as `render.rustyprint`, with the asset paths and the warnings, and the warnings are logged.

When `config.asset_host` is set, asset URLs point at another host. The renderer only fetches from hosts that `remote:` allows, so leave `asset_host` unset for PDF views.

[demos/rails](../../demos/rails) is a Rails app that does this. It also has pages that compare the sample documents with how a browser shows them.

## Platforms

Each release has a gem with the native library built in for each of these platforms:

| Platform | Gem platform |
| --- | --- |
| Linux, glibc | `x86_64-linux-gnu`, `aarch64-linux-gnu` |
| Linux, musl (Alpine) | `x86_64-linux-musl`, `aarch64-linux-musl` |
| macOS | `arm64-darwin` (Apple silicon), `x86_64-darwin` (Intel) |
| Windows (RubyInstaller, UCRT) | `x64-mingw-ucrt`, `aarch64-mingw-ucrt` |

Each release also has a source gem, for the `ruby` platform, with the Ruby code but no native library. Because it exists, Bundler keeps `ruby` in your `Gemfile.lock`, so the lockfile still installs on platforms it doesn't list. On a listed platform, Bundler installs the matching platform gem. It installs the source gem only where no platform gem fits, or when `force_ruby_platform` is set. The source gem installs without errors, but `require "rustyprint"` raises a `LoadError` that names the platform, unless `RUSTYPRINT_LIBRARY` points at a native library.

- **If your platform is in the table**, add it to the lockfile, so that Bundler installs the platform gem:

  ```sh
  bundle lock --add-platform x86_64-linux arm64-darwin
  ```

- **If it isn't**, build the library from source with Rust ([rustup.rs](https://rustup.rs)), and set `RUSTYPRINT_LIBRARY` to its path:

  ```sh
  git clone --branch v<version> https://github.com/vryan95/rustyprint && cd rustyprint
  cargo build --release --locked -p rustyprint-ffi
  export RUSTYPRINT_LIBRARY=$PWD/target/release/librustyprint_ffi.so   # .dylib on macOS, rustyprint_ffi.dll on Windows
  ```

  Build the tag that matches the gem's version, since the gem and the library must be from the same release.

## Develop

From this directory, with Rust installed:

```sh
bundle install
bundle exec rake          # builds the native library with Cargo, runs the specs, then checks the documentation
bundle exec rake compile  # only builds the library, into lib/rustyprint/native
bundle exec rake docs     # fails unless every public class, method and constant is documented
```

`RUSTYPRINT_LIBRARY` points the gem at a library somewhere else. `rake native PLATFORM=x86_64-linux-gnu LIBRARY=path` builds a platform gem around a library, as the release workflow does for every platform. `rake source` builds the source gem, and fails if it would hold a native library.

## License

MIT or Apache-2.0, at your option ([LICENSE-MIT](https://github.com/vryan95/rustyprint/blob/master/LICENSE-MIT), [LICENSE-APACHE](https://github.com/vryan95/rustyprint/blob/master/LICENSE-APACHE)). The fonts built into the native library are under the SIL Open Font License ([OFL.txt](https://github.com/vryan95/rustyprint/blob/master/fonts/OFL.txt)). The gem carries all three.
