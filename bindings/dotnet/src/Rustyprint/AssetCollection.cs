using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;

namespace Rustyprint;

/// <summary>
/// The files a document refers to, such as stylesheets, images and fonts, by the path the
/// document uses for each, such as <c>img/logo.png</c>. A path added again replaces the
/// earlier file.
/// </summary>
/// <example>
/// <code>
/// var options = new RenderOptions { Assets = { ["invoice.css"] = File.ReadAllBytes("invoice.css") } };
/// options.Assets.AddFile("logo.png", "images/logo.png");
/// options.Assets.AddDirectory("fonts", "fonts");
/// </code>
/// </example>
public sealed class AssetCollection : IEnumerable<KeyValuePair<string, byte[]>>
{
    private readonly Dictionary<string, byte[]> _files = new(StringComparer.Ordinal);

    /// <summary>The number of files.</summary>
    public int Count => _files.Count;

    /// <summary>The file at this path, which is added or replaced when set.</summary>
    /// <exception cref="KeyNotFoundException">There is no file at the path.</exception>
    public byte[] this[string path]
    {
        get => _files[path];
        set => Add(path, value);
    }

    /// <summary>Adds a file's contents under the path the document uses.</summary>
    /// <exception cref="ArgumentException">The path contains a NUL character or is not valid UTF-16.</exception>
    public void Add(string path, byte[] data)
    {
        Utf8.Check(path ?? throw new ArgumentNullException(nameof(path)), nameof(path));
        if (path.Contains('\0', StringComparison.Ordinal))
        {
            throw new ArgumentException("An asset path cannot contain a NUL character.", nameof(path));
        }
        _files[path] = data ?? throw new ArgumentNullException(nameof(data));
    }

    /// <summary>Adds text, such as a stylesheet, as UTF-8.</summary>
    public void Add(string path, string text) =>
        Add(path, Utf8.Bytes(text ?? throw new ArgumentNullException(nameof(text)), nameof(text)));

    /// <summary>Adds the rest of a stream.</summary>
    public void Add(string path, Stream stream)
    {
        ArgumentNullException.ThrowIfNull(stream);
        using var copy = new MemoryStream();
        stream.CopyTo(copy);
        Add(path, copy.ToArray());
    }

    /// <summary>Adds a file from disk under the path the document uses.</summary>
    public void AddFile(string path, string file) => Add(path, File.ReadAllBytes(file));

    /// <summary>
    /// Adds every file in a directory and its subdirectories, by its path relative to the
    /// directory, with <c>/</c> between the parts, such as <c>fonts/Caveat.ttf</c>, after
    /// <paramref name="prefix"/> and a <c>/</c> if given.
    /// </summary>
    public void AddDirectory(string directory, string? prefix = null)
    {
        var root = Path.GetFullPath(directory ?? throw new ArgumentNullException(nameof(directory)));
        foreach (var file in Directory.EnumerateFiles(root, "*", SearchOption.AllDirectories))
        {
            var relative = Path.GetRelativePath(root, file).Replace(Path.DirectorySeparatorChar, '/');
            AddFile(string.IsNullOrEmpty(prefix) ? relative : $"{prefix!.TrimEnd('/')}/{relative}", file);
        }
    }

    /// <summary>Whether there is a file at the path.</summary>
    public bool Contains(string path) => _files.ContainsKey(path);

    /// <summary>Removes the file at the path, if there is one.</summary>
    public bool Remove(string path) => _files.Remove(path);

    /// <summary>Removes every file.</summary>
    public void Clear() => _files.Clear();

    /// <inheritdoc/>
    public IEnumerator<KeyValuePair<string, byte[]>> GetEnumerator() => _files.GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}
