'use strict'

// The native library's C ABI (crates/rustyprint-ffi/include/rustyprint.h), through koffi.
//
// The HTML and the options are declared as `const uint8_t *` where the header has
// `const char *`: both are a pointer to bytes, the same to the ABI, and the binding passes them
// as bytes with their length, which koffi takes as a Uint8Array without copying them into a
// NUL-terminated string first.

const fs = require('node:fs')
const koffi = require('koffi')
const { libraryPath } = require('./library')

const file = libraryPath()
if (!fs.existsSync(file)) {
  throw new Error(`rustyprint's native library is missing at ${file}. Run \`npm run compile\` in ` +
    'bindings/node, or set RUSTYPRINT_LIBRARY to the library.')
}

let lib
try {
  lib = koffi.load(file)
} catch (error) {
  throw new Error(`rustyprint could not load its native library at ${file}: ${error.message}`, { cause: error })
}

koffi.opaque('rp_request')
koffi.opaque('rp_result')

module.exports = {
  rp_version: lib.func('const char *rp_version(void)'),
  rp_request_new: lib.func('rp_request *rp_request_new(const uint8_t *options_json, size_t len)'),
  rp_request_add_asset: lib.func(
    'int32_t rp_request_add_asset(rp_request *request, const char *path, const uint8_t *data, size_t len)'),
  rp_render: lib.func(
    'int32_t rp_render(const rp_request *request, const uint8_t *html, size_t len, _Out_ rp_result **out)'),
  rp_result_pdf: lib.func('const uint8_t *rp_result_pdf(const rp_result *result, _Out_ size_t *len)'),
  rp_result_warnings_json: lib.func(
    'const char *rp_result_warnings_json(const rp_result *result, _Out_ size_t *len)'),
  rp_result_error: lib.func('const char *rp_result_error(const rp_result *result, _Out_ size_t *len)'),
  rp_result_free: lib.func('void rp_result_free(rp_result *result)'),
  rp_request_free: lib.func('void rp_request_free(rp_request *request)')
}
