namespace RustyprintDemo.Samples;

// `dotnet run -- render-samples [directory]`: renders every sample to a PDF, in the directory
// given or out/samples.
public static class SamplesCommand
{
    public static async Task<int> RunAsync(string[] args, TextWriter output, CancellationToken cancellationToken = default)
    {
        var dir = args.FirstOrDefault() ?? Path.Combine("out", "samples");
        Directory.CreateDirectory(dir);
        foreach (var sample in Sample.All)
        {
            var rendering = await sample.RenderAsync(cancellationToken);
            var file = $"{sample.Id}.pdf";
            await File.WriteAllBytesAsync(Path.Combine(dir, file), rendering.Pdf, cancellationToken);
            await output.WriteLineAsync(
                $"{file,-14} {rendering.Pages,2} {(rendering.Pages == 1 ? "page" : "pages"),-5} {rendering.Elapsed.TotalMilliseconds,5:0} ms  {Format.Size(rendering.Bytes)}");
            foreach (var warning in rendering.Warnings)
            {
                await output.WriteLineAsync($"  warning: {warning}");
            }
        }
        return 0;
    }
}
