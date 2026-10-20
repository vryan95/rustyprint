import assert from 'node:assert/strict'
import { beforeEach, describe, it } from 'node:test'
import { AssetCollector } from '../lib/asset-collector.js'

const FILES = {
  '/css/app.css': '@import "/css/base.css"; h1 { background: url("/images/bg.png") }',
  '/css/base.css': "@font-face { font-family: X; src: url(/fonts/x.ttf) format('truetype') }",
  '/images/bg.png': 'png',
  '/fonts/x.ttf': 'ttf',
  '/images/logo.png': 'logo',
  '/images/photo.jpg': 'jpg',
  '/hero.png': 'hero',
  '/my logo.png': 'spaced'
}

describe('AssetCollector', () => {
  let requested

  beforeEach(() => {
    requested = []
  })

  function collect (html) {
    const assets = new AssetCollector(file => {
      requested.push(file)
      return file in FILES ? Buffer.from(FILES[file]) : null
    }).collect(html)
    return Object.fromEntries(Object.entries(assets).map(([key, data]) => [key, data.toString()]))
  }

  it('collects what the HTML and its stylesheets refer to, by the path the document uses', () => {
    const assets = collect(`
      <link rel="stylesheet" href="/css/app.css?v=1">
      <img src='/images/logo.png' alt="">
      <img srcset="/images/photo.jpg 1x, /images/photo.jpg 2x">
      <div style="background: url(&quot;hero.png&quot;)"></div>
      <img src="/my%20logo.png">`)
    assert.deepEqual(assets, {
      'css/app.css': FILES['/css/app.css'],
      'css/base.css': FILES['/css/base.css'],
      'images/bg.png': 'png',
      'fonts/x.ttf': 'ttf',
      'images/logo.png': 'logo',
      'images/photo.jpg': 'jpg',
      'hero.png': 'hero',
      'my logo.png': 'spaced'
    })
  })

  it('fetches each file once', () => {
    collect('<img src="/hero.png"><img src="hero.png"><img src="./hero.png#x">')
    assert.deepEqual(requested, ['/hero.png'])
  })

  it('leaves remote, data:, fragment and escaping URLs to the renderer', () => {
    const assets = collect(`
      <img src="https://cdn.example.com/a.png"><img src="//cdn.example.com/b.png">
      <img src="data:image/png;base64,AAAA"><a href="#top">top</a><a href="mailto:x@example.com">x</a>
      <img src="/../secret.png"><img src="/">`)
    assert.deepEqual(assets, {})
    assert.deepEqual(requested, [])
  })

  it('skips files that cannot be found', () => {
    assert.deepEqual(Object.keys(collect('<img src="/missing.png"><img src="/hero.png">')), ['hero.png'])
  })

  it('skips links that are not resources', () => {
    assert.deepEqual(collect('<a href="/invoices/1">Invoice</a><link rel="canonical" href="/hero.png">'), {})
    assert.deepEqual(requested, [])
  })
})
