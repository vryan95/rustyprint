namespace RustyprintDemo;

// Where the demo finds the repository's files.
public static class DemoPaths
{
    // The repository, found from the app's directory by its Cargo.toml.
    public static string Repository { get; } = FindRepository();

    // The shared sample documents, or RUSTYPRINT_SAMPLES if set.
    public static string Samples { get; } = Path.GetFullPath(
        Environment.GetEnvironmentVariable("RUSTYPRINT_SAMPLES") is { Length: > 0 } samples
            ? samples
            : Path.Combine(Repository, "demos", "samples"));

    private static string FindRepository()
    {
        foreach (var start in new[] { AppContext.BaseDirectory, Directory.GetCurrentDirectory() })
        {
            for (var dir = new DirectoryInfo(start); dir != null; dir = dir.Parent)
            {
                if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")) && Directory.Exists(Path.Combine(dir.FullName, "demos")))
                {
                    return dir.FullName;
                }
            }
        }
        throw new InvalidOperationException("The demo runs from inside the rustyprint repository.");
    }
}
