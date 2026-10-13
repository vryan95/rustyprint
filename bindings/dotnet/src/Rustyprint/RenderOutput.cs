using System.Collections.Generic;

namespace Rustyprint;

/// <summary>A rendered document: the PDF, and the problems found in its CSS and assets.</summary>
public sealed class RenderOutput
{
    internal RenderOutput(byte[] pdf, IReadOnlyList<string> warnings)
    {
        Pdf = pdf;
        Warnings = warnings;
    }

    /// <summary>The PDF file.</summary>
#pragma warning disable CA1819 // The PDF is the result; a copy for each read would only cost.
    public byte[] Pdf { get; }
#pragma warning restore CA1819

    /// <summary>
    /// Problems the render worked around, such as an unknown CSS property or a missing image,
    /// like <c>style.css:3: unsupported property `colr`</c>.
    /// </summary>
    public IReadOnlyList<string> Warnings { get; }
}
