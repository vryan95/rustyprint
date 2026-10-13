# rustyprint Rails demo

A Rails app that uses the `rustyprint` gem from [bindings/ruby](../../bindings/ruby) as a Rails app would. It has four parts:

- **Samples** (`/`): the shared documents in [demos/samples](../samples). Each one is shown next to how your browser displays the same HTML, with its page count, render time, warnings and source.
- **Invoices** (`/invoices`): an ERB view filled from data in the app. The controller answers `.pdf` requests with `render pdf:`, which takes the stylesheet and logo from the asset pipeline. See [invoices_controller.rb](app/controllers/invoices_controller.rb).
- **Playground** (`/playground`): edit HTML, CSS and the page options, and see the PDF and the warnings as you type.
- **`bin/rails samples:render`**: writes every sample to a PDF in `tmp/samples`, or in the directory given by `OUT`.

## Run it

You need Ruby (see [.ruby-version](.ruby-version)) and Rust. Build the native library once, and again after changing the Rust code:

```sh
cd bindings/ruby
bundle install
bundle exec rake compile
```

Then start the app:

```sh
cd demos/rails
bundle install
bin/rails server
```

Open http://localhost:3000.

`RUSTYPRINT_LIBRARY` points the gem at a native library somewhere else, such as `target/release/librustyprint_ffi.so`. `RUSTYPRINT_SAMPLES` points the samples pages at another directory of samples.

## Test it

```sh
bin/rails test
bin/rails samples:render
```

CI runs both, then starts the app and requests every sample and invoice as a PDF.
