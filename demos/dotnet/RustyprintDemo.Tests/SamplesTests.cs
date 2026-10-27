using System.Net;
using System.Text.RegularExpressions;
using RustyprintDemo.Samples;

namespace RustyprintDemo.Tests;

public class SamplesTests(DemoApp app) : IClassFixture<DemoApp>
{
    private readonly HttpClient _client = app.CreateClient();
    private static CancellationToken Cancel => TestContext.Current.CancellationToken;

    [Fact]
    public async Task The_home_page_lists_every_sample()
    {
        var html = await _client.GetStringAsync("/", Cancel);
        Assert.Equal(5, Regex.Count(html, "class=\"sample-card\""));
        foreach (var id in new[] { "invoice", "report", "report-ar", "report-zh", "letter" })
        {
            Assert.Contains($"href=\"/samples/{id}\"", html, StringComparison.Ordinal);
            Assert.Contains($"href=\"/samples/{id}.pdf\"", html, StringComparison.Ordinal);
        }
    }

    [Fact]
    public async Task A_samples_page_shows_its_pdf_how_it_renders_in_a_browser_and_its_source()
    {
        var html = await _client.GetStringAsync("/samples/report", Cancel);
        Assert.Matches("<h1>Quarterly report, Q3 2026</h1>", html);
        Assert.Contains("<iframe src=\"/samples/report.pdf\"", html, StringComparison.Ordinal);
        Assert.Contains("<iframe src=\"/samples/report/files/index.html\"", html, StringComparison.Ordinal);
        Assert.Matches(@"<dd>8 pages</dd>", html);
        Assert.Contains("charts/revenue.png", html, StringComparison.Ordinal);
        Assert.Contains("PdfRenderer.RenderAsync", html, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("invoice", 3)]
    [InlineData("report", 8)]
    [InlineData("report-ar", 6)]
    [InlineData("report-zh", 7)]
    [InlineData("letter", 1)]
    public async Task Each_sample_renders_to_a_pdf_with_the_pages_it_is_meant_to_have(string id, int pages)
    {
        var response = await _client.GetAsync($"/samples/{id}.pdf", Cancel);
        Assert.Equal(HttpStatusCode.OK, response.StatusCode);
        Assert.Equal("application/pdf", response.Content.Headers.ContentType?.MediaType);
        Assert.Equal("inline", response.Content.Headers.ContentDisposition?.DispositionType);
        Assert.Equal($"{id}.pdf", response.Content.Headers.ContentDisposition?.FileName?.Trim('"'));
        var pdf = await response.Content.ReadAsByteArrayAsync(Cancel);
        Assert.True(Pdf.IsPdf(pdf));
        Assert.Equal(pages, Pdf.Pages(pdf));
    }

    [Fact]
    public async Task A_pdf_can_be_downloaded()
    {
        var response = await _client.GetAsync("/samples/letter.pdf?download=1", Cancel);
        Assert.Equal("attachment", response.Content.Headers.ContentDisposition?.DispositionType);
        Assert.Equal("letter.pdf", response.Content.Headers.ContentDisposition?.FileName?.Trim('"'));
    }

    [Fact]
    public async Task A_samples_own_files_are_served_for_the_browser_preview()
    {
        var page = await _client.GetAsync("/samples/letter/files/index.html", Cancel);
        Assert.Equal(HttpStatusCode.OK, page.StatusCode);
        Assert.Equal("text/html", page.Content.Headers.ContentType?.MediaType);

        var font = await _client.GetByteArrayAsync("/samples/letter/files/fonts/Caveat.ttf", Cancel);
        Assert.Equal(await File.ReadAllBytesAsync(Path.Combine(Sample.Find("letter")!.Dir, "fonts", "Caveat.ttf"), Cancel), font);
    }

    [Theory]
    [InlineData("/samples/letter/files/..%2Finvoice%2Flogo.png")]
    [InlineData("/samples/letter/files/missing.css")]
    [InlineData("/samples/nope")]
    [InlineData("/samples/nope.pdf")]
    public async Task Nothing_outside_a_sample_is_served(string path)
    {
        Assert.Equal(HttpStatusCode.NotFound, (await _client.GetAsync(path, Cancel)).StatusCode);
        Assert.Null(Sample.Find("letter")!.Asset("../invoice/logo.png"));
    }
}
