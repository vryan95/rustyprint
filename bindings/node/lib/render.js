'use strict'

// Renders on the calling thread through the native library. renderSync uses it on the caller's
// thread, and the render workers on theirs.

const koffi = require('koffi')
const native = require('./native')
const { errorFor } = require('./errors')
const { snakeCase } = require('./options')

// rp_status, by value.
const STATUS = ['ok', 'invalid_argument', 'invalid_options', 'limit_exceeded', 'timeout', 'internal']

// Text as UTF-8 bytes: a string is encoded, and bytes are taken as they are.
function bytes (data, what) {
  if (typeof data === 'string') return Buffer.from(data, 'utf8')
  if (data instanceof ArrayBuffer) return new Uint8Array(data)
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
  throw new TypeError(`${what} must be a string, a Buffer, a typed array or an ArrayBuffer`)
}

function addAsset (request, path, data) {
  if (typeof path !== 'string' || path.includes('\0')) {
    throw new TypeError(`invalid asset path ${JSON.stringify(String(path))}`)
  }
  const content = bytes(data, `asset ${JSON.stringify(path)}`)
  const status = native.rp_request_add_asset(request, path, content, content.byteLength)
  if (status !== 0) throw new TypeError(`invalid asset path ${JSON.stringify(path)}`)
}

// What `read` makes of the result that `call` stores for a request, or the error it holds.
function resultOf (call, read) {
  const out = [null]
  const status = call(out)
  const result = out[0]
  if (result == null) throw errorFor('internal', 'the native library could not make a result')

  try {
    if (status !== 0) throw errorFor(STATUS[status], native.rp_result_error(result, null))
    return read(result)
  } finally {
    native.rp_result_free(result)
  }
}

function renderRequest (request, html) {
  return resultOf(out => native.rp_render(request, html, html.byteLength, out), result => {
    const len = [0]
    const pdf = native.rp_result_pdf(result, len)
    return {
      // A copy, since the PDF's memory goes with the result.
      pdf: koffi.view(pdf, Number(len[0])).slice(0),
      warnings: JSON.parse(native.rp_result_warnings_json(result, null))
    }
  })
}

// What `work` makes of the document as bytes and a native request with the options and assets.
function withRequest (html, options, work) {
  const { assets = {}, ...rest } = options ?? {}
  const document = bytes(html, 'html')
  const json = Buffer.from(JSON.stringify(snakeCase(rest)), 'utf8')

  const request = native.rp_request_new(json, json.byteLength)
  if (request == null) throw errorFor('internal', 'the native library could not make a request')

  try {
    const entries = assets instanceof Map ? assets.entries() : Object.entries(assets)
    for (const [path, data] of entries) addAsset(request, path, data)
    return work(request, document)
  } finally {
    native.rp_request_free(request)
  }
}

// Renders a document, with the PDF as an ArrayBuffer that can move to another thread.
function renderBytes (html, options) {
  return withRequest(html, options, renderRequest)
}

// The width in CSS pixels of a document's content in a browser window of the size `window`.
function contentWidthSync (html, window, options) {
  const { width, height } = window ?? {}
  if (typeof width !== 'number' || typeof height !== 'number') {
    throw new TypeError('window must be a width and a height in CSS pixels, such as { width: 1280, height: 720 }')
  }
  // A signal stops only work waiting for a worker, so it means nothing here.
  const { signal, ...rest } = options ?? {}
  return withRequest(html, rest, (request, document) => resultOf(
    out => native.rp_content_width(request, document, document.byteLength, width, height, out),
    result => native.rp_result_content_width(result)))
}

function renderSync (html, options) {
  // A signal stops only renders waiting for a worker, so it means nothing here.
  const { signal, ...rest } = options ?? {}
  const { pdf, warnings } = renderBytes(html, rest)
  return { pdf: Buffer.from(pdf), warnings }
}

function nativeVersion () {
  return native.rp_version()
}

module.exports = { renderBytes, renderSync, contentWidthSync, nativeVersion }
