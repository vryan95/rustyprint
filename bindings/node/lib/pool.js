'use strict'

// Runs renders on worker threads, at most `concurrency` at once. A render never holds the event
// loop, nor libuv's thread pool, which file system, DNS, crypto and zlib calls share: a native
// call made with koffi's `.async` would hold one of its four threads for the whole render.

const path = require('node:path')
const { Worker } = require('node:worker_threads')
const { errorFor } = require('./errors')
const { libraryPath } = require('./library')

const WORKER = path.join(__dirname, 'worker.js')

class Pool {
  constructor (concurrency) {
    if (!Number.isInteger(concurrency) || concurrency < 1) {
      throw new RangeError(`concurrency must be a positive integer, not ${concurrency}`)
    }
    this.concurrency = concurrency
    this.workers = new Set()
    this.idle = []
    this.running = new Map()
    this.queue = []
    this.closed = false
  }

  // Renders a document on a worker. A signal stops a render that has not started; one that has
  // started runs to the end, bounded by the deadline in its options.
  run (html, options, signal) {
    return new Promise((resolve, reject) => {
      if (this.closed) return reject(closedError())
      if (signal?.aborted) return reject(signal.reason)

      const task = { message: { html, options }, resolve, reject, signal, onAbort: null }
      if (signal) {
        task.onAbort = () => {
          const index = this.queue.indexOf(task)
          if (index >= 0) {
            this.queue.splice(index, 1)
            reject(signal.reason)
          }
        }
        signal.addEventListener('abort', task.onAbort, { once: true })
      }
      this.queue.push(task)
      this.drain()
    })
  }

  // Stops every worker, and refuses the renders still queued and any made later.
  async close () {
    this.closed = true
    for (const task of this.queue.splice(0)) task.reject(closedError())
    await Promise.all([...this.workers].map(worker => worker.terminate()))
  }

  drain () {
    while (this.queue.length > 0) {
      let worker = this.idle.pop()
      if (!worker && this.workers.size < this.concurrency) {
        try {
          worker = this.spawn()
        } catch (error) {
          // No native library to render with: every render waiting fails the same way.
          for (const task of this.queue.splice(0)) {
            task.signal?.removeEventListener('abort', task.onAbort)
            task.reject(error)
          }
          return
        }
      }
      if (!worker) return
      this.start(worker, this.queue.shift())
    }
  }

  spawn () {
    const worker = new Worker(WORKER, { workerData: { library: libraryPath() } })
    this.workers.add(worker)
    worker.on('message', reply => this.finish(worker, reply))
    worker.on('error', error => this.fail(worker, error))
    worker.on('exit', code => this.fail(worker, new Error(`a render worker stopped with code ${code}`)))
    return worker
  }

  start (worker, task) {
    task.signal?.removeEventListener('abort', task.onAbort)
    try {
      worker.postMessage(task.message)
    } catch (error) {
      // Options or assets that cannot be copied to another thread.
      worker.unref()
      this.idle.push(worker)
      task.reject(error)
      return
    }
    this.running.set(worker, task)
    // A worker keeps the process alive only while it renders.
    worker.ref()
  }

  finish (worker, reply) {
    const task = this.running.get(worker)
    this.running.delete(worker)
    worker.unref()
    this.idle.push(worker)

    if (reply.error) task.reject(errorFor(reply.error.kind, reply.error.message))
    else task.resolve({ pdf: Buffer.from(reply.pdf), warnings: reply.warnings })
    this.drain()
  }

  fail (worker, error) {
    if (!this.workers.delete(worker)) return
    this.idle = this.idle.filter(other => other !== worker)
    const task = this.running.get(worker)
    this.running.delete(worker)
    task?.reject(error)
    if (!this.closed) this.drain()
  }
}

function closedError () {
  return new Error('the renderer is closed')
}

module.exports = { Pool }
