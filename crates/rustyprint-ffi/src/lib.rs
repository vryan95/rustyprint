//! A C ABI over [`rustyprint::render`], built as the shared library that C programs, the Ruby
//! gem and the .NET package load. The header, `include/rustyprint.h`, is generated from this
//! file by cbindgen.
//!
//! A program makes a request from JSON options, adds the files the document refers to, renders
//! HTML with it and reads the PDF, the warnings or the error from the result:
//!
//! ```c
//! rp_request *request = rp_request_new(options, strlen(options));
//! rp_request_add_asset(request, "img/logo.png", logo, logo_len);
//! rp_result *result = NULL;
//! if (rp_render(request, html, strlen(html), &result) == RP_STATUS_OK) {
//!     size_t len;
//!     const uint8_t *pdf = rp_result_pdf(result, &len);
//!     fwrite(pdf, 1, len, file);
//! } else {
//!     fprintf(stderr, "%s\n", rp_result_error(result, NULL));
//! }
//! rp_result_free(result);
//! rp_request_free(request);
//! ```
//!
//! - Options cross as JSON, in the format of `schema/render-options.schema.json`, so new options
//!   never change the ABI.
//! - Text is UTF-8. Options and HTML are a pointer and a length, and need not end in NUL;
//!   asset paths end in NUL. Strings the library gives end in NUL.
//! - Requests and results are opaque, and only the library frees them, with
//!   [`rp_request_free`] and [`rp_result_free`].
//! - No entry point unwinds into the caller: a panic is caught and reported as
//!   `RP_STATUS_INTERNAL`.
//! - A request may render on several threads at once, as long as no thread adds assets to it
//!   meanwhile. Each render runs on a thread of its own with a large stack, so the calling
//!   thread's stack size does not matter.

use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ptr, slice};

use rustyprint::{Error, Output, RenderOptions};

/// What a call did. A render's status is also its result's.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The call succeeded.
    Ok = 0,
    /// An argument is null where it may not be, or is not UTF-8.
    InvalidArgument = 1,
    /// The JSON options are not valid.
    InvalidOptions = 2,
    /// The document exceeds one of the limits in the options.
    LimitExceeded = 3,
    /// Rendering took longer than the deadline in the options.
    Timeout = 4,
    /// A bug in rustyprint, such as a caught panic.
    Internal = 5,
}

/// Options and assets for rendering documents. Made by `rp_request_new` and freed by
/// `rp_request_free`.
pub struct Request {
    /// The options with the assets added so far, or why the JSON options are invalid.
    options: Result<RenderOptions, String>,
}

/// What a render gave: a PDF and its warnings, or an error. Made by `rp_render` and freed by
/// `rp_result_free`.
pub struct RenderResult {
    status: Status,
    pdf: Vec<u8>,
    /// A JSON array of strings.
    warnings: CString,
    error: Option<CString>,
}

impl RenderResult {
    fn output(output: Output) -> Self {
        let warnings = serde_json::to_string(&output.warnings).unwrap_or_else(|_| "[]".into());
        Self {
            status: Status::Ok,
            pdf: output.pdf,
            warnings: c_string(warnings),
            error: None,
        }
    }

    fn error(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            pdf: Vec::new(),
            warnings: c"[]".to_owned(),
            error: Some(c_string(message.into())),
        }
    }
}

/// `text` as a C string, with any NUL in it replaced.
fn c_string(text: String) -> CString {
    CString::new(text.replace('\0', "\u{fffd}")).unwrap_or_default()
}

fn status(error: &Error) -> Status {
    match error {
        Error::InvalidOptions(_) => Status::InvalidOptions,
        Error::LimitExceeded { .. } => Status::LimitExceeded,
        Error::Timeout => Status::Timeout,
        _ => Status::Internal,
    }
}

/// Runs `f`, or `on_panic` if it panics, so that no panic unwinds into the caller.
fn guard<T>(on_panic: impl FnOnce() -> T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| on_panic())
}

/// The `len` bytes at `data`, or `None` if `data` is null and `len` is not zero.
///
/// # Safety
///
/// Unless `len` is zero, `data` is null or points to `len` bytes that stay unchanged for `'a`.
unsafe fn bytes<'a>(data: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        Some(&[])
    } else if data.is_null() {
        None
    } else {
        // SAFETY: the caller guarantees `len` readable bytes at `data`.
        Some(unsafe { slice::from_raw_parts(data, len) })
    }
}

