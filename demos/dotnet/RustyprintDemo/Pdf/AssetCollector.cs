using System.Net;
using System.Text;
using System.Text.RegularExpressions;

namespace RustyprintDemo.Pdf;

// Finds the files an HTML document refers to, in its tags, its style attributes and blocks, and
// the stylesheets it loads, and reads each once, for RenderOptions.Assets.
//
// The read function gets a root-relative path, such as /css/app.css, decoded, and returns the file's
// contents, or null if there is none. The result maps the path the document uses, without its
// leading slash, to the contents. Remote and data: URLs are left to the renderer.
public sealed partial class AssetCollector(Func<string, byte[]?> read)
{
    private static readonly string[] UrlAttributes = ["src", "poster", "data"];

    private readonly Dictionary<string, byte[]> _assets = new(StringComparer.Ordinal);
    private readonly HashSet<string> _seen = new(StringComparer.Ordinal);

    // The files the document refers to, by the path it uses for each.
    public IReadOnlyDictionary<string, byte[]> Collect(string html)
    {
        _assets.Clear();
        _seen.Clear();
        foreach (Match block in StyleBlock().Matches(html)) CollectCss(block.Groups[1].Value);
        foreach (Match tag in Tag().Matches(html)) CollectTag(tag.Groups[1].Value.ToLowerInvariant(), tag.Groups[2].Value);
        return new Dictionary<string, byte[]>(_assets, StringComparer.Ordinal);
    }

    private void CollectTag(string name, string text)
    {
        var attributes = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (Match attribute in Attribute().Matches(text))
        {
            attributes.TryAdd(attribute.Groups[1].Value.ToLowerInvariant(), WebUtility.HtmlDecode(attribute.Groups[2].Value.Trim('"', '\'')));
        }
        foreach (var key in UrlAttributes)
        {
            if (attributes.TryGetValue(key, out var url)) Add(url);
        }
        if (attributes.TryGetValue("srcset", out var srcset))
        {
            foreach (var candidate in srcset.Split(',')) Add(candidate.Trim().Split(' ')[0]);
        }
        if (attributes.TryGetValue("style", out var style)) CollectCss(style);
        if (attributes.TryGetValue("href", out var href))
        {
            if (name == "link" && attributes.TryGetValue("rel", out var rel) && rel.Split(' ', StringSplitOptions.RemoveEmptyEntries).Contains("stylesheet", StringComparer.OrdinalIgnoreCase))
            {
                Add(href, stylesheet: true);
            }
            else if (name is "image" or "use")
            {
                Add(href);
            }
        }
    }

    private void CollectCss(string css)
    {
        foreach (Match url in CssUrl().Matches(CssComment().Replace(css, "")))
        {
            var import = url.Groups["import"].Success;
            Add(url.Groups[import ? "import" : "url"].Value, stylesheet: import);
        }
    }

    private void Add(string url, bool stylesheet = false)
    {
        if (AssetPath(url) is not { } path || !_seen.Add(path)) return;

        var key = Uri.UnescapeDataString(path);
        if (read("/" + key) is not { } data) return;

        _assets[key] = data;
        if (stylesheet || key.EndsWith(".css", StringComparison.OrdinalIgnoreCase))
        {
            CollectCss(Encoding.UTF8.GetString(data));
        }
    }

    // The URL's path from the document's root, as the renderer resolves it, or null for remote,
    // data: and fragment-only URLs and paths that leave the root.
    private static string? AssetPath(string url)
    {
        url = url.Trim().Split('?', '#')[0];
        if (url.Length == 0 || url.StartsWith("//", StringComparison.Ordinal) || url.StartsWith('\\') || Scheme().IsMatch(url)) return null;

        var segments = url.Split('/').Where(segment => segment.Length > 0 && segment != ".").ToArray();
        return segments.Length == 0 || segments.Contains("..") ? null : string.Join('/', segments);
    }

    [GeneratedRegex(@"<([a-zA-Z][\w-]*)((?:\s+[^\s=>/]+(?:\s*=\s*(?:""[^""]*""|'[^']*'|[^\s>]+))?)*)\s*/?>")]
    private static partial Regex Tag();

    [GeneratedRegex(@"([^\s=>/]+)(?:\s*=\s*(""[^""]*""|'[^']*'|[^\s>]+))?")]
    private static partial Regex Attribute();

    [GeneratedRegex(@"<style\b[^>]*>(.*?)</style>", RegexOptions.Singleline | RegexOptions.IgnoreCase)]
    private static partial Regex StyleBlock();

    [GeneratedRegex(@"/\*.*?\*/", RegexOptions.Singleline)]
    private static partial Regex CssComment();

    [GeneratedRegex(@"url\(\s*(?:""(?<url>[^""]*)""|'(?<url>[^']*)'|(?<url>[^)\s]*))\s*\)|@import\s+(?:""(?<import>[^""]*)""|'(?<import>[^']*)')", RegexOptions.IgnoreCase)]
    private static partial Regex CssUrl();

    [GeneratedRegex(@"^[a-z][a-z\d+.-]*:", RegexOptions.IgnoreCase)]
    private static partial Regex Scheme();
}
