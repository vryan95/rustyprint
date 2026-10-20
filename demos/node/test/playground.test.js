import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { EXAMPLE_CSS, EXAMPLE_HTML } from '../lib/playground.js'
import { demoApp, pdf } from './helpers.js'

describe('the playground', () => {
  const app = demoApp()

  // Posts the form as the page's script does, with the token from the page and its cookie.
  async function renderForm (fields) {
    const page = await app.get('/playground')
    const token = /name="token" value="([^"]+)"/.exec(await page.text())[1]
    const cookie = page.headers.get('set-cookie').split(';')[0]
    const response = await app.get('/playground', {
      method: 'POST',
      headers: { cookie },
      body: new URLSearchParams({ ...fields, token })
    })
    return { status: response.status, body: await response.json() }
  }

  it('starts with an example to edit', async () => {
    const html = await app.text('/playground')
    assert.ok(html.includes('<form class="playground"'))
    assert.match(html, /<textarea name="html"[^>]*>[^<]*&lt;h1&gt;/)
    assert.match(html, /<textarea name="css"[^>]*>[^<]*@page/)
    assert.ok(html.match(/<option/g).length >= 3)
    assert.ok(html.includes('class="preview"'))
  })

  it('returns the PDF, its pages and the warnings', async () => {
    const { status, body } = await renderForm({
      html: "<h1>Hello</h1><p style='colr: red'>x</p>",
      css: '@page { margin: 1cm } h1 { color: teal }',
      page_size: 'A5',
      landscape: '1',
      margin: ''
    })
    assert.equal(status, 200)
    assert.ok(pdf.is(Buffer.from(body.pdf, 'base64')))
    assert.equal(body.pages, 1)
    assert.ok(Number.isInteger(body.milliseconds))
    assert.ok(body.warnings.some(warning => warning.includes('colr')))
    assert.deepEqual(body.page_mm, [210, 148])
  })

  it('leaves the page size and margin of the example to the options', async () => {
    const example = { html: EXAMPLE_HTML, css: EXAMPLE_CSS }
    const plain = (await renderForm(example)).body
    const margin = (await renderForm({ ...example, margin: '5mm' })).body
    const letter = (await renderForm({ ...example, page_size: 'Letter' })).body
    assert.deepEqual(plain.page_mm, [210, 297])
    assert.notEqual(margin.pdf, plain.pdf)
    assert.deepEqual(letter.page_mm, [216, 279])
  })

  it('reports invalid options and documents over the limits, rather than failing', async () => {
    let { status, body } = await renderForm({ html: '<p>x</p>', margin: 'wide' })
    assert.equal(status, 422)
    assert.match(body.error, /margin/)

    ;({ status, body } = await renderForm({ html: "<div style='break-after: page'>x</div>".repeat(30) }))
    assert.equal(status, 422)
    assert.match(body.error, /page/)
  })

  it("needs the form's token", async () => {
    const response = await app.get('/playground', { method: 'POST', body: new URLSearchParams({ html: '<p>x</p>' }) })
    assert.equal(response.status, 400)
  })
})
