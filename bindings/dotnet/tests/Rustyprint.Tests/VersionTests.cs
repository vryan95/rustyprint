using Xunit;

namespace Rustyprint.Tests;

public class VersionTests
{
    [Fact]
    public void Matches_the_workspace_version() => Assert.Equal(Repo.CargoVersion(), PdfRenderer.Version);

    [Fact]
    public void Matches_the_native_library_version() => Assert.Equal(PdfRenderer.Version, PdfRenderer.NativeVersion);
}
