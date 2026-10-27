// The shared sample documents in demos/samples: each an index.html with the stylesheets, images
// and fonts it refers to by relative URL beside it.

import fs from 'node:fs'
import path from 'node:path'
import { render } from 'rustyprint'
import { pageCount, pages, size } from './format.js'
import { decode, html, shell } from './html.js'
import { SAMPLES } from './paths.js'
import { sendFile } from './pdf.js'

export class Sample {
  constructor (id, summary, features) {
    this.id = id
    this.summary = summary
    this.features = features
    this.dir = path.join(SAMPLES, id)
    // The files the document uses, by the relative path it uses for each: every file beside it.
    this.assetPaths = fs.readdirSync(this.dir, { recursive: true, withFileTypes: true })
      .filter(entry => entry.isFile())
      .map(entry => path.relative(this.dir, path.join(entry.parentPath, entry.name)).split(path.sep).join('/'))
      .filter(file => file !== 'index.html')
      .sort()
  }

  static find (id) {
    return CATALOG.find(sample => sample.id === id) ?? null
  }

  get html () {
    return fs.readFileSync(path.join(this.dir, 'index.html'), 'utf8')
  }

  get title () {
    return decode(/<title>([\s\S]*?)<\/title>/.exec(this.html)?.[1].trim() ?? this.id)
  }

  // An asset the document refers to by a relative path, or null for anything else.
  asset (file) {
    return this.assetPaths.includes(file) ? path.join(this.dir, file) : null
  }

  assets () {
    return Object.fromEntries(this.assetPaths.map(file => [file, fs.readFileSync(path.join(this.dir, file))]))
  }

  // The sample rendered to PDF, with what it took.
  async render () {
    const started = performance.now()
    const { pdf, warnings } = await render(this.html, { assets: this.assets() })
    return { pdf, warnings, milliseconds: performance.now() - started, pages: pageCount(pdf), bytes: pdf.length }
  }
}

// What each sample shows off, in the order the home page lists them.
export const CATALOG = [
  new Sample('invoice', 'A three-page invoice with a logo, a long table and totals.',
    ['Logo image', '40-line table whose header row repeats on every page', 'Page numbers in the page footer']),
  new Sample('report', 'An eight-page quarterly report with a title page, a table of contents and charts.',
    ['Title page with its own page style', 'Table of contents with page numbers and leaders',
      'Page headers and footers', 'Charts as images, and tables']),
  new Sample('report-ar', 'A six-page quarterly report in Arabic, laid out right to left.',
    ['Arabic text in the bundled Noto Naskh Arabic and Noto Sans Arabic, without @font-face',
      'Right-to-left tables, lists and page margin boxes', 'Arabic-Indic page numbers and list numbers',
      'Latin names within Arabic lines']),
  new Sample('report-zh', 'A seven-page quarterly report in Simplified Chinese, with a page of vertical text.',
    ['Chinese text in the bundled Noto Sans SC, without @font-face',
      "Chairman's letter set top to bottom with writing-mode: vertical-rl",
      'Page numbers as 第 1 页，共 7 页 and list numbers as 一、二、', 'Bar chart drawn with CSS']),
  new Sample('letter', 'A one-page letter signed in a handwriting web font.',
    ['Web font loaded with @font-face (Caveat)'])
]

// Renders every sample to a PDF in `dir`, and writes a line for each with what it took.
export async function renderSamples (dir, write = line => console.log(line)) {
  fs.mkdirSync(dir, { recursive: true })
  for (const sample of CATALOG) {
    const rendering = await sample.render()
    const file = `${sample.id}.pdf`
    fs.writeFileSync(path.join(dir, file), rendering.pdf)
    const count = String(rendering.pages).padStart(2)
    const unit = (rendering.pages === 1 ? 'page' : 'pages').padEnd(5)
    write(`${file.padEnd(14)} ${count} ${unit} ${String(Math.round(rendering.milliseconds)).padStart(5)} ms  ${size(rendering.bytes)}`)
    for (const warning of rendering.warnings) write(`  warning: ${warning}`)
  }
}

