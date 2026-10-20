import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { euros, INVOICES } from '../lib/invoice.js'
import { invoiceDocument } from '../lib/invoices.js'
import { collectAssets, renderPdf } from '../lib/pdf.js'
import { bytes, demoApp, pdf } from './helpers.js'

describe('invoices', () => {
  const app = demoApp()

  it('are listed', async () => {
    const html = await app.text('/invoices')
    for (const invoice of INVOICES) {
      assert.ok(html.includes(`href="/invoices/${invoice.id}"`), invoice.id)
      assert.ok(html.includes(`href="/invoices/${invoice.id}.pdf"`), invoice.id)
    }
    // The page shows its own routes, escaped.
    assert.ok(html.includes('app.get(&#39;/invoices/:id.pdf&#39;'))
  })

  it('show as HTML on their own', async () => {
    const html = await app.text(`/invoices/${INVOICES[0].id}`)
    assert.ok(html.includes('<h1>INVOICE</h1>'))
    assert.ok(html.includes('<link rel="stylesheet" href="/invoice/invoice.css"'))
    assert.ok(html.includes('<img src="/invoice/logo.png"'))
    assert.ok(!html.includes('site-header'))
    assert.equal((await app.get('/invoice/invoice.css')).status, 200)
  })

  it('render as a PDF with the stylesheet and logo from the static files', async () => {
    const invoice = INVOICES[0]
    const response = await app.get(`/invoices/${invoice.id}.pdf`)
    assert.equal(response.status, 200)
    assert.equal(response.headers.get('content-type'), 'application/pdf')
    assert.equal(response.headers.get('content-disposition'),
      `inline; filename="invoice ${invoice.id}.pdf"; filename*=UTF-8''invoice%20${invoice.id}.pdf`)
    const body = await bytes(response)
    assert.ok(pdf.is(body))
    assert.ok(pdf.hasImage(body), 'the logo is in the PDF')
  })

  it('the PDF view finds the files the HTML refers to', async () => {
    const html = invoiceDocument(INVOICES[0])
    assert.deepEqual(Object.keys(collectAssets(String(html))).sort(), ['invoice/invoice.css', 'invoice/logo.png'])
    assert.deepEqual((await renderPdf(html)).warnings, [])
  })

  for (const url of ['/invoices/nope', '/invoices/nope.pdf']) {
    it(`an unknown invoice is not found: ${url}`, async () => {
      assert.equal((await app.get(url)).status, 404)
    })
  }

  it('add up to the cent, as the .NET demo does', () => {
    const [first, second] = INVOICES
    assert.equal(first.lines.length, 36)
    assert.equal(second.lines.length, 4)
    for (const invoice of INVOICES) {
      assert.equal(invoice.total, invoice.subtotal + invoice.vat)
      assert.ok(Math.abs(invoice.vat - invoice.subtotal * 0.21) <= 0.5)
    }
    assert.equal(euros(131600), '€ 1,316.00')
    assert.equal(euros(5), '€ 0.05')
  })
})
