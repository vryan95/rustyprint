import { after, before } from 'node:test'
import { createApp } from '../app.js'

// The app on a port of its own for a test file's tests, with `get` and `post` against it.
export function demoApp () {
  const app = {}
  let server
  before(async () => {
    server = createApp().listen(0, '127.0.0.1')
    await new Promise(resolve => server.once('listening', resolve))
    app.base = `http://127.0.0.1:${server.address().port}`
  })
  after(() => new Promise(resolve => server.close(resolve)))

  app.get = (path, init) => fetch(app.base + path, init)
  app.text = async path => (await app.get(path)).text()
  return app
}

export const pdf = {
  is: bytes => Buffer.from(bytes).subarray(0, 5).toString('latin1') === '%PDF-',
  pages: bytes => (Buffer.from(bytes).toString('latin1').match(/\/MediaBox \[/g) ?? []).length,
  hasImage: bytes => /\/Subtype\s*\/Image/.test(Buffer.from(bytes).toString('latin1'))
}

export async function bytes (response) {
  return Buffer.from(await response.arrayBuffer())
}
