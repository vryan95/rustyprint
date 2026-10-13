using System.Diagnostics;
using System.Net;
using System.Text;
using System.Text.RegularExpressions;
using Rustyprint;

namespace RustyprintDemo.Samples;

// A sample document: one of the shared samples in demos/samples, an index.html with the
// stylesheets, images and fonts it refers to by relative URL beside it.
public sealed partial class Sample
{
    // What each sample shows off, in the order the home page lists them.
    private static readonly Sample[] Catalog =
    [
        new("invoice", "A three-page invoice with a logo, a long table and totals.",
            ["Logo image", "40-line table whose header row repeats on every page", "Page numbers in the page footer"]),
        new("report", "An eight-page quarterly report with a title page, a table of contents and charts.",
            ["Title page with its own page style", "Table of contents with page numbers and leaders",
             "Page headers and footers", "Charts as images, and tables"]),
        new("letter", "A one-page letter signed in a handwriting web font.",
            ["Web font loaded with @font-face (Caveat)"]),
    ];

    private Sample(string id, string summary, string[] features)
    {
        Id = id;
        Summary = summary;
        Features = features;
        Dir = Path.Combine(DemoPaths.Samples, id);
        AssetPaths = Directory.EnumerateFiles(Dir, "*", SearchOption.AllDirectories)
            .Select(path => Path.GetRelativePath(Dir, path).Replace(Path.DirectorySeparatorChar, '/'))
            .Where(path => path != "index.html")
            .Order(StringComparer.Ordinal)
            .ToArray();
    }

    public static IReadOnlyList<Sample> All => Catalog;

    public static Sample? Find(string id) => Catalog.FirstOrDefault(sample => sample.Id == id);

    public string Id { get; }

    public string Summary { get; }

    public IReadOnlyList<string> Features { get; }

    public string Dir { get; }

    // The files the document uses, by the relative path it uses for each: every file beside it.
    public IReadOnlyList<string> AssetPaths { get; }

    public string Html => File.ReadAllText(Path.Combine(Dir, "index.html"));

    public string Title => WebUtility.HtmlDecode(TitleTag().Match(Html).Groups[1].Value.Trim());

    // An asset the document refers to by a relative path, or null for anything else.
    public string? Asset(string path) => AssetPaths.Contains(path) ? Path.Combine(Dir, path) : null;

    public RenderOptions Options()
    {
        var options = new RenderOptions();
        foreach (var path in AssetPaths)
        {
            options.Assets.AddFile(path, Path.Combine(Dir, path));
        }
        return options;
    }

    public async Task<Rendering> RenderAsync(CancellationToken cancellationToken = default)
    {
        var started = Stopwatch.GetTimestamp();
        var output = await PdfRenderer.RenderAsync(Html, Options(), cancellationToken);
        return new Rendering(output.Pdf, output.Warnings, Stopwatch.GetElapsedTime(started));
    }

    [GeneratedRegex("<title>(.*?)</title>", RegexOptions.Singleline)]
    private static partial Regex TitleTag();
}

// A sample rendered to PDF, with what it took.
public sealed partial record Rendering(byte[] Pdf, IReadOnlyList<string> Warnings, TimeSpan Elapsed)
{
    public int Pages => MediaBox().Count(Encoding.Latin1.GetString(Pdf));

    public int Bytes => Pdf.Length;

    [GeneratedRegex(@"/MediaBox \[")]
    private static partial Regex MediaBox();
}
