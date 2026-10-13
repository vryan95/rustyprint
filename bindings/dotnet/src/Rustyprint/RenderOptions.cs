using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Threading;

namespace Rustyprint;

/// <summary>
/// How to render a document: the options of <c>schema/render-options.schema.json</c>, and the
/// files the document refers to. An option left unset keeps its default.
/// </summary>
/// <example>
/// <code>
/// var options = new RenderOptions { PageSize = PageSize.A4, Margin = "15mm" };
/// options.Assets.AddFile("logo.png", "wwwroot/logo.png");
/// </code>
/// </example>
public sealed class RenderOptions
{
    /// <summary>The page size. Default: A4.</summary>
    public PageSize? PageSize { get; set; }

    /// <summary>Turns the page so that it is wider than it is tall. Default: false.</summary>
    public bool? Landscape { get; set; }

    /// <summary>
    /// Page margins like the CSS <c>margin</c> shorthand: one to four non-negative lengths for
    /// the top, right, bottom and left, such as <c>15mm</c> or <c>1cm 2cm</c>. Default: 2cm.
    /// </summary>
    public string? Margin { get; set; }

    /// <summary>The PDF document title. Overrides the document's <c>&lt;title&gt;</c>.</summary>
    public string? Title { get; set; }

    /// <summary>A stylesheet applied after the document's own styles.</summary>
    public string? Stylesheet { get; set; }

    /// <summary>
    /// Hosts that absolute URLs may be fetched from. Without it, nothing is fetched from the
    /// network.
    /// </summary>
    public RemoteOptions? Remote { get; set; }

    /// <summary>
    /// Bounds on the work of the render. Exceeding one fails the render, except that larger
    /// images are skipped with a warning.
    /// </summary>
    public RenderLimits? Limits { get; set; }

    /// <summary>
    /// The files the document refers to, such as stylesheets, images and fonts, by the path the
    /// document uses for each. Nothing else is read.
    /// </summary>
    public AssetCollection Assets { get; } = new();

    /// <summary>The options as JSON, in the format of the schema, without the assets.</summary>
    public string ToJson()
    {
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping }))
        {
            json.WriteStartObject();
            if (PageSize is { } size)
            {
                if (size.Name is { } name)
                {
                    json.WriteString("page_size", name);
                }
                else
                {
                    json.WriteStartObject("page_size");
                    json.WriteString("width", size.Width);
                    json.WriteString("height", size.Height);
                    json.WriteEndObject();
                }
            }
            if (Landscape is { } landscape) json.WriteBoolean("landscape", landscape);
            if (Margin is not null) json.WriteString("margin", Margin);
            if (Title is not null) json.WriteString("title", Title);
            if (Stylesheet is not null) json.WriteString("stylesheet", Stylesheet);
            if (Remote is { } remote)
            {
                json.WriteStartObject("remote");
                json.WriteStartArray("allowed_hosts");
                foreach (var host in remote.AllowedHosts) json.WriteStringValue(host);
                json.WriteEndArray();
                if (remote.AllowHttp is { } allowHttp) json.WriteBoolean("allow_http", allowHttp);
                if (remote.Timeout is { } timeout) WriteMilliseconds(json, "timeout_ms", timeout);
                if (remote.MaxBytes is { } maxBytes) json.WriteNumber("max_bytes", maxBytes);
                if (remote.MaxRedirects is { } maxRedirects) json.WriteNumber("max_redirects", maxRedirects);
                if (remote.MaxFetches is { } maxFetches) json.WriteNumber("max_fetches", maxFetches);
                json.WriteEndObject();
            }
            if (Limits is { } limits)
            {
                json.WriteStartObject("limits");
                if (limits.MaxInputBytes is { } maxInputBytes) json.WriteNumber("max_input_bytes", maxInputBytes);
                if (limits.MaxNodes is { } maxNodes) json.WriteNumber("max_nodes", maxNodes);
                if (limits.MaxDepth is { } maxDepth) json.WriteNumber("max_depth", maxDepth);
                if (limits.MaxPages is { } maxPages) json.WriteNumber("max_pages", maxPages);
                if (limits.MaxImagePixels is { } maxImagePixels) json.WriteNumber("max_image_pixels", maxImagePixels);
                if (limits.Timeout is { } timeout) WriteMilliseconds(json, "timeout_ms", timeout);
                json.WriteEndObject();
            }
            json.WriteEndObject();
        }
        return System.Text.Encoding.UTF8.GetString(stream.ToArray());
    }

    // Whole milliseconds, or null for Timeout.InfiniteTimeSpan.
    private static void WriteMilliseconds(Utf8JsonWriter json, string name, TimeSpan timeout)
    {
        if (timeout == Timeout.InfiniteTimeSpan)
        {
            json.WriteNull(name);
        }
        else
        {
            json.WriteNumber(name, timeout.Ticks / TimeSpan.TicksPerMillisecond);
        }
    }
}

