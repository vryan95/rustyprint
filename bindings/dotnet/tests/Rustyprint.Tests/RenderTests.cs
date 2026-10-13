using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Xunit;

namespace Rustyprint.Tests;

public class RenderTests
{
    [Fact]
    public void Renders_html_to_a_pdf()
    {
        var output = PdfRenderer.Render("<h1>Hello</h1>");
        Assert.True(Repo.IsPdf(output.Pdf));
        Assert.Empty(output.Warnings);
        // A4 with the default options.
        Assert.Equal([(595.3, 841.9)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Takes_typed_options()
    {
        var output = PdfRenderer.Render("<p>Hello</p>", new RenderOptions
        {
            PageSize = PageSize.A5,
            Landscape = true,
            Margin = "1cm",
            Title = "Hello",
        });
        Assert.Equal([(595.3, 419.5)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Takes_a_page_size_as_lengths_and_limits()
    {
        var output = PdfRenderer.Render("<p>x</p>", new RenderOptions
        {
            PageSize = new PageSize("100mm", "150mm"),
            Limits = new RenderLimits { MaxPages = 5, Timeout = Timeout.InfiniteTimeSpan },
        });
        Assert.Equal([(283.5, 425.2)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Names_page_sizes_in_any_case()
    {
        var output = PdfRenderer.Render("<p>x</p>", new RenderOptions { PageSize = PageSize.Named("letter") });
        Assert.Equal([(612.0, 792.0)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Uses_assets_by_the_path_the_document_gives()
    {
        var options = new RenderOptions { Assets = { ["css/page.css"] = Encoding.UTF8.GetBytes("@page { size: 300px 200px }") } };
        var output = PdfRenderer.Render("<link rel=\"stylesheet\" href=\"css/page.css\"><p>x</p>", options);
        Assert.Equal([(225.0, 150.0)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Reads_assets_from_text_files_streams_and_directories()
    {
        var dir = Directory.CreateTempSubdirectory("rustyprint-");
        try
        {
            var css = Path.Combine(dir.FullName, "css", "page.css");
            Directory.CreateDirectory(Path.GetDirectoryName(css)!);
            File.WriteAllText(css, "@page { size: 400px 400px }");
            const string html = "<link rel=\"stylesheet\" href=\"css/page.css\">";

            var fromText = new RenderOptions();
            fromText.Assets.Add("css/page.css", "@page { size: 400px 400px }");
            var fromFile = new RenderOptions();
            fromFile.Assets.AddFile("css/page.css", css);
            var fromStream = new RenderOptions();
            using (var stream = File.OpenRead(css))
            {
                fromStream.Assets.Add("css/page.css", stream);
            }
            var fromDirectory = new RenderOptions();
            fromDirectory.Assets.AddDirectory(dir.FullName);

            Assert.Equal(["css/page.css"], fromDirectory.Assets.Select(asset => asset.Key));
            foreach (var options in new[] { fromText, fromFile, fromStream, fromDirectory })
            {
                Assert.Equal([(300.0, 300.0)], Repo.PageSizes(PdfRenderer.Render(html, options).Pdf));
            }
        }
        finally
        {
            dir.Delete(recursive: true);
        }
    }

    [Fact]
    public void An_asset_added_again_replaces_the_first()
    {
        var options = new RenderOptions();
        options.Assets.Add("page.css", "@page { size: 300px 200px }");
        options.Assets.Add("page.css", "@page { size: 400px 400px }");
        Assert.Equal(1, options.Assets.Count);
        var output = PdfRenderer.Render("<link rel=\"stylesheet\" href=\"page.css\">", options);
        Assert.Equal([(300.0, 300.0)], Repo.PageSizes(output.Pdf));
    }

    [Fact]
    public void Returns_the_warnings()
    {
        var output = PdfRenderer.Render("<style>p { colör: red; colr: blue }</style><p>x</p>");
        Assert.Equal(2, output.Warnings.Count);
        Assert.Contains("colör", string.Join("\n", output.Warnings), StringComparison.Ordinal);
    }

    [Fact]
    public void Refuses_text_that_is_not_valid_utf16()
    {
        var html = Assert.Throws<ArgumentException>(() => PdfRenderer.Render("<p>\ud800</p>"));
        Assert.Equal("html", html.ParamName);
        Assert.Throws<ArgumentException>(() => new RenderOptions().Assets.Add("logo\ud800.png", []));
        Assert.Throws<ArgumentException>(() => new RenderOptions().Assets.Add("logo\0.png", []));
        Assert.Throws<ArgumentNullException>(() => PdfRenderer.Render(null!));
    }

    [Fact]
    public void Gives_the_same_bytes_for_the_same_input_on_any_thread()
    {
        var dir = Repo.PathTo("demos/samples/invoice");
        var html = File.ReadAllText(Path.Combine(dir, "index.html"));
        var options = new RenderOptions();
        options.Assets.AddFile("invoice.css", Path.Combine(dir, "invoice.css"));
        options.Assets.AddFile("logo.png", Path.Combine(dir, "logo.png"));

        var first = PdfRenderer.Render(html, options).Pdf;
        var again = PdfRenderer.Render(html, options).Pdf;
        var threaded = Enumerable.Range(0, 4)
            .Select(_ => Task.Run(() => PdfRenderer.Render(html, options).Pdf))
            .Select(task => task.Result)
            .ToArray();
        Assert.All(threaded.Prepend(again), pdf => Assert.Equal(first, pdf));
    }

    [Fact]
    public void Renders_on_several_threads_at_once()
    {
        var outputs = new RenderOutput[4];
        var threads = Enumerable.Range(0, 4)
            .Select(i => new Thread(() => outputs[i] = PdfRenderer.Render($"<p>{i}</p>", new RenderOptions { PageSize = PageSize.A5 })))
            .ToArray();
        foreach (var thread in threads) thread.Start();
        foreach (var thread in threads) thread.Join();
        Assert.All(outputs, output => Assert.Equal([(419.5, 595.3)], Repo.PageSizes(output.Pdf)));
    }

    [Fact]
    public void Renders_deeply_nested_documents_on_a_thread_with_a_small_stack()
    {
        // rustyprint renders on a thread of its own, so the caller's stack size does not matter.
        var html = string.Concat(Enumerable.Repeat("<div>", 300)) + "x" + string.Concat(Enumerable.Repeat("</div>", 300));
        RenderOutput? output = null;
        var thread = new Thread(() => output = PdfRenderer.Render(html), maxStackSize: 256 * 1024);
        thread.Start();
        thread.Join();
        Assert.True(Repo.IsPdf(output!.Pdf));
    }
}

public class RenderAsyncTests
{
    private static readonly string LongDocument =
        string.Concat(Enumerable.Repeat("<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>", 3_000));

    [Fact]
    public async Task Gives_what_render_gives()
    {
        var options = new RenderOptions { PageSize = PageSize.Letter, Title = "Async" };
        var output = await PdfRenderer.RenderAsync("<h1>Hello</h1>", options, TestContext.Current.CancellationToken);
        Assert.Equal(PdfRenderer.Render("<h1>Hello</h1>", options).Pdf, output.Pdf);
    }

    [Fact]
    public async Task Returns_before_the_render_finishes()
    {
        var task = PdfRenderer.RenderAsync(LongDocument, cancellationToken: TestContext.Current.CancellationToken);
        var completedOnReturn = task.IsCompleted;
        var output = await task;
        Assert.False(completedOnReturn);
        Assert.True(Repo.IsPdf(output.Pdf));
    }

    [Fact]
    public async Task Uses_the_options_as_they_were_when_called()
    {
        var options = new RenderOptions { PageSize = PageSize.A5 };
        var task = PdfRenderer.RenderAsync(LongDocument, options, TestContext.Current.CancellationToken);
        options.PageSize = PageSize.A3;
        options.Assets.Add("late.css", "@page { size: 100px 100px }");
        var output = await task;
        Assert.All(Repo.PageSizes(output.Pdf), size => Assert.Equal((419.5, 595.3), size));
    }

    [Fact]
    public async Task Does_not_start_when_already_cancelled()
    {
        using var cancelled = new CancellationTokenSource();
        await cancelled.CancelAsync();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => PdfRenderer.RenderAsync("<p>x</p>", null, cancelled.Token));
    }

    [Fact]
    public async Task Throws_the_same_exceptions()
    {
        var options = new RenderOptions { PageSize = PageSize.Named("B99") };
        await Assert.ThrowsAsync<InvalidOptionsException>(() => PdfRenderer.RenderAsync("<p>x</p>", options, TestContext.Current.CancellationToken));
    }
}
