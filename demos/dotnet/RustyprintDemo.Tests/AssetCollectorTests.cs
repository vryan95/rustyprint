using RustyprintDemo.Pdf;

namespace RustyprintDemo.Tests;

public class AssetCollectorTests
{
    private static readonly Dictionary<string, string> Files = new()
    {
        ["/css/app.css"] = "@import \"/css/base.css\"; h1 { background: url(\"/images/bg.png\") }",
        ["/css/base.css"] = "@font-face { font-family: X; src: url(/fonts/x.ttf) format('truetype') }",
        ["/images/bg.png"] = "png",
        ["/fonts/x.ttf"] = "ttf",
        ["/images/logo.png"] = "logo",
        ["/images/photo.jpg"] = "jpg",
        ["/hero.png"] = "hero",
        ["/my logo.png"] = "spaced",
    };

    private readonly List<string> _requested = [];

    private Dictionary<string, string> Collect(string html) =>
        new AssetCollector(path =>
        {
            _requested.Add(path);
            return Files.TryGetValue(path, out var text) ? System.Text.Encoding.UTF8.GetBytes(text) : null;
        })
        .Collect(html)
        .ToDictionary(asset => asset.Key, asset => System.Text.Encoding.UTF8.GetString(asset.Value));

    [Fact]
    public void Collects_what_the_html_and_its_stylesheets_refer_to_by_the_path_the_document_uses()
    {
        var assets = Collect("""
            <link rel="stylesheet" href="/css/app.css?v=1">
            <img src='/images/logo.png' alt="">
            <img srcset="/images/photo.jpg 1x, /images/photo.jpg 2x">
            <div style="background: url(&quot;hero.png&quot;)"></div>
            <img src="/my%20logo.png">
            """);
        Assert.Equal(new Dictionary<string, string>
        {
            ["css/app.css"] = Files["/css/app.css"],
            ["css/base.css"] = Files["/css/base.css"],
            ["images/bg.png"] = "png",
            ["fonts/x.ttf"] = "ttf",
            ["images/logo.png"] = "logo",
            ["images/photo.jpg"] = "jpg",
            ["hero.png"] = "hero",
            ["my logo.png"] = "spaced",
        }, assets);
    }

    [Fact]
    public void Fetches_each_file_once()
    {
        Collect("""<img src="/hero.png"><img src="hero.png"><img src="./hero.png#x">""");
        Assert.Equal(["/hero.png"], _requested);
    }

    [Fact]
    public void Leaves_remote_data_fragment_and_escaping_urls_to_the_renderer()
    {
        var assets = Collect("""
            <img src="https://cdn.example.com/a.png"><img src="//cdn.example.com/b.png">
            <img src="data:image/png;base64,AAAA"><a href="#top">top</a><a href="mailto:x@example.com">x</a>
            <img src="/../secret.png"><img src="/">
            """);
        Assert.Empty(assets);
        Assert.Empty(_requested);
    }

    [Fact]
    public void Skips_files_that_cannot_be_found() =>
        Assert.Equal(["hero.png"], Collect("""<img src="/missing.png"><img src="/hero.png">""").Keys);

    [Fact]
    public void Skips_links_that_are_not_resources()
    {
        Assert.Empty(Collect("""<a href="/invoices/1">Invoice</a><link rel="canonical" href="/hero.png">"""));
        Assert.Empty(_requested);
    }
}