/// Stores `value` in `*len`, unless `len` is null.
///
/// # Safety
///
/// `len` is null or writable.
unsafe fn set_len(len: *mut usize, value: usize) {
    if !len.is_null() {
        // SAFETY: the caller guarantees `len` is writable.
        unsafe { *len = value };
    }
}

/// The library's version, such as `1.2.0`, as a static string.
#[unsafe(no_mangle)]
pub extern "C" fn rp_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// Makes a request from JSON options: `len` bytes of UTF-8 at `options_json`, in the format of
/// `schema/render-options.schema.json`. A length of zero gives the default options.
///
/// Invalid options still give a request; rendering with it fails with
/// `RP_STATUS_INVALID_OPTIONS` and says why. Returns null only if the library fails.
///
/// # Safety
///
/// Unless `len` is zero, `options_json` is null or points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_request_new(options_json: *const c_char, len: usize) -> *mut Request {
    guard(ptr::null_mut, || {
        // SAFETY: the caller guarantees the bytes are readable.
        let options = match unsafe { bytes(options_json.cast(), len) } {
            None => Err(format!(
                "invalid options: options_json is null but len is {len}"
            )),
            Some([]) => Ok(RenderOptions::default()),
            Some(json) => match std::str::from_utf8(json) {
                Ok(json) => RenderOptions::from_json(json).map_err(|e| e.to_string()),
                Err(e) => Err(format!("invalid options: the JSON is not UTF-8: {e}")),
            },
        };
        Box::into_raw(Box::new(Request { options }))
    })
}

/// Adds a file the document can refer to, such as an image, stylesheet or font: `len` bytes at
/// `data`, under `path`, the NUL-terminated UTF-8 path the document uses, such as
/// `img/logo.png`. The data is copied. A path added again replaces the earlier file.
///
/// Returns `RP_STATUS_INVALID_ARGUMENT` if `request` or `path` is null, `path` is not UTF-8,
/// or `data` is null with a length that is not zero.
///
/// # Safety
///
/// `request` is null or a request no other thread is using. `path` is null or a NUL-terminated
/// string. Unless `len` is zero, `data` is null or points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_request_add_asset(
    request: *mut Request,
    path: *const c_char,
    data: *const u8,
    len: usize,
) -> Status {
    guard(
        || Status::Internal,
        || {
            // SAFETY: the caller guarantees the request is live and not in use elsewhere.
            let Some(request) = (unsafe { request.as_mut() }) else {
                return Status::InvalidArgument;
            };
            if path.is_null() {
                return Status::InvalidArgument;
            }
            // SAFETY: the caller guarantees the path is NUL-terminated.
            let Ok(path) = unsafe { std::ffi::CStr::from_ptr(path) }.to_str() else {
                return Status::InvalidArgument;
            };
            // SAFETY: the caller guarantees the bytes are readable.
            let Some(data) = (unsafe { bytes(data, len) }) else {
                return Status::InvalidArgument;
            };
            // Assets of a request with invalid options are dropped: it cannot render.
            if let Ok(options) = &mut request.options {
                options.assets.files.insert(path.to_owned(), data.to_vec());
            }
            Status::Ok
        },
    )
}

/// Renders `len` bytes of UTF-8 HTML at `html` with the request's options and assets, and
/// stores the result in `*out`, which the caller frees with `rp_result_free`. Returns the
/// result's status: `RP_STATUS_OK` with a PDF, or an error with a message.
///
/// Every call with a non-null `out` stores a result there, even when it fails, unless the
/// library cannot allocate one. A null `out` returns `RP_STATUS_INVALID_ARGUMENT`.
///
/// # Safety
///
/// `request` is null or a live request that no thread is adding assets to. Unless `len` is
/// zero, `html` is null or points to `len` readable bytes. `out` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_render(
    request: *const Request,
    html: *const c_char,
    len: usize,
    out: *mut *mut RenderResult,
) -> Status {
    if out.is_null() {
        return Status::InvalidArgument;
    }
    let result = guard(
        || RenderResult::error(Status::Internal, "internal error: rp_render panicked"),
        // SAFETY: the caller's guarantees are those `render` needs.
        || unsafe { render(request, html, len) },
    );
    let status = result.status;
    // SAFETY: `out` is not null, and the caller guarantees it is writable.
    unsafe { *out = Box::into_raw(Box::new(result)) };
    status
}

