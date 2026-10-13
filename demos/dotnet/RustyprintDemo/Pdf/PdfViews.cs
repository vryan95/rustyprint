using Microsoft.AspNetCore.Components;
using Microsoft.AspNetCore.Components.Web;
using Rustyprint;

namespace RustyprintDemo.Pdf;

// Renders a Razor component to PDF, as Rails' `render pdf:` does a view:
//
//   app.MapGet("/invoices/{id}.pdf", (string id, PdfViews views) =>
//       views.PdfAsync<InvoiceDocument>($"invoice {id}", new() { ["Invoice"] = Invoice.Find(id) }));
//
// The component renders to HTML on its own, without the site's layout. The stylesheets, images
// and fonts the HTML refers to by path, such as /invoice/invoice.css, are read from the app's
// static files, and the warnings are logged.
public sealed class PdfViews(IServiceScopeFactory scopes, ILoggerFactory loggers, StaticFileMounts files, ILogger<PdfViews> logger)
{
    public async Task<string> RenderHtmlAsync<TComponent>(Dictionary<string, object?> parameters) where TComponent : IComponent
    {
        // Components can use scoped services, so each render has a scope of its own.
        await using var scope = scopes.CreateAsyncScope();
        await using var renderer = new HtmlRenderer(scope.ServiceProvider, loggers);
        return await renderer.Dispatcher.InvokeAsync(async () =>
            (await renderer.RenderComponentAsync<TComponent>(ParameterView.FromDictionary(parameters))).ToHtmlString());
    }

    // The static files the HTML and its stylesheets refer to, by the path the HTML uses.
    public IReadOnlyDictionary<string, byte[]> CollectAssets(string html) => new AssetCollector(files.Read).Collect(html);

    // Renders HTML with the files it refers to, besides the assets already in the options.
    public async Task<RenderOutput> RenderAsync(string html, RenderOptions? options = null, CancellationToken cancellationToken = default)
    {
        options ??= new RenderOptions();
        foreach (var (path, data) in CollectAssets(html))
        {
            if (!options.Assets.Contains(path)) options.Assets[path] = data;
        }
        var output = await PdfRenderer.RenderAsync(html, options, cancellationToken);
        foreach (var warning in output.Warnings)
        {
            logger.LogWarning("rustyprint: {Warning}", warning);
        }
        return output;
    }

    // The component as a PDF response, named `name` with .pdf added, shown in the browser or,
    // unless `inline`, downloaded.
    public async Task<IResult> PdfAsync<TComponent>(string name, Dictionary<string, object?> parameters, RenderOptions? options = null,
        bool inline = true, CancellationToken cancellationToken = default) where TComponent : IComponent
    {
        var html = await RenderHtmlAsync<TComponent>(parameters);
        var output = await RenderAsync(html, options, cancellationToken);
        return new PdfFile(output.Pdf, name, inline);
    }
}
