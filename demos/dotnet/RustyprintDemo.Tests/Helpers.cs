using System.Text;
using System.Text.RegularExpressions;
using Microsoft.AspNetCore.Mvc.Testing;

namespace RustyprintDemo.Tests;

// The demo app, served in memory, shared by the tests of one class.
public sealed class DemoApp : WebApplicationFactory<Program>;

internal static partial class Pdf
{
    public static bool IsPdf(byte[] pdf) => pdf.Length > 5 && Encoding.ASCII.GetString(pdf, 0, 5) == "%PDF-";

    public static int Pages(byte[] pdf) => MediaBox().Count(Encoding.Latin1.GetString(pdf));

    public static bool HasImage(byte[] pdf) => Image().IsMatch(Encoding.Latin1.GetString(pdf));

    [GeneratedRegex(@"/MediaBox \[")]
    private static partial Regex MediaBox();

    [GeneratedRegex(@"/Subtype\s*/Image")]
    private static partial Regex Image();
}
