// The invoices: a template filled from data in the app, shown as HTML and rendered to PDF.

import fs from 'node:fs'
import { html, shell } from './html.js'
import { euros, INVOICES, Invoice, longDate, VAT_PERCENT } from './invoice.js'
import { sendPdf } from './pdf.js'

export function mapInvoices (app) {
  app.get('/invoices', (req, res) => {
    res.send(String(invoicesIndex(fs.readFileSync(import.meta.filename, 'utf8'))))
  })

  // The same document as a PDF. sendPdf renders it with the stylesheet and logo it links to,
  // read from the app's static files. Registered before /invoices/:id, which would match too.
  app.get('/invoices/:id.pdf', async (req, res) => {
    const invoice = Invoice.find(req.params.id)
    if (!invoice) return res.sendStatus(404)
    await sendPdf(res, `invoice ${invoice.id}`, invoiceDocument(invoice))
  })

  // The invoice document as HTML, as the PDF is made from it.
  app.get('/invoices/:id', (req, res) => {
    const invoice = Invoice.find(req.params.id)
    if (!invoice) return res.sendStatus(404)
    res.send(String(invoiceDocument(invoice)))
  })
}

// An invoice as a document of its own, for the browser and for rustyprint.
export function invoiceDocument (invoice) {
  return html`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Invoice ${invoice.id}</title>
  <link rel="stylesheet" href="/invoice/invoice.css">
</head>
<body>
  <header>
    <img src="/invoice/logo.png" alt="Northwind Trading Co.">
    <div>
      <h1>INVOICE</h1>
      <p class="number">No. ${invoice.id}<br>${longDate(invoice.date)}</p>
    </div>
  </header>

  <section class="parties">
    <div>
      <h2>Billed to</h2>
      <p>
        ${invoice.customer.name}${invoice.customer.address.map(line => html`<br>${line}`)}
      </p>
    </div>
    <div>
      <h2>Details</h2>
      <p>Order ${invoice.order}<br>Due ${longDate(invoice.due)}</p>
    </div>
  </section>

  <table class="lines">
    <thead>
      <tr>
        <th>#</th>
        <th>Description</th>
        <th class="num">Qty</th>
        <th class="num">Unit price</th>
        <th class="num">Amount</th>
      </tr>
    </thead>
    <tbody>
      ${invoice.lines.map((line, i) => html`
      <tr>
        <td>${i + 1}</td>
        <td>${line.description}<small>${line.detail}</small></td>
        <td class="num">${line.quantity}</td>
        <td class="num">${euros(line.unitPrice)}</td>
        <td class="num">${euros(line.amount)}</td>
      </tr>`)}
    </tbody>
  </table>

  <table class="totals">
    <tr><td>Subtotal</td><td class="num">${euros(invoice.subtotal)}</td></tr>
    <tr><td>VAT ${VAT_PERCENT}%</td><td class="num">${euros(invoice.vat)}</td></tr>
    <tr class="due"><td>Total due</td><td class="num">${euros(invoice.total)}</td></tr>
  </table>

  <section class="payment">
    <h2>Payment</h2>
    <p>
      Please transfer the total within 30 days to IBAN NL91 ABNA 0417 1643 00 (BIC ABNANL2A),
      quoting invoice ${invoice.id}.
    </p>
  </section>
</body>
</html>
`
}

function invoicesIndex (source) {
  return shell({
    title: 'Invoices · rustyprint',
    content: html`<section class="intro">
      <h1>Invoices from a template</h1>
      <p>
        These invoices are an HTML template, a JavaScript template literal filled from data in the
        app. The <code>.pdf</code> route renders it to HTML and passes it to rustyprint with the
        stylesheet and logo it links to, read from the app's static files.
      </p>
    </section>

    <div class="sample-grid">
      ${INVOICES.map(invoice => html`
      <article class="sample-card">
        <h2><a href="/invoices/${invoice.id}">Invoice ${invoice.id}</a></h2>
        <p>${invoice.customer.name} · ${invoice.lines.length} ${invoice.lines.length === 1 ? 'line' : 'lines'} · ${euros(invoice.total)}</p>
        <div class="actions">
          <a href="/invoices/${invoice.id}.pdf" class="button" target="_blank">Open PDF</a>
          <a href="/invoices/${invoice.id}" class="button secondary">View HTML</a>
        </div>
      </article>`)}
    </div>

    <section class="source">
      <h2>The routes</h2>
      <pre><code>${source}</code></pre>
    </section>`
  })
}
