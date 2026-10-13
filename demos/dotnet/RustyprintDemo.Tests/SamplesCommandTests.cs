using RustyprintDemo.Samples;

namespace RustyprintDemo.Tests;

public class SamplesCommandTests
{
    [Fact]
    public async Task Render_samples_writes_every_sample_to_a_pdf()
    {
        var dir = Directory.CreateTempSubdirectory("rustyprint-samples-");
        try
        {
            var output = new StringWriter();
            var exit = await SamplesCommand.RunAsync([dir.FullName], output, TestContext.Current.CancellationToken);
            Assert.Equal(0, exit);
            foreach (var sample in Sample.All)
            {
                var pdf = await File.ReadAllBytesAsync(Path.Combine(dir.FullName, $"{sample.Id}.pdf"), TestContext.Current.CancellationToken);
                Assert.True(Pdf.IsPdf(pdf), sample.Id);
                var pages = Pdf.Pages(pdf);
                Assert.Matches($@"{sample.Id}\.pdf\s+{pages} {(pages == 1 ? "page" : "pages")}", output.ToString());
            }
        }
        finally
        {
            dir.Delete(recursive: true);
        }
    }
}
