using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json.Nodes;
using System.Threading;
using Xunit;

namespace Rustyprint.Tests;

// RenderOptions are the JSON options of schema/render-options.schema.json, which every front end
// shares.
public class OptionsSchemaTests
{
    private static readonly JsonObject Schema =
        JsonNode.Parse(File.ReadAllText(Repo.PathTo("schema/render-options.schema.json")))!.AsObject();

    // The schema's examples, in order, as typed options.
    private static readonly RenderOptions[] Examples =
    [
        new(),
        new() { PageSize = PageSize.A4, Margin = "15mm", Title = "Invoice 42" },
        new()
        {
            PageSize = PageSize.Letter,
            Landscape = true,
            Margin = "0.5in 0.75in",
            Stylesheet = "body { font-family: sans-serif }",
        },
        new()
        {
            PageSize = new PageSize("100mm", "150mm"),
            Margin = "0",
            Remote = new RemoteOptions("cdn.example.com", "*.images.example.com"),
            Limits = new RenderLimits { MaxPages = 50, Timeout = TimeSpan.FromSeconds(10) },
        },
        new() { Limits = new RenderLimits { Timeout = Timeout.InfiniteTimeSpan } },
    ];

    public static TheoryData<int> ExampleIndexes => new(Enumerable.Range(0, Examples.Length));

    [Theory]
    [MemberData(nameof(ExampleIndexes))]
    public void Writes_each_schema_example_and_renders_with_it(int index)
    {
        var example = Schema["examples"]![index];
        Assert.True(JsonNode.DeepEquals(example, JsonNode.Parse(Examples[index].ToJson())),
            $"{Examples[index].ToJson()} is not {example!.ToJsonString()}");
        Assert.True(Repo.IsPdf(PdfRenderer.Render("<p>x</p>", Examples[index]).Pdf));
    }

    [Fact]
    public void Has_the_examples_of_the_schema() => Assert.Equal(Schema["examples"]!.AsArray().Count, Examples.Length);

    [Fact]
    public void Covers_every_option_in_the_schema()
    {
        var everything = new RenderOptions
        {
            PageSize = new PageSize("210mm", "297mm"),
            Landscape = false,
            Margin = "1cm",
            Title = "Title",
            Stylesheet = "p { color: red }",
            Remote = new RemoteOptions("cdn.example.com")
            {
                AllowHttp = true,
                Timeout = TimeSpan.FromSeconds(5),
                MaxBytes = 1_000_000,
                MaxRedirects = 2,
                MaxFetches = 10,
            },
            Limits = new RenderLimits
            {
                MaxInputBytes = 1_000_000,
                MaxNodes = 10_000,
                MaxDepth = 100,
                MaxPages = 10,
                MaxImagePixels = 1_000_000,
                Timeout = TimeSpan.FromSeconds(30),
            },
        };
        var json = JsonNode.Parse(everything.ToJson())!.AsObject();
        var properties = Schema["properties"]!.AsObject();

        Assert.Equal(Keys(properties), Keys(json));
        Assert.Equal(Keys(properties["remote"]!["properties"]!.AsObject()), Keys(json["remote"]!.AsObject()));
        Assert.Equal(Keys(properties["limits"]!["properties"]!.AsObject()), Keys(json["limits"]!.AsObject()));
        var sizeAsLengths = properties["page_size"]!["oneOf"]![1]!["properties"]!.AsObject();
        Assert.Equal(Keys(sizeAsLengths), Keys(json["page_size"]!.AsObject()));
        Assert.True(Repo.IsPdf(PdfRenderer.Render("<p>x</p>", everything).Pdf));
    }

    [Fact]
    public void Leaves_out_the_options_not_set() => Assert.Equal("{}", new RenderOptions { Assets = { ["a.css"] = [] } }.ToJson());

    [Fact]
    public void Refuses_what_the_schema_does_not_allow()
    {
        Assert.Throws<InvalidOptionsException>(() =>
            PdfRenderer.Render("<p>x</p>", new RenderOptions { Limits = new RenderLimits { MaxPages = -1 } }));
        Assert.Throws<InvalidOptionsException>(() =>
            PdfRenderer.Render("<p>x</p>", new RenderOptions { PageSize = new PageSize("wide", "1cm") }));
    }

    private static SortedSet<string> Keys(JsonObject json) => new(json.Select(property => property.Key), StringComparer.Ordinal);
}
