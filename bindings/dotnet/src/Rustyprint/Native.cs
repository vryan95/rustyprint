using System;
using System.Runtime.InteropServices;

namespace Rustyprint;

// The native library's C ABI (crates/rustyprint-ffi/include/rustyprint.h).
internal static class Native
{
    // rustyprint_ffi.dll, librustyprint_ffi.so or librustyprint_ffi.dylib: in the package's
    // runtimes/<rid>/native/, or next to the assembly in a build from this repository.
    private const string Library = "rustyprint_ffi";

    // RUSTYPRINT_LIBRARY, if set, is the path of the library to use instead.
    static Native()
    {
        NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, (name, assembly, searchPath) =>
            name == Library && Environment.GetEnvironmentVariable("RUSTYPRINT_LIBRARY") is { Length: > 0 } path
                ? NativeLibrary.Load(path)
                : IntPtr.Zero);
    }

    internal enum Status
    {
        Ok = 0,
        InvalidArgument = 1,
        InvalidOptions = 2,
        LimitExceeded = 3,
        Timeout = 4,
        Internal = 5,
    }

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr rp_version();

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern RequestHandle rp_request_new(byte[] optionsJson, UIntPtr len);

    // `path` is NUL-terminated UTF-8.
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern Status rp_request_add_asset(RequestHandle request, byte[] path, byte[] data, UIntPtr len);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern Status rp_render(RequestHandle request, byte[] html, UIntPtr len, out ResultHandle result);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr rp_result_pdf(ResultHandle result, out UIntPtr len);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr rp_result_warnings_json(ResultHandle result, out UIntPtr len);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr rp_result_error(ResultHandle result, out UIntPtr len);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void rp_request_free(IntPtr request);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void rp_result_free(IntPtr result);

    // `len` bytes at `pointer`, copied, or none for a null pointer.
    internal static byte[] Copy(IntPtr pointer, UIntPtr len)
    {
        var bytes = new byte[pointer == IntPtr.Zero ? 0 : checked((int)len.ToUInt64())];
        if (bytes.Length > 0)
        {
            Marshal.Copy(pointer, bytes, 0, bytes.Length);
        }
        return bytes;
    }
}

// A request: options and assets, freed by the library.
internal sealed class RequestHandle : SafeHandle
{
    public RequestHandle() : base(IntPtr.Zero, ownsHandle: true) { }

    public override bool IsInvalid => handle == IntPtr.Zero;

    protected override bool ReleaseHandle()
    {
        Native.rp_request_free(handle);
        return true;
    }
}

// A render's result: a PDF and its warnings, or an error, freed by the library.
internal sealed class ResultHandle : SafeHandle
{
    public ResultHandle() : base(IntPtr.Zero, ownsHandle: true) { }

    public override bool IsInvalid => handle == IntPtr.Zero;

    protected override bool ReleaseHandle()
    {
        Native.rp_result_free(handle);
        return true;
    }
}
