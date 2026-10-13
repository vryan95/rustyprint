using System;

namespace Rustyprint;

/// <summary>The kind of a failed render.</summary>
public enum RenderErrorKind
{
    /// <summary>The options are not valid, such as an unknown page size.</summary>
    InvalidOptions,

    /// <summary>The document exceeds one of the limits in the options.</summary>
    LimitExceeded,

    /// <summary>Rendering took longer than the deadline in the options.</summary>
    Timeout,

    /// <summary>A bug in rustyprint.</summary>
    Internal,
}

/// <summary>A render that failed. Each kind of failure has its own subclass.</summary>
public abstract class RustyprintException : Exception
{
    /// <summary>Makes an exception of this kind with a message.</summary>
    protected RustyprintException(string message, RenderErrorKind kind)
        : base(message)
    {
        Kind = kind;
    }

    /// <summary>The kind of failure.</summary>
    public RenderErrorKind Kind { get; }
}

/// <summary>The options are not valid, such as an unknown page size or a margin that is not a length.</summary>
public sealed class InvalidOptionsException : RustyprintException
{
    /// <summary>Makes the exception with a message saying what is wrong.</summary>
    public InvalidOptionsException(string message) : base(message, RenderErrorKind.InvalidOptions) { }
}

/// <summary>The document exceeds one of the limits in <see cref="RenderOptions.Limits"/>.</summary>
public sealed class LimitExceededException : RustyprintException
{
    /// <summary>Makes the exception with a message saying which limit.</summary>
    public LimitExceededException(string message) : base(message, RenderErrorKind.LimitExceeded) { }
}

/// <summary>Rendering took longer than <see cref="RenderLimits.Timeout"/>.</summary>
public sealed class RenderTimeoutException : RustyprintException
{
    /// <summary>Makes the exception with a message.</summary>
    public RenderTimeoutException(string message) : base(message, RenderErrorKind.Timeout) { }
}

/// <summary>A bug in rustyprint, such as a panic in the renderer.</summary>
public sealed class InternalRenderException : RustyprintException
{
    /// <summary>Makes the exception with a message.</summary>
    public InternalRenderException(string message) : base(message, RenderErrorKind.Internal) { }
}