/// # Safety
///
/// As for [`rp_render`].
unsafe fn render(request: *const Request, html: *const c_char, len: usize) -> RenderResult {
    // SAFETY: the caller guarantees the request is null or live.
    let Some(request) = (unsafe { request.as_ref() }) else {
        return RenderResult::error(Status::InvalidArgument, "request is null");
    };
    // SAFETY: the caller guarantees the bytes are readable.
    let Some(html) = (unsafe { bytes(html.cast(), len) }) else {
        return RenderResult::error(
            Status::InvalidArgument,
            format!("html is null but len is {len}"),
        );
    };
    let html = match std::str::from_utf8(html) {
        Ok(html) => html,
        Err(e) => {
            return RenderResult::error(Status::InvalidArgument, format!("html is not UTF-8: {e}"));
        }
    };
    let options = match &request.options {
        Ok(options) => options,
        Err(message) => return RenderResult::error(Status::InvalidOptions, message.clone()),
    };
    match rustyprint::render(html, options) {
        Ok(output) => RenderResult::output(output),
        Err(error) => RenderResult::error(status(&error), error.to_string()),
    }
}

/// The result's status, the one `rp_render` returned. `RP_STATUS_INVALID_ARGUMENT` for a null
/// result.
///
/// # Safety
///
/// `result` is null or a live result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_result_status(result: *const RenderResult) -> Status {
    // SAFETY: the caller guarantees the result is null or live.
    guard(
        || Status::Internal,
        || unsafe { result.as_ref() }.map_or(Status::InvalidArgument, |r| r.status),
    )
}

/// The PDF, with its length in bytes stored in `*len` unless `len` is null. Null, with a
/// length of zero, if the render failed. The bytes live as long as the result.
///
/// # Safety
///
/// `result` is null or a live result. `len` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_result_pdf(result: *const RenderResult, len: *mut usize) -> *const u8 {
    guard(ptr::null, || {
        // SAFETY: the caller guarantees the result is null or live.
        let pdf = unsafe { result.as_ref() }
            .filter(|r| r.status == Status::Ok)
            .map(|r| &r.pdf);
        // SAFETY: the caller guarantees `len` is null or writable.
        unsafe { set_len(len, pdf.map_or(0, Vec::len)) };
        pdf.map_or(ptr::null(), |pdf| pdf.as_ptr())
    })
}

/// The render's warnings, as a NUL-terminated JSON array of strings such as
/// `` ["style.css:3: unsupported property `colr`"] ``, with its length in bytes, not counting the NUL,
/// stored in `*len` unless `len` is null. `[]` if there are none or the render failed; null
/// only for a null result. The string lives as long as the result.
///
/// # Safety
///
/// `result` is null or a live result. `len` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_result_warnings_json(
    result: *const RenderResult,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: the caller guarantees the result is null or live.
    guard(ptr::null, || unsafe {
        string(result.as_ref().map(|r| r.warnings.as_c_str()), len)
    })
}

/// Why the render failed, as a NUL-terminated message, with its length in bytes, not counting
/// the NUL, stored in `*len` unless `len` is null. Null, with a length of zero, if the render
/// succeeded. The string lives as long as the result.
///
/// # Safety
///
/// `result` is null or a live result. `len` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_result_error(
    result: *const RenderResult,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: the caller guarantees the result is null or live.
    guard(ptr::null, || unsafe {
        string(result.as_ref().and_then(|r| r.error.as_deref()), len)
    })
}

/// # Safety
///
/// `len` is null or writable.
unsafe fn string(text: Option<&std::ffi::CStr>, len: *mut usize) -> *const c_char {
    // SAFETY: the caller guarantees `len` is null or writable.
    unsafe { set_len(len, text.map_or(0, |t| t.count_bytes())) };
    text.map_or(ptr::null(), std::ffi::CStr::as_ptr)
}

/// Frees a request. Null does nothing.
///
/// # Safety
///
/// `request` is null or a live request from `rp_request_new`, which no other thread is using
/// and which is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_request_free(request: *mut Request) {
    if !request.is_null() {
        // SAFETY: the caller guarantees the request came from `rp_request_new` and is not used
        // again.
        guard(|| (), || drop(unsafe { Box::from_raw(request) }));
    }
}

/// Frees a result, and with it the PDF, warnings and error it holds. Null does nothing.
///
/// # Safety
///
/// `result` is null or a live result from `rp_render`, which is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_result_free(result: *mut RenderResult) {
    if !result.is_null() {
        // SAFETY: the caller guarantees the result came from `rp_render` and is not used again.
        guard(|| (), || drop(unsafe { Box::from_raw(result) }));
    }
}
