using System;
using System.Linq;
using Xunit;

namespace Rustyprint.Tests;

public class ErrorTests
{
    [Fact]
    public void Invalid_values_throw_InvalidOptionsException()
    {
        var error = Assert.Throws<InvalidOptionsException>(() =>
            PdfRenderer.Render("<p>x</p>", new RenderOptions { PageSize = PageSize.Named("B99") }));
        Assert.Equal(RenderErrorKind.InvalidOptions, error.Kind);
        Assert.Contains("B99", error.Message, StringComparison.Ordinal);

        var margin = Assert.Throws<InvalidOptionsException>(() =>
            PdfRenderer.Render("<p>x</p>", new RenderOptions { Margin = "wide" }));
        Assert.Contains("margin", margin.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void A_document_over_a_limit_throws_LimitExceededException()
    {
        var error = Assert.Throws<LimitExceededException>(() =>
            PdfRenderer.Render("<p>More than ten bytes</p>", new RenderOptions { Limits = new RenderLimits { MaxInputBytes = 10 } }));
        Assert.Equal(RenderErrorKind.LimitExceeded, error.Kind);
        Assert.Contains("input size", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void A_render_past_its_deadline_throws_RenderTimeoutException()
    {
        var html = string.Concat(Enumerable.Repeat("<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>", 5_000));
        var error = Assert.Throws<RenderTimeoutException>(() =>
            PdfRenderer.Render(html, new RenderOptions { Limits = new RenderLimits { Timeout = TimeSpan.FromMilliseconds(1) } }));
        Assert.Equal(RenderErrorKind.Timeout, error.Kind);
        Assert.Contains("deadline", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void Every_error_is_a_RustyprintException()
    {
        foreach (var type in new[] { typeof(InvalidOptionsException), typeof(LimitExceededException), typeof(RenderTimeoutException), typeof(InternalRenderException) })
        {
            Assert.True(typeof(RustyprintException).IsAssignableFrom(type), type.Name);
        }
        Assert.Equal(RenderErrorKind.Internal, new InternalRenderException("bug").Kind);
    }
}
