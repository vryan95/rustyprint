using Microsoft.Net.Http.Headers;

namespace RustyprintDemo.Pdf;

// A PDF response, shown in the browser (inline) or downloaded, named `name` with .pdf added.
public sealed class PdfFile(byte[] pdf, string name, bool inline = true) : IResult
{
    public Task ExecuteAsync(HttpContext httpContext)
    {
        var disposition = new ContentDispositionHeaderValue(inline ? "inline" : "attachment");
        disposition.SetHttpFileName(name.EndsWith(".pdf", StringComparison.OrdinalIgnoreCase) ? name : $"{name}.pdf");
        var response = httpContext.Response;
        response.ContentType = "application/pdf";
        response.ContentLength = pdf.Length;
        response.Headers.ContentDisposition = disposition.ToString();
        return response.Body.WriteAsync(pdf).AsTask();
    }
}
