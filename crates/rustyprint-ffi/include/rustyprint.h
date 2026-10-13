/* rustyprint: render HTML and CSS to PDF. */

#ifndef RUSTYPRINT_H
#define RUSTYPRINT_H

/* Generated from crates/rustyprint-ffi/src/lib.rs by cbindgen; do not edit. Run `cargo run -p rustyprint-ffi --example header` to regenerate it. */

#include <stddef.h>
#include <stdint.h>

/**
 * What a call did. A render's status is also its result's.
 */
enum rp_status
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : int32_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  /**
   * The call succeeded.
   */
  RP_STATUS_OK = 0,
  /**
   * An argument is null where it may not be, or is not UTF-8.
   */
  RP_STATUS_INVALID_ARGUMENT = 1,
  /**
   * The JSON options are not valid.
   */
  RP_STATUS_INVALID_OPTIONS = 2,
  /**
   * The document exceeds one of the limits in the options.
   */
  RP_STATUS_LIMIT_EXCEEDED = 3,
  /**
   * Rendering took longer than the deadline in the options.
   */
  RP_STATUS_TIMEOUT = 4,
  /**
   * A bug in rustyprint, such as a caught panic.
   */
  RP_STATUS_INTERNAL = 5,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum rp_status rp_status;
#else
typedef int32_t rp_status;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

/**
 * What a render gave: a PDF and its warnings, or an error. Made by `rp_render` and freed by
 * `rp_result_free`.
 */
typedef struct rp_result rp_result;

/**
 * Options and assets for rendering documents. Made by `rp_request_new` and freed by
 * `rp_request_free`.
 */
typedef struct rp_request rp_request;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * The library's version, such as `1.2.0`, as a static string.
 */
const char *rp_version(void);

/**
 * Makes a request from JSON options: `len` bytes of UTF-8 at `options_json`, in the format of
 * `schema/render-options.schema.json`. A length of zero gives the default options.
 *
 * Invalid options still give a request; rendering with it fails with
 * `RP_STATUS_INVALID_OPTIONS` and says why. Returns null only if the library fails.
 *
 * # Safety
 *
 * Unless `len` is zero, `options_json` is null or points to `len` readable bytes.
 */
struct rp_request *rp_request_new(const char *options_json, size_t len);

/**
 * Adds a file the document can refer to, such as an image, stylesheet or font: `len` bytes at
 * `data`, under `path`, the NUL-terminated UTF-8 path the document uses, such as
 * `img/logo.png`. The data is copied. A path added again replaces the earlier file.
 *
 * Returns `RP_STATUS_INVALID_ARGUMENT` if `request` or `path` is null, `path` is not UTF-8,
 * or `data` is null with a length that is not zero.
 *
 * # Safety
 *
 * `request` is null or a request no other thread is using. `path` is null or a NUL-terminated
 * string. Unless `len` is zero, `data` is null or points to `len` readable bytes.
 */
rp_status rp_request_add_asset(struct rp_request *request,
                               const char *path,
                               const uint8_t *data,
                               size_t len);

/**
 * Renders `len` bytes of UTF-8 HTML at `html` with the request's options and assets, and
 * stores the result in `*out`, which the caller frees with `rp_result_free`. Returns the
 * result's status: `RP_STATUS_OK` with a PDF, or an error with a message.
 *
 * Every call with a non-null `out` stores a result there, even when it fails, unless the
 * library cannot allocate one. A null `out` returns `RP_STATUS_INVALID_ARGUMENT`.
 *
 * # Safety
 *
 * `request` is null or a live request that no thread is adding assets to. Unless `len` is
 * zero, `html` is null or points to `len` readable bytes. `out` is null or writable.
 */
rp_status rp_render(const struct rp_request *request,
                    const char *html,
                    size_t len,
                    struct rp_result **out);

/**
 * The result's status, the one `rp_render` returned. `RP_STATUS_INVALID_ARGUMENT` for a null
 * result.
 *
 * # Safety
 *
 * `result` is null or a live result.
 */
rp_status rp_result_status(const struct rp_result *result);

/**
 * The PDF, with its length in bytes stored in `*len` unless `len` is null. Null, with a
 * length of zero, if the render failed. The bytes live as long as the result.
 *
 * # Safety
 *
 * `result` is null or a live result. `len` is null or writable.
 */
const uint8_t *rp_result_pdf(const struct rp_result *result, size_t *len);

/**
 * The render's warnings, as a NUL-terminated JSON array of strings such as
 * `` ["style.css:3: unsupported property `colr`"] ``, with its length in bytes, not counting the NUL,
 * stored in `*len` unless `len` is null. `[]` if there are none or the render failed; null
 * only for a null result. The string lives as long as the result.
 *
 * # Safety
 *
 * `result` is null or a live result. `len` is null or writable.
 */
const char *rp_result_warnings_json(const struct rp_result *result,
                                    size_t *len);

/**
 * Why the render failed, as a NUL-terminated message, with its length in bytes, not counting
 * the NUL, stored in `*len` unless `len` is null. Null, with a length of zero, if the render
 * succeeded. The string lives as long as the result.
 *
 * # Safety
 *
 * `result` is null or a live result. `len` is null or writable.
 */
const char *rp_result_error(const struct rp_result *result, size_t *len);

/**
 * Frees a request. Null does nothing.
 *
 * # Safety
 *
 * `request` is null or a live request from `rp_request_new`, which no other thread is using
 * and which is not used again.
 */
void rp_request_free(struct rp_request *request);

/**
 * Frees a result, and with it the PDF, warnings and error it holds. Null does nothing.
 *
 * # Safety
 *
 * `result` is null or a live result from `rp_render`, which is not used again.
 */
void rp_result_free(struct rp_result *result);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* RUSTYPRINT_H */
