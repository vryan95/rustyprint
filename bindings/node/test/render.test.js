'use strict'

const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const { setTimeout: sleep } = require('node:timers/promises')
const { describe, it } = require('node:test')
const rustyprint = require('..')
const { REPO, LOREM, LONG, pageSizes, invoice } = require('./helpers')

const { render, renderSync, createRenderer } = rustyprint

// Each test of a render runs it both ways: on a worker, and on the calling thread.
const BOTH = {
  render: (html, options) => render(html, options),
  renderSync: async (html, options) => renderSync(html, options)
}

for (const [name, run] of Object.entries(BOTH)) {
  describe(name, () => {
    it('renders HTML to a PDF', async () => {
      const output = await run('<h1>Hello</h1>')
      assert.ok(Buffer.isBuffer(output.pdf))
      assert.equal(output.pdf.subarray(0, 5).toString('latin1'), '%PDF-')
      assert.deepEqual(output.warnings, [])
      // A4 with the default options.
      assert.deepEqual(pageSizes(output.pdf), [[595.3, 841.9]])
    })

    it('takes the options in camelCase', async () => {
      const output = await run('<p>Hello</p>', { pageSize: 'A5', landscape: true, margin: '1cm', title: 'Hello' })
      assert.deepEqual(pageSizes(output.pdf), [[595.3, 419.5]])
    })

    it('takes nested options', async () => {
      const output = await run('<p>x</p>', {
        pageSize: { width: '100mm', height: '150mm' },
        limits: { maxPages: 5, timeoutMs: null }
      })
      assert.deepEqual(pageSizes(output.pdf), [[283.5, 425.2]])
    })

    it('uses assets by the path the document gives', async () => {
      const output = await run('<link rel="stylesheet" href="css/page.css"><p>x</p>', {
        assets: { 'css/page.css': '@page { size: 300px 200px }' }
      })
      assert.deepEqual(pageSizes(output.pdf), [[225, 150]])
    })

    it('takes assets as bytes and in a Map', async () => {
      const html = '<link rel="stylesheet" href="page.css">'
      const css = '@page { size: 400px 400px }'
      const fromBuffer = await run(html, { assets: { 'page.css': Buffer.from(css) } })
      const fromArrayBuffer = await run(html, { assets: { 'page.css': new TextEncoder().encode(css).buffer } })
      const fromMap = await run(html, { assets: new Map([['page.css', css]]) })
      for (const output of [fromBuffer, fromArrayBuffer, fromMap]) {
        assert.deepEqual(pageSizes(output.pdf), [[300, 300]])
      }
    })

    it('returns the warnings', async () => {
      const output = await run('<style>p { colör: red; colr: blue }</style><p>x</p>')
      assert.equal(output.warnings.length, 2)
      assert.ok(output.warnings.every(warning => typeof warning === 'string'))
      assert.match(output.warnings.join(), /colör/)
    })

    it('takes the HTML as UTF-8 bytes', async () => {
      const output = await run(Buffer.from('<p>Café</p>', 'utf8'))
      assert.equal(output.pdf.subarray(0, 5).toString('latin1'), '%PDF-')
    })

    it('refuses HTML bytes that are not UTF-8', async () => {
      await assert.rejects(run(Buffer.from([0x3c, 0x70, 0x3e, 0xff])), TypeError)
    })

    it('refuses an asset path with a NUL', async () => {
      await assert.rejects(run('<p>x</p>', { assets: { 'a\0b.css': 'p {}' } }), TypeError)
    })

    it('refuses HTML that is not text or bytes', async () => {
      await assert.rejects(run(42), TypeError)
    })

    it('throws InvalidOptionsError for options the renderer does not know', async () => {
      await assert.rejects(run('<p>x</p>', { colour: 'red' }),
        error => error instanceof rustyprint.InvalidOptionsError && /colour/.test(error.message))
    })

    it('throws InvalidOptionsError for invalid values', async () => {
      await assert.rejects(run('<p>x</p>', { pageSize: 'B99' }),
        error => error instanceof rustyprint.InvalidOptionsError && error.kind === 'invalid_options')
    })

    it('throws LimitExceededError', async () => {
      await assert.rejects(run('<p>More than ten bytes</p>', { limits: { maxInputBytes: 10 } }),
        error => error instanceof rustyprint.LimitExceededError && /input size/.test(error.message))
    })

    it('throws TimeoutError when the deadline passes', async () => {
      await assert.rejects(run(LOREM.repeat(5000), { limits: { timeoutMs: 1 } }),
        error => error instanceof rustyprint.TimeoutError && /deadline/.test(error.message))
    })
  })
}

