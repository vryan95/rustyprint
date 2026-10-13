using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using System.Text.RegularExpressions;
using RustyprintDemo.Playground;

namespace RustyprintDemo.Tests;

public partial class PlaygroundTests(DemoApp app) : IClassFixture<DemoApp>
{
    private readonly HttpClient _client = app.CreateClient();
    private static CancellationToken Cancel => TestContext.Current.CancellationToken;

    [Fact]
    public async Task The_playground_starts_with_an_example_to_edit()
    {
        var html = await _client.GetStringAsync("/playground", Cancel);
        Assert.Contains("<form class=\"playground\"", html, StringComparison.Ordinal);
        Assert.Matches(@"<textarea name=""html""[^>]*>[^<]*&lt;h1&gt;", html);
        Assert.Matches(@"<textarea name=""css""[^>]*>[^<]*@page", html);
        Assert.True(Regex.Count(html, "<option") >= 3);
        Assert.Contains("class=\"preview\"", html, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Rendering_returns_the_pdf_its_pages_and_the_warnings()
    {
        var (status, body) = await RenderAsync(new()
        {
            ["html"] = "<h1>Hello</h1><p style='colr: red'>x</p>",
            ["css"] = "@page { margin: 1cm } h1 { color: teal }",
            ["page_size"] = "A5",
            ["landscape"] = "1",
            ["margin"] = "",
        });
        Assert.Equal(HttpStatusCode.OK, status);
        var pdf = Convert.FromBase64String(body.GetProperty("pdf").GetString()!);
        Assert.True(Pdf.IsPdf(pdf));
        Assert.Equal(1, body.GetProperty("pages").GetInt32());
        Assert.True(body.GetProperty("milliseconds").TryGetInt64(out _));
        Assert.Contains(body.GetProperty("warnings").EnumerateArray(), warning => warning.GetString()!.Contains("colr", StringComparison.Ordinal));
        Assert.Equal([210, 148], body.GetProperty("page_mm").EnumerateArray().Select(mm => mm.GetInt32()));
    }

    [Fact]
    public async Task The_example_leaves_the_page_size_and_margin_to_the_options()
    {
        var bodies = new List<JsonElement>();
        foreach (var options in new Dictionary<string, string>[] { new(), new() { ["margin"] = "5mm" }, new() { ["page_size"] = "Letter" } })
        {
            options["html"] = PlaygroundEndpoints.ExampleHtml;
            options["css"] = PlaygroundEndpoints.ExampleCss;
            bodies.Add((await RenderAsync(options)).Body);
        }
        Assert.Equal([210, 297], bodies[0].GetProperty("page_mm").EnumerateArray().Select(mm => mm.GetInt32()));
        Assert.NotEqual(bodies[0].GetProperty("pdf").GetString(), bodies[1].GetProperty("pdf").GetString());
        Assert.Equal([216, 279], bodies[2].GetProperty("page_mm").EnumerateArray().Select(mm => mm.GetInt32()));
    }

    [Fact]
    public async Task Invalid_options_and_documents_over_the_limits_are_reported_not_thrown()
    {
        var (status, body) = await RenderAsync(new() { ["html"] = "<p>x</p>", ["margin"] = "wide" });
        Assert.Equal(HttpStatusCode.UnprocessableEntity, status);
        Assert.Contains("margin", body.GetProperty("error").GetString(), StringComparison.Ordinal);

        (status, body) = await RenderAsync(new() { ["html"] = string.Concat(Enumerable.Repeat("<div style='break-after: page'>x</div>", 30)) });
        Assert.Equal(HttpStatusCode.UnprocessableEntity, status);
        Assert.Contains("page", body.GetProperty("error").GetString(), StringComparison.Ordinal);
    }

    [Fact]
    public async Task A_render_needs_the_forms_antiforgery_token()
    {
        var response = await _client.PostAsync("/playground", new FormUrlEncodedContent(new Dictionary<string, string> { ["html"] = "<p>x</p>" }), Cancel);
        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode);
    }

    // Posts the form as the page's script does, with the token from the page.
    private async Task<(HttpStatusCode Status, JsonElement Body)> RenderAsync(Dictionary<string, string> fields)
    {
        var page = await _client.GetStringAsync("/playground", Cancel);
        fields["__RequestVerificationToken"] = Token().Match(page).Groups[1].Value;
        var response = await _client.PostAsync("/playground", new FormUrlEncodedContent(fields), Cancel);
        return (response.StatusCode, await response.Content.ReadFromJsonAsync<JsonElement>(Cancel));
    }

    [GeneratedRegex(@"name=""__RequestVerificationToken"" value=""([^""]+)""")]
    private static partial Regex Token();
}
