using System.Diagnostics;
using Microsoft.AspNetCore.Http.HttpResults;
using Microsoft.AspNetCore.Mvc;
using Rustyprint;
using RustyprintDemo.Components.Pages;

namespace RustyprintDemo.Playground;

// Edit HTML, CSS and the render options, and see the PDF and the warnings at once.
public static class PlaygroundEndpoints
{
    public static readonly string[] PageSizes = ["A4", "A5", "A3", "Letter", "Legal"];

    public static void MapPlayground(this IEndpointRouteBuilder app)
    {
        app.MapGet("/playground", () => new RazorComponentResult<PlaygroundPage>(new { Html = ExampleHtml, Css = ExampleCss }));

        // The form's fields are bound from the form, so the antiforgery token is checked too.
        app.MapPost("/playground", async ([FromForm] string? html, [FromForm] string? css, [FromForm(Name = "page_size")] string? pageSize,
            [FromForm] string? margin, [FromForm] string? landscape, CancellationToken cancel) =>
        {
            var options = new RenderOptions
            {
                Stylesheet = string.IsNullOrEmpty(css) ? null : css,
                PageSize = PageSizes.Contains(pageSize) ? PageSize.Named(pageSize!) : null,
                Landscape = landscape == "1",
                Margin = string.IsNullOrEmpty(margin) ? null : margin,
                // A playground renders whatever it is given, so it gets tighter limits than the defaults.
                Limits = new RenderLimits { MaxPages = 20, Timeout = TimeSpan.FromSeconds(10) },
            };
            try
            {
                var started = Stopwatch.GetTimestamp();
                var output = await PdfRenderer.RenderAsync(html ?? "", options, cancel);
                var elapsed = Stopwatch.GetElapsedTime(started);
                return Results.Json(new
                {
                    pdf = Convert.ToBase64String(output.Pdf),
                    pages = new Samples.Rendering(output.Pdf, output.Warnings, elapsed).Pages,
                    page_mm = Format.PageMillimetres(output.Pdf),
                    milliseconds = (long)elapsed.TotalMilliseconds,
                    warnings = output.Warnings,
                });
            }
            catch (Exception error) when (error is RustyprintException or ArgumentException)
            {
                return Results.Json(new { error = error.Message }, statusCode: StatusCodes.Status422UnprocessableEntity);
            }
        });
    }

    public const string ExampleHtml = """
        <h1>Hello from the playground</h1>
        <p>
          Edit the HTML and CSS, choose the page options, and the PDF on the right updates.
          Problems found in the CSS show as warnings below.
        </p>
        <table>
          <thead><tr><th>Item</th><th>Qty</th><th>Price</th></tr></thead>
          <tbody>
            <tr><td>Oak desk</td><td>2</td><td>€ 978.00</td></tr>
            <tr><td>Office chair</td><td>4</td><td>€ 1,316.00</td></tr>
            <tr><td>Desk lamp</td><td>4</td><td>€ 259.80</td></tr>
          </tbody>
        </table>
        """;

    public const string ExampleCss = """
        @page {
          @bottom-right { content: "Page " counter(page) " of " counter(pages); font-size: 9pt; }
        }
        body { font-family: "Noto Sans", sans-serif; color: #222; }
        h1 { color: #b7410e; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 6px 8px; border-bottom: 1px solid #ddd; text-align: left; }
        th { background: #f4f1ea; }
        """;
}
