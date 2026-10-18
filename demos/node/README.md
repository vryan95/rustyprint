# rustyprint Express demo

An Express app that uses the `rustyprint` package from [bindings/node](../../bindings/node) as an app would, by a `file:` dependency. It has four parts:

- **Samples** (`/`): the shared documents in [demos/samples](../samples). Each one is shown next to how your browser displays the same HTML, with its page count, render time, warnings and source.
- **Invoices** (`/invoices`): an HTML template, a JavaScript template literal filled from data in the app. The `.pdf` route passes it to `render` through [sendPdf](lib/pdf.js), with the stylesheet and logo it links to, read from the app's static files by [AssetCollector](lib/asset-collector.js). See [lib/invoices.js](lib/invoices.js).
- **Playground** (`/playground`): edit HTML, CSS and the page options, and see the PDF and the warnings as you type.
- **`npm run render-samples`**: writes every sample to a PDF in `out/samples`, or in the directory given after `--`.

## Run it

You need Node.js 22 or newer and Rust. Build the native library into the package first, as `rake compile` does for the gem, then start the app:

```sh
cd bindings/node
npm install
npm run compile
cd ../../demos/node
npm install
npm start
```

Open http://localhost:3000, or the port in `PORT`.

`RUSTYPRINT_LIBRARY` points the package at a native library somewhere else, such as `target/release/librustyprint_ffi.so`, with no need to compile. `RUSTYPRINT_SAMPLES` points the samples pages at another directory of samples.

## Test it

```sh
npm test
npm run render-samples
```

CI runs both, then starts the app and requests every sample and invoice as a PDF.
