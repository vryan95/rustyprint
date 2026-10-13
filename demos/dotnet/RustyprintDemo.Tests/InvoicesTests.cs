using System.Net;
using Microsoft.Extensions.DependencyInjection;
using RustyprintDemo.Components.Documents;
using RustyprintDemo.Invoices;
using RustyprintDemo.Pdf;

namespace RustyprintDemo.Tests;

public class InvoicesTests(DemoApp app) : IClassFixture<DemoApp>
{
    private readonly HttpClient _client = app.CreateClient();
    private static CancellationToken Cancel => TestContext.Current.CancellationToken;

    [Fact]
    public async Task The_invoices_are_listed()
    {
        var html = await _client.GetStringAsync("/invoices", Cancel);
        foreach (var invoice in Invoice.All)
        {
            Assert.Contains($"href=\"/invoices/{invoice.Id}\"", html, StringComparison.Ordinal);
            Assert.Contains($"href=\"/invoices/{invoice.Id}.pdf\"", html, StringComparison.Ordinal);
        }
        Assert.Contains("InvoiceEndpoints", html, StringComparison.Ordinal);
    }

    [Fact]
    public async Task An_invoice_shows_as_html_on_its_own()
    {
        var html = await _client.GetStringAsync($"/invoices/{Invoice.All[0].Id}", Cancel);
        Assert.Contains("<h1>INVOICE</h1>", html, StringComparison.Ordinal);
        Assert.Contains("<link rel=\"stylesheet\" href=\"/invoice/invoice.css\"", html, StringComparison.Ordinal);
        Assert.Contains("<img src=\"/invoice/logo.png\"", html, StringComparison.Ordinal);
        Assert.DoesNotContain("site-header", html, StringComparison.Ordinal);
        Assert.Equal(HttpStatusCode.OK, (await _client.GetAsync("/invoice/invoice.css", Cancel)).StatusCode);
    }

    [Fact]
    public async Task An_invoice_renders_as_a_pdf_with_the_stylesheet_and_logo_from_the_static_files()
    {
        var invoice = Invoice.All[0];
        var response = await _client.GetAsync($"/invoices/{invoice.Id}.pdf", Cancel);
        Assert.Equal(HttpStatusCode.OK, response.StatusCode);
        Assert.Equal("application/pdf", response.Content.Headers.ContentType?.MediaType);
        Assert.Equal($"inline; filename=\"invoice {invoice.Id}.pdf\"; filename*=UTF-8''invoice%20{invoice.Id}.pdf",
            response.Content.Headers.GetValues("Content-Disposition").Single());
        var pdf = await response.Content.ReadAsByteArrayAsync(Cancel);
        Assert.True(Pdf.IsPdf(pdf));
        Assert.True(Pdf.HasImage(pdf), "the logo is in the PDF");
    }

    [Fact]
    public async Task The_pdf_view_finds_the_files_the_html_refers_to()
    {
        var views = app.Services.GetRequiredService<PdfViews>();
        var html = await views.RenderHtmlAsync<InvoiceDocument>(new() { ["Invoice"] = Invoice.All[0] });
        var assets = views.CollectAssets(html);
        Assert.Equal(["invoice/invoice.css", "invoice/logo.png"], assets.Select(asset => asset.Key).Order());

        var output = await views.RenderAsync(html, cancellationToken: Cancel);
        Assert.Empty(output.Warnings);
    }

    [Theory]
    [InlineData("/invoices/nope")]
    [InlineData("/invoices/nope.pdf")]
    public async Task An_unknown_invoice_is_not_found(string path) =>
        Assert.Equal(HttpStatusCode.NotFound, (await _client.GetAsync(path, Cancel)).StatusCode);
}
