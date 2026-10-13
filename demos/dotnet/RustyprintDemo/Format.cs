using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace RustyprintDemo;

public static partial class Format
{
    public static string Pages(int pages) => pages == 1 ? "1 page" : $"{pages} pages";

    // A size such as 512 Bytes, 45.3 KB or 1.2 MB.
    public static string Size(long bytes)
    {
        if (bytes < 1024) return $"{bytes} Bytes";
        var kilobytes = bytes / 1024.0;
        return kilobytes < 1024
            ? $"{kilobytes.ToString(kilobytes < 100 ? "0.#" : "0", CultureInfo.InvariantCulture)} KB"
            : $"{(kilobytes / 1024).ToString("0.#", CultureInfo.InvariantCulture)} MB";
    }

    // The first page's width and height in millimetres, from its /MediaBox.
    public static int[]? PageMillimetres(byte[] pdf)
    {
        var box = MediaBox().Match(Encoding.Latin1.GetString(pdf));
        return box.Success
            ? [Millimetres(box.Groups[1].Value), Millimetres(box.Groups[2].Value)]
            : null;
    }

    private static int Millimetres(string points) =>
        (int)Math.Round(double.Parse(points, CultureInfo.InvariantCulture) * 25.4 / 72);

    [GeneratedRegex(@"/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]")]
    private static partial Regex MediaBox();
}
