# rustyprint for .NET

Renders HTML and CSS to PDF with [rustyprint](https://github.com/vryan95/rustyprint), a layout engine of its own written in Rust. It needs no browser, no external runtime and no network access, and the fonts it needs are built in.

```sh
dotnet add package Rustyprint
```

The package targets .NET 8 and newer. Its native library is included for Linux (glibc and musl) and macOS on x64 and arm64, and Windows on x64 and arm64.

## Render

```csharp
using Rustyprint;

var options = new RenderOptions
{
    PageSize = PageSize.A4,
    Margin = "15mm",
    Title = "Invoice 42",
};
options.Assets.AddFile("logo.png", "images/logo.png");
options.Assets.Add("invoice.css", File.ReadAllText("invoice.css"));

RenderOutput output = await PdfRenderer.RenderAsync(html, options);
File.WriteAllBytes("invoice.pdf", output.Pdf);
// output.Warnings: ["invoice.css:12: unsupported property `colr`"]
```

`PdfRenderer.Render` renders on the calling thread, and `RenderAsync` on the thread pool. Both return a `RenderOutput` with `Pdf`, the PDF's bytes, and `Warnings`, problems found in the document's CSS and assets. `RenderAsync` copies the options and assets when it is called. Its cancellation token stops a render that has not started yet. To bound one that has, set `Limits.Timeout`.

**Options** are the JSON options every rustyprint front end shares ([schema/render-options.schema.json](https://github.com/vryan95/rustyprint/blob/master/schema/render-options.schema.json)), as properties. An option left unset keeps its default. `ToJson()` gives the options as JSON, as the HTTP service takes them.

| Property | Example | Default |
| --- | --- | --- |
| `PageSize` | `PageSize.Letter`, `PageSize.Named("a5")`, `new PageSize("100mm", "150mm")` | A4 |
| `Landscape` | `true` | `false` |
| `Margin` | `"15mm"`, `"10mm 20mm"` | `"2cm"` |
| `Title` | `"Invoice 42"` | the document's `<title>` |
| `Stylesheet` | `"body { font-family: sans-serif }"` | none |
| `Remote` | `new RemoteOptions("cdn.example.com") { AllowHttp = true }` | nothing fetched |
| `Limits` | `new RenderLimits { MaxPages = 50, Timeout = TimeSpan.FromSeconds(10) }` | 20 MB of HTML, 500,000 nodes, 2,000 pages, 60 s |

`Limits.Timeout = Timeout.InfiniteTimeSpan` turns off the deadline.

**Assets** are the files the document refers to by relative or root-relative URL: stylesheets, images, fonts and `iframe` documents. Add each under the path the HTML uses, without a leading slash (`/css/app.css` is the asset `css/app.css`), as bytes, text, a stream or a file. `AddDirectory` adds every file in a directory by its relative path. Nothing else is read, and nothing is fetched from the network unless `Remote` allows the host.

**Errors** are `RustyprintException` subclasses, each with a `Kind`:

| Exception | `Kind` | When |
| --- | --- | --- |
| `InvalidOptionsException` | `InvalidOptions` | An invalid value, such as an unknown page size |
| `LimitExceededException` | `LimitExceeded` | The document exceeds a limit |
| `RenderTimeoutException` | `Timeout` | Rendering took longer than the deadline |
| `InternalRenderException` | `Internal` | A bug in rustyprint |

Text with a lone surrogate, which is not valid UTF-16, throws `ArgumentException`.

**Threads:** renders on several threads run at once. Each render runs on a thread of its own with a large stack, so deeply nested documents render from threads with small stacks too.

`RUSTYPRINT_LIBRARY` points the package at a native library somewhere else.

## In ASP.NET Core

Render a page to HTML, then to PDF, and send it:

```csharp
app.MapGet("/invoices/{id}.pdf", async (string id, CancellationToken cancel) =>
{
    var html = await RenderInvoiceHtmlAsync(id); // a Razor component, view or template
    var options = new RenderOptions();
    options.Assets.AddDirectory("wwwroot/invoice", "invoice"); // what the HTML links to, such as /invoice/logo.png
    var output = await PdfRenderer.RenderAsync(html, options, cancel);
    return Results.File(output.Pdf, "application/pdf", $"invoice {id}.pdf");
});
```

[demos/dotnet](https://github.com/vryan95/rustyprint/tree/master/demos/dotnet) is an ASP.NET Core app that does this. It renders a Razor component to HTML with `HtmlRenderer`, and finds the stylesheets and images the HTML refers to in the app's static files. It also has pages that compare the sample documents with how a browser shows them, and an HTML playground.

## Develop

From `bindings/dotnet` in the repository, with Rust and the .NET SDK installed:

```sh
dotnet test   # builds the native library with Cargo, then runs the tests
dotnet pack   # a package with the native library for this platform
```

The release workflow packs the native libraries of every platform, with `-p:RustyprintNativeDir=<dir>` naming a directory that holds `runtimes/<rid>/native/<library>` for each.

## License

MIT or Apache-2.0, at your option ([LICENSE-MIT](https://github.com/vryan95/rustyprint/blob/master/LICENSE-MIT), [LICENSE-APACHE](https://github.com/vryan95/rustyprint/blob/master/LICENSE-APACHE)). The fonts built into the native library are under the SIL Open Font License ([OFL.txt](https://github.com/vryan95/rustyprint/blob/master/fonts/OFL.txt)). The package carries all three.
