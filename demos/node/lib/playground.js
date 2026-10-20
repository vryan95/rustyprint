// Edit HTML, CSS and the render options, and see the PDF and the warnings at once.

import crypto from 'node:crypto'
import express from 'express'
import { render, RustyprintError } from 'rustyprint'
import { pageCount, pageMillimetres } from './format.js'
import { html, shell } from './html.js'

export const PAGE_SIZES = ['A4', 'A5', 'A3', 'Letter', 'Legal']

// The form carries a token that must match the one in its cookie, so another site cannot post
// it: it can make a browser send the cookie, but cannot read it to put in the form.
const TOKEN_COOKIE = 'playground_token'

function newToken () {
  return crypto.randomBytes(32).toString('base64url')
}

function cookie (req, name) {
  for (const pair of (req.headers.cookie ?? '').split(';')) {
    const [key, ...value] = pair.trim().split('=')
    if (key === name) return value.join('=')
  }
  return ''
}

function sameToken (a, b) {
  const [left, right] = [Buffer.from(a), Buffer.from(b)]
  return left.length > 0 && left.length === right.length && crypto.timingSafeEqual(left, right)
}

export function mapPlayground (app) {
  app.get('/playground', (req, res) => {
    const token = newToken()
    res.cookie(TOKEN_COOKIE, token, { httpOnly: true, sameSite: 'strict', path: '/playground' })
    res.send(String(playgroundPage(EXAMPLE_HTML, EXAMPLE_CSS, token)))
  })

  // A body larger than this is refused with 413: the playground is for small documents.
  app.post('/playground', express.urlencoded({ extended: false, limit: '2mb' }), async (req, res) => {
    const form = req.body ?? {}
    if (!sameToken(cookie(req, TOKEN_COOKIE), form.token ?? '')) {
      return res.status(400).json({ error: 'The form has expired: reload the page.' })
    }

    const options = {
      stylesheet: form.css || undefined,
      pageSize: PAGE_SIZES.includes(form.page_size) ? form.page_size : undefined,
      landscape: form.landscape === '1',
      margin: form.margin || undefined,
      // A playground renders whatever it is given, so it gets tighter limits than the defaults.
      limits: { maxPages: 20, timeoutMs: 10000 }
    }
    try {
      const started = performance.now()
      const { pdf, warnings } = await render(form.html ?? '', options)
      res.json({
        pdf: pdf.toString('base64'),
        pages: pageCount(pdf),
        page_mm: pageMillimetres(pdf),
        milliseconds: Math.round(performance.now() - started),
        warnings
      })
    } catch (error) {
      if (!(error instanceof RustyprintError || error instanceof TypeError)) throw error
      res.status(422).json({ error: error.message })
    }
  })

  // Errors from the playground's route, such as a body over the limit, as JSON for its script.
  app.use('/playground', (error, req, res, next) => {
    res.status(error.status ?? 500).json({ error: error.message })
  })
}

function playgroundPage (htmlSource, css, token) {
  return shell({
    title: 'Playground · rustyprint',
    content: html`<section class="intro">
      <h1>Playground</h1>
      <p>
        Edit the HTML and CSS and choose the page options. The PDF renders as you type, with
        <code>render(html, { stylesheet: css, … })</code>. Images can be given as
        <code>data:</code> URLs.
      </p>
    </section>

    <form class="playground" method="post" action="/playground" data-playground>
      <input type="hidden" name="token" value="${token}">
      <div class="editors">
        <label>
          <span>HTML</span>
          <textarea name="html" spellcheck="false" rows="14">${htmlSource}</textarea>
        </label>
        <label>
          <span>CSS</span>
          <textarea name="css" spellcheck="false" rows="12">${css}</textarea>
        </label>
        <div class="options">
          <label>
            <span>Page size</span>
            <select name="page_size">
              ${PAGE_SIZES.map(size => html`<option${size === 'A4' ? html` selected` : ''}>${size}</option>`)}
            </select>
          </label>
          <label><span>Margin</span><input type="text" name="margin" placeholder="2cm"></label>
          <label class="check"><input type="checkbox" name="landscape" value="1"> Landscape</label>
          <button type="submit" class="button">Render</button>
          <p class="hint">An <code>@page</code> rule in the CSS that sets <code>size</code> or <code>margin</code> takes precedence over these options.</p>
        </div>
      </div>

      <div class="result">
        <p class="status" data-status aria-live="polite">Rendering…</p>
        <ul class="warnings" data-warnings hidden></ul>
        <iframe class="preview" data-preview title="The rendered PDF"></iframe>
      </div>
    </form>`,
    scripts: html`<script src="/playground.js"></script>`
  })
}

export const EXAMPLE_HTML = `<h1>Hello from the playground</h1>
<p>
  Edit the HTML and CSS, choose the page options, and the PDF on the right updates.
  Problems found in the CSS show as warnings below.
</p>
<table>
  <thead><tr><th>Item</th><th>Qty</th><th>Price</th></tr></thead>
  <tbody>
    <tr><td>Oak desk</td><td>2</td><td>€ 978.00</td></tr>
    <tr><td>Office chair</td><td>4</td><td>€ 1,316.00</td></tr>
    <tr><td>Desk lamp</td><td>4</td><td>€ 259.80</td></tr>
  </tbody>
</table>`

export const EXAMPLE_CSS = `@page {
  @bottom-right { content: "Page " counter(page) " of " counter(pages); font-size: 9pt; }
}
body { font-family: "Noto Sans", sans-serif; color: #222; }
h1 { color: #b7410e; }
table { width: 100%; border-collapse: collapse; }
th, td { padding: 6px 8px; border-bottom: 1px solid #ddd; text-align: left; }
th { background: #f4f1ea; }`
