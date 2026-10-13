using Microsoft.AspNetCore.Http.HttpResults;
using RustyprintDemo.Components.Documents;
using RustyprintDemo.Components.Pages;
using RustyprintDemo.Pdf;

namespace RustyprintDemo.Invoices;

public static class InvoiceEndpoints
{
    public static void MapInvoices(this IEndpointRouteBuilder app)
    {
        app.MapGet("/invoices", (IWebHostEnvironment environment) => new RazorComponentResult<InvoicesIndex>(new
        {
            Invoices = Invoice.All,
            Source = File.ReadAllText(Path.Combine(environment.ContentRootPath, "Invoices", "InvoiceEndpoints.cs")),
        }));

        // The invoice document as HTML, as the PDF is made from it.
        app.MapGet("/invoices/{id}", IResult (string id) =>
            Invoice.Find(id) is { } invoice
                ? new RazorComponentResult<InvoiceDocument>(new { Invoice = invoice })
                : Results.NotFound());

        // The same document as a PDF. PdfViews renders the component to HTML, and gives rustyprint
        // the stylesheet and logo it links to from the app's static files.
        app.MapGet("/invoices/{id}.pdf", async Task<IResult> (string id, PdfViews views, CancellationToken cancel) =>
            Invoice.Find(id) is { } invoice
                ? await views.PdfAsync<InvoiceDocument>($"invoice {invoice.Id}", new() { ["Invoice"] = invoice },
                    cancellationToken: cancel)
                : Results.NotFound());
    }
}