/// <summary>A page size: by name, such as <see cref="A4"/>, or as a width and height.</summary>
public sealed class PageSize
{
    private PageSize(string? name, string? width, string? height)
    {
        Name = name;
        Width = width;
        Height = height;
    }

    /// <summary>A page size of this width and height, as CSS lengths such as <c>100mm</c>.</summary>
    public PageSize(string width, string height)
        : this(null, width ?? throw new ArgumentNullException(nameof(width)), height ?? throw new ArgumentNullException(nameof(height)))
    {
    }

    /// <summary>A3, 297 × 420 mm.</summary>
    public static PageSize A3 { get; } = Named("A3");

    /// <summary>A4, 210 × 297 mm.</summary>
    public static PageSize A4 { get; } = Named("A4");

    /// <summary>A5, 148 × 210 mm.</summary>
    public static PageSize A5 { get; } = Named("A5");

    /// <summary>US Letter, 8.5 × 11 in.</summary>
    public static PageSize Letter { get; } = Named("Letter");

    /// <summary>US Legal, 8.5 × 14 in.</summary>
    public static PageSize Legal { get; } = Named("Legal");

    /// <summary>The name of the size, such as <c>A4</c>, or null for a width and height.</summary>
    public string? Name { get; }

    /// <summary>The width as a CSS length, or null for a named size.</summary>
    public string? Width { get; }

    /// <summary>The height as a CSS length, or null for a named size.</summary>
    public string? Height { get; }

    /// <summary>
    /// A page size by name: A3, A4, A5, Letter or Legal, in any case. Rendering with another name
    /// throws <see cref="InvalidOptionsException"/>.
    /// </summary>
    public static PageSize Named(string name) => new(name ?? throw new ArgumentNullException(nameof(name)), null, null);

    /// <inheritdoc/>
    public override string ToString() => Name ?? $"{Width} × {Height}";
}

/// <summary>Where absolute URLs may be fetched from, and the bounds on each fetch.</summary>
public sealed class RemoteOptions
{
    /// <summary>Allows fetching from these hosts.</summary>
    public RemoteOptions(params string[] allowedHosts)
    {
        AllowedHosts = new List<string>(allowedHosts ?? throw new ArgumentNullException(nameof(allowedHosts)));
    }

    /// <summary>
    /// Host names, such as <c>cdn.example.com</c>, or <c>*.example.com</c> for its subdomains.
    /// Redirects must stay on them.
    /// </summary>
    public IList<string> AllowedHosts { get; }

    /// <summary>Also fetch plain http URLs. Default: false.</summary>
    public bool? AllowHttp { get; set; }

    /// <summary>How long one fetch may take, in whole milliseconds. Default: 10 seconds.</summary>
    public TimeSpan? Timeout { get; set; }

    /// <summary>The largest file fetched. Default: 10485760 (10 MB).</summary>
    public long? MaxBytes { get; set; }

    /// <summary>Redirects followed per fetch. Default: 5.</summary>
    public int? MaxRedirects { get; set; }

    /// <summary>Files fetched per render; the rest are not loaded. Default: 100.</summary>
    public int? MaxFetches { get; set; }
}

/// <summary>Bounds on the work of a render.</summary>
public sealed class RenderLimits
{
    /// <summary>Size of the HTML in bytes. Default: 20971520 (20 MB).</summary>
    public long? MaxInputBytes { get; set; }

    /// <summary>
    /// DOM nodes, including text and comments, across the document and its iframe documents.
    /// Default: 500000.
    /// </summary>
    public long? MaxNodes { get; set; }

    /// <summary>How deeply elements nest; 1 for the root element alone. Default: 512.</summary>
    public int? MaxDepth { get; set; }

    /// <summary>Pages in the PDF. Default: 2000.</summary>
    public int? MaxPages { get; set; }

    /// <summary>
    /// Width times height of an image; larger images are skipped with a warning.
    /// Default: 100000000.
    /// </summary>
    public long? MaxImagePixels { get; set; }

    /// <summary>
    /// How long the render may take, in whole milliseconds, or
    /// <see cref="System.Threading.Timeout.InfiniteTimeSpan"/> for no deadline. Default: 60 seconds.
    /// </summary>
    public TimeSpan? Timeout { get; set; }
}
