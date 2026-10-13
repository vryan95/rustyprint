using System;
using System.Text;

namespace Rustyprint;

// Strict UTF-8, which refuses text with lone surrogates instead of replacing them.
internal static class Utf8
{
    private static readonly UTF8Encoding Encoding = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    internal static byte[] Bytes(string text, string parameter)
    {
        try
        {
            return Encoding.GetBytes(text);
        }
        catch (EncoderFallbackException e)
        {
            throw new ArgumentException($"The {parameter} is not valid UTF-16: {e.Message}", parameter, e);
        }
    }

    // A NUL-terminated copy, for the C ABI.
    internal static byte[] NulTerminated(string text, string parameter)
    {
        var bytes = Bytes(text, parameter);
        Array.Resize(ref bytes, bytes.Length + 1);
        return bytes;
    }

    internal static void Check(string text, string parameter) => Bytes(text, parameter);

    internal static string String(byte[] bytes) => Encoding.GetString(bytes);
}
