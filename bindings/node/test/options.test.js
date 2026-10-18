'use strict'

// The options are the JSON options of schema/render-options.schema.json, which every front end
// shares, with camelCase names.

const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const { describe, it } = require('node:test')
const rustyprint = require('..')
const { REPO } = require('./helpers')

const schema = JSON.parse(fs.readFileSync(path.join(REPO, 'schema/render-options.schema.json'), 'utf8'))

describe('render options', () => {
  for (const example of schema.examples) {
    it(`accepts the schema example ${JSON.stringify(example)}`, () => {
      assert.equal(rustyprint.renderSync('<p>x</p>', example).pdf.subarray(0, 5).toString('latin1'), '%PDF-')
    })
  }

  it('covers every option in the schema', () => {
    const used = new Set(schema.examples.flatMap(example => Object.keys(example)))
    assert.deepEqual([...used].sort(), Object.keys(schema.properties).sort())
  })

  it('takes camelCase and snake_case names alike', () => {
    const camel = rustyprint.renderSync('<p>x</p>', { pageSize: 'Letter', limits: { maxPages: 5 } })
    const snake = rustyprint.renderSync('<p>x</p>', { page_size: 'Letter', limits: { max_pages: 5 } })
    assert.ok(camel.pdf.equals(snake.pdf))
  })

  it('refuses what the schema does not allow', () => {
    assert.throws(() => rustyprint.renderSync('<p>x</p>', { landscape: 'yes' }), rustyprint.InvalidOptionsError)
    assert.throws(() => rustyprint.renderSync('<p>x</p>', { limits: { maxPages: -1 } }), rustyprint.InvalidOptionsError)
  })
})
