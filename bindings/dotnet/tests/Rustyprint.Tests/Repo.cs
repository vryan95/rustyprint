using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace Rustyprint.Tests;

internal static class Repo
{
    // The repository, found from the test assembly's directory by its Cargo.toml.
    public static string Root { get; } = FindRoot();

    public static string PathTo(string relative) => Path.Combine(Root, relative);

    // The workspace version in Cargo.toml.
    public static string CargoVersion() =>
        Regex.Match(File.ReadAllText(PathTo("Cargo.toml")), @"\[workspace\.package\]\s+version = ""([^""]+)""").Groups[1].Value;

    // The page sizes of a PDF in points, from its /MediaBox entries.
    public static (double Width, double Height)[] PageSizes(byte[] pdf) =>
        Regex.Matches(Encoding.Latin1.GetString(pdf), @"/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]")
            .Select(m => (Round(m.Groups[1].Value), Round(m.Groups[2].Value)))
            .ToArray();

    public static bool IsPdf(byte[] pdf) => pdf.Length > 5 && Encoding.ASCII.GetString(pdf, 0, 5) == "%PDF-";

    private static double Round(string points) =>
        Math.Round(double.Parse(points, System.Globalization.CultureInfo.InvariantCulture), 1);

    private static string FindRoot()
    {
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir != null; dir = dir.Parent)
        {
            if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")) && Directory.Exists(Path.Combine(dir.FullName, "crates")))
            {
                return dir.FullName;
            }
        }
        throw new InvalidOperationException("the tests run from inside the rustyprint repository");
    }
}
