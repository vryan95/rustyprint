# rustyprint ASP.NET Core demo

An ASP.NET Core app that uses the `Rustyprint` package from [bindings/dotnet](../../bindings/dotnet) as an app would, by a project reference. It has four parts:

- **Samples** (`/`): the shared documents in [demos/samples](../samples). Each one is shown next to how your browser displays the same HTML, with its page count, render time, warnings and source.
- **Invoices** (`/invoices`): a Razor component filled from data in the app. The `.pdf` endpoint renders it to HTML with `HtmlRenderer`, and [PdfViews](RustyprintDemo/Pdf/PdfViews.cs) passes it to `PdfRenderer.RenderAsync` with the stylesheet and logo it links to, read from the app's static files. See [InvoiceEndpoints.cs](RustyprintDemo/Invoices/InvoiceEndpoints.cs).
- **Playground** (`/playground`): edit HTML, CSS and the page options, and see the PDF and the warnings as you type.
- **`dotnet run -- render-samples`**: writes every sample to a PDF in `out/samples`, or in the directory given after it.

## Run it

You need the .NET 10 SDK and Rust. Building the app builds the native library with Cargo too, so there is nothing to build first:

```sh
cd demos/dotnet
dotnet run --project RustyprintDemo
```

Open the URL it prints, such as http://localhost:5000.

`RUSTYPRINT_LIBRARY` points the package at a native library somewhere else, such as `target/release/librustyprint_ffi.so`. `RUSTYPRINT_SAMPLES` points the samples pages at another directory of samples.

## Test it

```sh
dotnet test
dotnet run --project RustyprintDemo -- render-samples
```

CI runs both, then starts the app and requests every sample and invoice as a PDF.
