import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { describe, it } from 'node:test'
import { Sample } from '../lib/samples.js'
import { bytes, demoApp, pdf } from './helpers.js'

describe('samples', () => {
  const app = demoApp()

  it('the home page lists every sample', async () => {
    const html = await app.text('/')
    assert.equal(html.match(/class="sample-card"/g).length, 5)
    for (const id of ['invoice', 'report', 'report-ar', 'report-zh', 'letter']) {
      assert.ok(html.includes(`href="/samples/${id}"`), id)
      assert.ok(html.includes(`href="/samples/${id}.pdf"`), id)
    }
  })

  it("a sample's page shows its PDF, how it renders in a browser and its source", async () => {
    const html = await app.text('/samples/report')
    assert.ok(html.includes('<h1>Quarterly report, Q3 2026</h1>'))
    assert.ok(html.includes('<iframe src="/samples/report.pdf"'))
    assert.ok(html.includes('<iframe src="/samples/report/files/index.html"'))
    assert.ok(html.includes('<dd>8 pages</dd>'))
    assert.ok(html.includes('charts/revenue.png'))
    assert.ok(html.includes('await render(html, { assets })'))
  })

  for (const [id, pages] of [['invoice', 3], ['report', 8], ['report-ar', 6], ['report-zh', 7], ['letter', 1]]) {
    it(`${id} renders to a PDF of ${pages} ${pages === 1 ? 'page' : 'pages'}`, async () => {
      const response = await app.get(`/samples/${id}.pdf`)
      assert.equal(response.status, 200)
      assert.equal(response.headers.get('content-type'), 'application/pdf')
      assert.match(response.headers.get('content-disposition'), new RegExp(`^inline; filename="${id}\\.pdf"`))
      const body = await bytes(response)
      assert.ok(pdf.is(body))
      assert.equal(pdf.pages(body), pages)
    })
  }

  it('a PDF can be downloaded', async () => {
    const response = await app.get('/samples/letter.pdf?download=1')
    assert.match(response.headers.get('content-disposition'), /^attachment; filename="letter\.pdf"/)
  })

  it("a sample's own files are served, for the browser preview", async () => {
    const page = await app.get('/samples/letter/files/index.html')
    assert.equal(page.status, 200)
    assert.match(page.headers.get('content-type'), /^text\/html/)

    const font = await bytes(await app.get('/samples/letter/files/fonts/Caveat.ttf'))
    assert.ok(font.equals(fs.readFileSync(path.join(Sample.find('letter').dir, 'fonts', 'Caveat.ttf'))))
  })

  for (const url of ['/samples/letter/files/..%2Finvoice%2Flogo.png', '/samples/letter/files/missing.css',
    '/samples/nope', '/samples/nope.pdf']) {
    it(`serves nothing outside a sample: ${url}`, async () => {
      assert.equal((await app.get(url)).status, 404)
      assert.equal(Sample.find('letter').asset('../invoice/logo.png'), null)
    })
  }
})
