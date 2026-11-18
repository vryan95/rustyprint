'use strict'

// A render worker: renders or measures each document the pool sends on this thread, and sends
// back the PDF, moved rather than copied, the width or the error.

const { parentPort, workerData } = require('node:worker_threads')

// The library the main thread found, so that every worker loads the same one without looking.
require('./library').useLibrary(workerData.library)
const { renderBytes, contentWidthSync } = require('./render')

parentPort.on('message', ({ html, options, window }) => {
  let reply
  let transfer
  try {
    if (window) {
      reply = { width: contentWidthSync(html, window, options) }
    } else {
      reply = renderBytes(html, options)
      transfer = [reply.pdf]
    }
  } catch (error) {
    const kind = error.kind ?? (error instanceof TypeError ? 'invalid_argument' : 'internal')
    reply = { error: { kind, message: error.message } }
  }
  parentPort.postMessage(reply, transfer)
})
