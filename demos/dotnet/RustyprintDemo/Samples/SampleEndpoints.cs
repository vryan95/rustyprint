using Microsoft.AspNetCore.Http.HttpResults;
using Microsoft.AspNetCore.StaticFiles;
using RustyprintDemo.Components.Pages;
using RustyprintDemo.Pdf;

namespace RustyprintDemo.Samples;

public static class SampleEndpoints
{
    private static readonly FileExtensionContentTypeProvider ContentTypes = new();

    public static void MapSamples(this IEndpointRouteBuilder app)
    {
        app.MapGet("/", () => new RazorComponentResult<SamplesIndex>(new { Samples = Sample.All }));

        // The sample rendered to PDF: inline, or as a download with ?download=1.
        app.MapGet("/samples/{id}.pdf", async Task<IResult> (string id, string? download, CancellationToken cancel) =>
            Sample.Find(id) is { } sample
                ? new PdfFile((await sample.RenderAsync(cancel)).Pdf, sample.Id, inline: string.IsNullOrEmpty(download))
                : Results.NotFound());

        // The sample's page: its PDF next to how the browser shows its HTML.
        app.MapGet("/samples/{id}", async Task<IResult> (string id, CancellationToken cancel) =>
            Sample.Find(id) is { } sample
                ? new RazorComponentResult<SampleShow>(new { Sample = sample, Rendering = await sample.RenderAsync(cancel) })
                : Results.NotFound());

        // The sample's document, as index.html, and its assets, so a browser can show the HTML as
        // rustyprint gets it.
        app.MapGet("/samples/{id}/files/{**path}", IResult (string id, string path) =>
        {
            if (Sample.Find(id) is not { } sample) return Results.NotFound();
            if (path == "index.html") return Results.Content(sample.Html, "text/html; charset=utf-8");
            if (sample.Asset(path) is not { } file) return Results.NotFound();

            return Results.File(file, ContentTypes.TryGetContentType(file, out var type) ? type : "application/octet-stream");
        });
    }
}