export function mapSamples (app) {
  app.get('/', (req, res) => res.send(String(samplesIndex())))

  // The sample rendered to PDF: inline, or as a download with ?download=1.
  app.get('/samples/:id.pdf', async (req, res) => {
    const sample = Sample.find(req.params.id)
    if (!sample) return res.sendStatus(404)
    sendFile(res, (await sample.render()).pdf, sample.id, { inline: !req.query.download })
  })

  // The sample's page: its PDF next to how the browser shows its HTML.
  app.get('/samples/:id', async (req, res) => {
    const sample = Sample.find(req.params.id)
    if (!sample) return res.sendStatus(404)
    res.send(String(sampleShow(sample, await sample.render())))
  })

  // The sample's document, as index.html, and its assets, so a browser can show the HTML as
  // rustyprint gets it.
  app.get('/samples/:id/files/*path', (req, res) => {
    const sample = Sample.find(req.params.id)
    if (!sample) return res.sendStatus(404)
    const file = req.params.path.join('/')
    if (file === 'index.html') return res.type('html').send(sample.html)
    const asset = sample.asset(file)
    if (!asset) return res.sendStatus(404)
    res.sendFile(asset)
  })
}

function samplesIndex () {
  return shell({
    title: 'Samples · rustyprint',
    content: html`<section class="intro">
      <h1>Sample documents</h1>
      <p>
        Each sample is an HTML page with its own stylesheet, images and fonts. This app passes them to
        <code>render</code> as assets and sends back the PDF. Open one to compare the PDF with how
        your browser shows the same HTML.
      </p>
    </section>

    <div class="sample-grid">
      ${CATALOG.map(sample => html`
      <article class="sample-card">
        <h2><a href="/samples/${sample.id}">${sample.title}</a></h2>
        <p>${sample.summary}</p>
        <ul class="features">
          ${sample.features.map(feature => html`<li>${feature}</li>`)}
        </ul>
        <div class="actions">
          <a href="/samples/${sample.id}" class="button">Compare</a>
          <a href="/samples/${sample.id}.pdf" class="button secondary" target="_blank">Open PDF</a>
        </div>
      </article>`)}
    </div>`
  })
}

function sampleShow (sample, rendering) {
  return shell({
    title: `${sample.title} · rustyprint`,
    content: html`<nav class="crumbs"><a href="/">All samples</a></nav>

    <section class="sample-header">
      <div>
        <h1>${sample.title}</h1>
        <p>${sample.summary}</p>
      </div>
      <div class="actions">
        <a href="/samples/${sample.id}.pdf" class="button" target="_blank">Open PDF</a>
        <a href="/samples/${sample.id}.pdf?download=1" class="button secondary">Download</a>
      </div>
    </section>

    <dl class="stats">
      <div class="stat"><dt>Pages</dt><dd>${pages(rendering.pages)}</dd></div>
      <div class="stat"><dt>Size</dt><dd>${size(rendering.bytes)}</dd></div>
      <div class="stat"><dt>Rendered in</dt><dd>${Math.round(rendering.milliseconds)} ms</dd></div>
      <div class="stat"><dt>Warnings</dt><dd>${rendering.warnings.length}</dd></div>
    </dl>

    ${rendering.warnings.length > 0 && html`<ul class="warnings">
      ${rendering.warnings.map(warning => html`<li>${warning}</li>`)}
    </ul>`}

    <div class="compare">
      <figure>
        <figcaption>PDF from rustyprint</figcaption>
        <iframe src="/samples/${sample.id}.pdf" title="${sample.title} as PDF"></iframe>
      </figure>
      <figure>
        <figcaption>The same HTML in your browser</figcaption>
        <iframe src="/samples/${sample.id}/files/index.html" title="${sample.title} as HTML"></iframe>
      </figure>
    </div>

    <section class="source">
      <h2>Source</h2>
      <p>
        <code>index.html</code>, rendered with these assets:
        ${sample.assetPaths.map(file => html`
        <a href="/samples/${sample.id}/files/${file}" class="asset" target="_blank">${file}</a>`)}
      </p>
      <details>
        <summary>Show the JavaScript that renders it</summary>
        <pre><code>${snippet(sample)}</code></pre>
      </details>
      <details>
        <summary>Show index.html</summary>
        <pre><code>${sample.html}</code></pre>
      </details>
    </section>`
  })
}

// The JavaScript an Express route would use to render the sample.
function snippet (sample) {
  return `const dir = path.join('samples', '${sample.id}')
const assets = {
${sample.assetPaths.map(file => `  '${file}': fs.readFileSync(path.join(dir, '${file}'))`).join(',\n')}
}
const html = fs.readFileSync(path.join(dir, 'index.html'), 'utf8')
const { pdf } = await render(html, { assets })
res.type('pdf').send(pdf)`
}
