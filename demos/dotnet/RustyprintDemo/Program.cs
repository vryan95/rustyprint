using RustyprintDemo;
using RustyprintDemo.Invoices;
using RustyprintDemo.Pdf;
using RustyprintDemo.Playground;
using RustyprintDemo.Samples;

if (args is ["render-samples", .. var rest])
{
    return await SamplesCommand.RunAsync(rest, Console.Out);
}

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddRazorComponents();
builder.Services.AddSingleton<StaticFileMounts>();
builder.Services.AddSingleton<PdfViews>();

var app = builder.Build();
foreach (var (requestPath, files) in app.Services.GetRequiredService<StaticFileMounts>().Mounts)
{
    app.UseStaticFiles(new StaticFileOptions { RequestPath = requestPath, FileProvider = files });
}
app.UseAntiforgery();

app.MapGet("/up", () => Results.Ok());
app.MapSamples();
app.MapInvoices();
app.MapPlayground();

await app.RunAsync();
return 0;

// For the tests' WebApplicationFactory.
public partial class Program;
