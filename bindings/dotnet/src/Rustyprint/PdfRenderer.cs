using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Rustyprint;

/// <summary>Renders HTML and CSS to PDF.</summary>
/// <example>
/// <code>
/// var options = new RenderOptions { PageSize = PageSize.A4, Margin = "15mm" };
/// options.Assets.AddFile("logo.png", "logo.png");
/// var output = await PdfRenderer.RenderAsync(html, options);
/// File.WriteAllBytes("invoice.pdf", output.Pdf);
/// </code>
/// </example>
public static class PdfRenderer
{
    /// <summary>The package's version, which every rustyprint package shares, such as <c>1.2.0</c>.</summary>
    public static string Version { get; } =
        typeof(PdfRenderer).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()!.InformationalVersion;

    /// <summary>The native library's version, the same as <see cref="Version"/>.</summary>
    public static string NativeVersion => Marshal.PtrToStringUTF8(Native.rp_version())!;

    /// <summary>
    /// Renders an HTML document to PDF. Only the files in <see cref="RenderOptions.Assets"/> are
    /// read, and nothing is fetched from the network unless <see cref="RenderOptions.Remote"/>
    /// allows the host. A render runs on a thread of its own, so it can be called from any thread.
    /// </summary>
    /// <exception cref="ArgumentNullException"><paramref name="html"/> is null.</exception>
    /// <exception cref="ArgumentException"><paramref name="html"/> is not valid UTF-16.</exception>
    /// <exception cref="RustyprintException">The render failed: see the subclasses.</exception>
    public static RenderOutput Render(string html, RenderOptions? options = null) => Prepare(html, options).Render();

    /// <summary>
    /// Renders an HTML document to PDF on the thread pool, as <see cref="Render"/> does, with the
    /// options and assets as they are when it is called. Cancelling stops a render that has not
    /// started; to bound a render that has, set <see cref="RenderLimits.Timeout"/>.
    /// </summary>
    /// <inheritdoc cref="Render" path="/exception"/>
    public static Task<RenderOutput> RenderAsync(string html, RenderOptions? options = null, CancellationToken cancellationToken = default)
    {
        var request = Prepare(html, options);
        return Task.Run(request.Render, cancellationToken);
    }

    // The HTML, options and assets as the native library takes them, copied from the caller's.
    private static Request Prepare(string html, RenderOptions? options)
    {
        var bytes = Utf8.Bytes(html ?? throw new ArgumentNullException(nameof(html)), nameof(html));
        options ??= new RenderOptions();
        var json = Utf8.Bytes(options.ToJson(), nameof(options));
        var assets = options.Assets.Select(asset => (Utf8.NulTerminated(asset.Key, "asset path"), asset.Value)).ToArray();
        return new Request(bytes, json, assets);
    }

    private sealed class Request(byte[] html, byte[] optionsJson, (byte[] Path, byte[] Data)[] assets)
    {
        public RenderOutput Render()
        {
            using var request = Native.rp_request_new(optionsJson, (UIntPtr)optionsJson.Length);
            if (request.IsInvalid)
            {
                throw new InternalRenderException("The native library could not make a request.");
            }
            foreach (var (path, data) in assets)
            {
                if (Native.rp_request_add_asset(request, path, data, (UIntPtr)data.Length) != Native.Status.Ok)
                {
                    throw new ArgumentException($"Invalid asset path {Utf8.String(path).TrimEnd('\0')}.");
                }
            }

            var status = Native.rp_render(request, html, (UIntPtr)html.Length, out var result);
            using (result)
            {
                if (result.IsInvalid)
                {
                    throw new InternalRenderException("The native library could not make a result.");
                }
                if (status != Native.Status.Ok)
                {
                    throw Error(status, Utf8.String(Native.Copy(Native.rp_result_error(result, out var len), len)));
                }

                var pdf = Native.Copy(Native.rp_result_pdf(result, out var pdfLen), pdfLen);
                var warnings = Utf8.String(Native.Copy(Native.rp_result_warnings_json(result, out var warningsLen), warningsLen));
                return new RenderOutput(pdf, Warnings(warnings));
            }
        }

        private static string[] Warnings(string json)
        {
            using var document = JsonDocument.Parse(json);
            return document.RootElement.EnumerateArray().Select(warning => warning.GetString()!).ToArray();
        }

        private static Exception Error(Native.Status status, string message) => status switch
        {
            Native.Status.InvalidOptions => new InvalidOptionsException(message),
            Native.Status.LimitExceeded => new LimitExceededException(message),
            Native.Status.Timeout => new RenderTimeoutException(message),
            // The HTML and the asset paths are checked here, so the library never sees invalid ones.
            _ => new InternalRenderException(message),
        };
    }
}
