using Microsoft.Extensions.FileProviders;

namespace RustyprintDemo;

// The app's static files: wwwroot, and the shared invoice sample's stylesheet and logo under
// /invoice. The PDF views read the files the HTML refers to from the same places a browser
// would get them.
public sealed class StaticFileMounts(IWebHostEnvironment environment)
{
    public IReadOnlyList<(string RequestPath, IFileProvider Files)> Mounts { get; } =
    [
        ("/invoice", new PhysicalFileProvider(Path.Combine(DemoPaths.Samples, "invoice"))),
        ("", environment.WebRootFileProvider),
    ];

    // The file at a root-relative path, such as /invoice/logo.png, or null if there is none.
    public byte[]? Read(string path)
    {
        foreach (var (requestPath, files) in Mounts)
        {
            if (!path.StartsWith(requestPath + "/", StringComparison.Ordinal)) continue;

            var file = files.GetFileInfo(path[requestPath.Length..]);
            if (file.Exists && !file.IsDirectory)
            {
                using var stream = file.CreateReadStream();
                using var copy = new MemoryStream();
                stream.CopyTo(copy);
                return copy.ToArray();
            }
        }
        return null;
    }
}
