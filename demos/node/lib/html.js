// HTML from template literals, with every value escaped unless it is HTML itself, and the site's
// page around a page's content.

import { VERSION } from 'rustyprint'

// HTML that is not escaped again when it goes into more HTML.
class Html {
  constructor (text) {
    this.text = text
  }

  toString () {
    return this.text
  }
}

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

export function escape (text) {
  return String(text).replace(/[&<>"']/g, char => ESCAPES[char])
}

function fill (value) {
  if (value == null || value === false) return ''
  if (Array.isArray(value)) return value.map(fill).join('')
  if (value instanceof Html) return value.text
  return escape(value)
}

//   html`<p>${name}</p>`
//
// escapes `name`; a value made with html`` itself, or an array of them, goes in as it is.
export function html (strings, ...values) {
  return new Html(strings.reduce((out, string, i) => out + fill(values[i - 1]) + string))
}

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' }

// Text with its character references decoded: &amp; is &, &#233; and &#xe9; are é. Covers the
// named references the samples and asset URLs use; others are left as they are.
export function decode (text) {
  return text.replace(/&(?:#(\d+)|#x([\da-f]+)|([a-z]+));/gi, (reference, decimal, hex, name) => {
    if (decimal) return String.fromCodePoint(Number(decimal))
    if (hex) return String.fromCodePoint(parseInt(hex, 16))
    return ENTITIES[name.toLowerCase()] ?? reference
  })
}

// The site's page: its header and navigation around a page's content.
export function shell ({ title, content, scripts = '' }) {
  return html`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>${title}</title>
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <link rel="icon" href="/icon.png" type="image/png">
  <link rel="icon" href="/icon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/app.css">
</head>
<body>
  <header class="site-header">
    <a class="brand" href="/">rustyprint</a>
    <nav class="site-nav">
      <a href="/">Samples</a>
      <a href="/invoices">Invoices</a>
      <a href="/playground">Playground</a>
    </nav>
    <span class="tagline">HTML and CSS to PDF, rendered in Node.js by its own layout engine</span>
    <span class="version">v${VERSION}</span>
  </header>

  <main>
    ${content}
  </main>
  ${scripts}
</body>
</html>
`
}
