import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { it } from 'node:test'
import { CATALOG, renderSamples } from '../lib/samples.js'
import { pdf } from './helpers.js'

it('render-samples writes every sample to a PDF', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'rustyprint-samples-'))
  try {
    const lines = []
    await renderSamples(dir, line => lines.push(line))
    for (const sample of CATALOG) {
      const file = fs.readFileSync(path.join(dir, `${sample.id}.pdf`))
      assert.ok(pdf.is(file), sample.id)
      const pages = pdf.pages(file)
      assert.match(lines.join('\n'), new RegExp(`${sample.id}\\.pdf\\s+${pages} ${pages === 1 ? 'page' : 'pages'}`))
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true })
  }
})
