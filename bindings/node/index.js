'use strict'

// Renders HTML and CSS to PDF.
//
//   const { render } = require('rustyprint')
//
//   const { pdf, warnings } = await render(html, {
//     pageSize: 'A4',
//     margin: '15mm',
//     assets: { 'logo.png': fs.readFileSync('logo.png') }
//   })

const os = require('node:os')
const { Pool } = require('./lib/pool')
const {
  RustyprintError,
  InvalidOptionsError,
  LimitExceededError,
  TimeoutError,
  InternalError
} = require('./lib/errors')

// The package's version, the same as the native library's.
const VERSION = require('./package.json').version

let defaultRenderer = null

// Makes a renderer with its own render workers, at most `concurrency` of them, so that at most
// that many renders run at once; the others wait their turn.
function createRenderer ({ concurrency = os.availableParallelism() } = {}) {
  const pool = new Pool(concurrency)
  return {
    render (html, options) {
      const { signal, ...rest } = options ?? {}
      return pool.run(html, rest, signal)
    },
    close () {
      return pool.close()
    }
  }
}

// Renders an HTML document to PDF on a render worker, and resolves to the PDF and the warnings.
// The event loop keeps running meanwhile, and so do file system, DNS and crypto calls. Uses a
// renderer shared by the process, made on the first call.
function render (html, options) {
  defaultRenderer ??= createRenderer()
  return defaultRenderer.render(html, options)
}

// Renders an HTML document to PDF on the calling thread, which blocks the event loop until it
// is done. For scripts and command-line tools; servers should use render().
function renderSync (html, options) {
  return require('./lib/render').renderSync(html, options)
}

// The native library's version.
function nativeVersion () {
  return require('./lib/render').nativeVersion()
}

module.exports = {
  render,
  renderSync,
  createRenderer,
  nativeVersion,
  VERSION,
  RustyprintError,
  InvalidOptionsError,
  LimitExceededError,
  TimeoutError,
  InternalError
}
