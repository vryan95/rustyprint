// Renders HTML to a PDF response, as Rails' `render pdf:` does a view:
//
//   app.get('/invoices/:id.pdf', async (req, res) => {
//     await sendPdf(res, `invoice ${id}`, invoiceDocument(invoice))
//   })
//
// The stylesheets, images and fonts the HTML refers to by path, such as /invoice/invoice.css,
// are read from the app's static files, and the warnings are logged.

import { render } from 'rustyprint'
import { AssetCollector } from './asset-collector.js'
import { readStaticFile } from './static-files.js'

// The static files the HTML and its stylesheets refer to, by the path the HTML uses.
export function collectAssets (html) {
  return new AssetCollector(readStaticFile).collect(html)
}

// Renders HTML with the files it refers to, besides the assets already in the options, which win.
export async function renderPdf (html, options = {}) {
  const assets = { ...collectAssets(String(html)), ...options.assets }
  const output = await render(String(html), { ...options, assets })
  for (const warning of output.warnings) console.warn(`rustyprint: ${warning}`)
  return output
}

// Sends a PDF, named `name` with .pdf added, shown in the browser or, unless `inline`, downloaded.
export function sendFile (res, pdf, name, { inline = true } = {}) {
  const file = name.toLowerCase().endsWith('.pdf') ? name : `${name}.pdf`
  res.set({
    'Content-Type': 'application/pdf',
    'Content-Disposition': `${inline ? 'inline' : 'attachment'}; filename="${file.replace(/["\\]/g, '_')}"; ` +
      `filename*=UTF-8''${encodeURIComponent(file)}`
  })
  res.send(pdf)
}

// Renders HTML to PDF and sends it.
export async function sendPdf (res, name, html, { options, inline } = {}) {
  const { pdf } = await renderPdf(html, options)
  sendFile(res, pdf, name, { inline })
}
