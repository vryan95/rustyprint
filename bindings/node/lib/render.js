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

function renderRequest (request, html) {
  const out = [null]
  const status = native.rp_render(request, html, html.byteLength, out)
  const result = out[0]
  if (result == null) throw errorFor('internal', 'the native library could not make a result')

  try {
    if (status !== 0) throw errorFor(STATUS[status], native.rp_result_error(result, null))

    const len = [0]
    const pdf = native.rp_result_pdf(result, len)
    return {
      // A copy, since the PDF's memory goes with the result.
      pdf: koffi.view(pdf, Number(len[0])).slice(0),
      warnings: JSON.parse(native.rp_result_warnings_json(result, null))
    }
  } finally {
    native.rp_result_free(result)
  }
}

// Renders a document, with the PDF as an ArrayBuffer that can move to another thread.
function renderBytes (html, options) {
  const { assets = {}, ...rest } = options ?? {}
  const document = bytes(html, 'html')
  const json = Buffer.from(JSON.stringify(snakeCase(rest)), 'utf8')

  const request = native.rp_request_new(json, json.byteLength)
  if (request == null) throw errorFor('internal', 'the native library could not make a request')

  try {
    const entries = assets instanceof Map ? assets.entries() : Object.entries(assets)
    for (const [path, data] of entries) addAsset(request, path, data)
    return renderRequest(request, document)
  } finally {
    native.rp_request_free(request)
  }
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

module.exports = { renderBytes, renderSync, nativeVersion }
