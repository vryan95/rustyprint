import assert from 'node:assert/strict'
import { it } from 'node:test'
import { render, renderSync, createRenderer, nativeVersion, VERSION, RustyprintError } from '../index.js'

it('works from ES modules', async () => {
  const { pdf } = await render('<p>x</p>')
  assert.equal(pdf.subarray(0, 5).toString('latin1'), '%PDF-')
  assert.equal(renderSync('<p>x</p>').pdf.subarray(0, 5).toString('latin1'), '%PDF-')
  assert.equal(typeof createRenderer, 'function')
  assert.equal(nativeVersion(), VERSION)
  assert.equal(typeof RustyprintError, 'function')
})