describe('determinism', () => {
  it('gives the same bytes for the same input, on any thread', async () => {
    const { html, assets } = invoice()
    const first = renderSync(html, { assets }).pdf
    const again = renderSync(html, { assets }).pdf
    const onWorkers = await Promise.all(Array.from({ length: 4 }, () => render(html, { assets })))
    for (const pdf of [again, ...onWorkers.map(output => output.pdf)]) assert.ok(pdf.equals(first))
  })

  it('renders several documents at once', async () => {
    const outputs = await Promise.all(Array.from({ length: 4 }, (_, i) => render(`<p>${i}</p>`, { pageSize: 'A5' })))
    for (const output of outputs) assert.deepEqual(pageSizes(output.pdf), [[419.5, 595.3]])
  })
})

describe('threads', () => {
  it('keeps the event loop running while it renders', async () => {
    const ticks = []
    const timer = setInterval(() => ticks.push(performance.now()), 5)
    const started = performance.now()
    await render(LONG)
    const finished = performance.now()
    clearInterval(timer)

    // Any tick in the middle of the render proves the event loop ran meanwhile.
    assert.ok(ticks.some(tick => tick > started + 20 && tick < finished - 20),
      `no timer ran during the ${Math.round(finished - started)} ms render`)
  })

  it("leaves libuv's thread pool to file system calls while it renders", async () => {
    const renderer = createRenderer({ concurrency: 4 })
    try {
      // Start the four workers before timing anything.
      await Promise.all(Array.from({ length: 4 }, () => renderer.render('<p>x</p>')))

      let firstRender = Infinity
      const renders = Array.from({ length: 4 }, () =>
        renderer.render(LONG).then(() => { firstRender = Math.min(firstRender, performance.now()) }))
      await sleep(20)
      // Four reads would wait for a render to finish if each render held one of libuv's four
      // threads.
      await Promise.all(Array.from({ length: 4 }, () => fs.promises.readFile(path.join(REPO, 'Cargo.toml'))))
      const read = performance.now()
      await Promise.all(renders)

      assert.ok(read < firstRender, 'the reads waited for a render')
    } finally {
      await renderer.close()
    }
  })
})

describe('signals and closing', () => {
  it('refuses a render whose signal is already aborted', async () => {
    await assert.rejects(render('<p>x</p>', { signal: AbortSignal.abort() }), { name: 'AbortError' })
  })

  it('stops a render that has not started', async () => {
    const renderer = createRenderer({ concurrency: 1 })
    try {
      const controller = new AbortController()
      const first = renderer.render(LONG)
      const second = renderer.render('<p>x</p>', { signal: controller.signal })
      controller.abort()
      await assert.rejects(second, { name: 'AbortError' })
      assert.equal((await first).pdf.subarray(0, 5).toString('latin1'), '%PDF-')
    } finally {
      await renderer.close()
    }
  })

  it('refuses renders once closed', async () => {
    const renderer = createRenderer({ concurrency: 1 })
    // Both are refused while close() runs, so they are checked from the start.
    const running = assert.rejects(renderer.render(LONG))
    const queued = assert.rejects(renderer.render('<p>x</p>'), /closed/)
    await renderer.close()
    await queued
    await running
    await assert.rejects(renderer.render('<p>x</p>'), /closed/)
  })

  it('refuses a concurrency that is not a positive integer', () => {
    assert.throws(() => createRenderer({ concurrency: 0 }), RangeError)
    assert.throws(() => createRenderer({ concurrency: 1.5 }), RangeError)
  })
})

describe('errors', () => {
  it('makes every error a RustyprintError', () => {
    for (const Class of [rustyprint.InvalidOptionsError, rustyprint.LimitExceededError,
      rustyprint.TimeoutError, rustyprint.InternalError]) {
      const error = new Class('x')
      assert.ok(error instanceof rustyprint.RustyprintError)
      assert.ok(error instanceof Error)
      assert.equal(error.name, Class.name)
    }
  })
})

describe('versions', () => {
  it('matches the workspace version', () => {
    const cargo = fs.readFileSync(path.join(REPO, 'Cargo.toml'), 'utf8')
    assert.equal(rustyprint.VERSION, cargo.match(/^\[workspace\.package\]\s+version = "([^"]+)"/m)[1])
  })

  it("matches the native library's version", () => {
    assert.equal(rustyprint.nativeVersion(), rustyprint.VERSION)
  })
})
