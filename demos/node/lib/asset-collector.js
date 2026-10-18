// Finds the files an HTML document refers to, in its tags, its style attributes and blocks, and
// the stylesheets it loads, and reads each once, for the `assets` option. The same rules as the
// .NET demo's AssetCollector and the Ruby gem's.
//
// `read` gets a root-relative path, such as /css/app.css, decoded, and returns the file's
// contents, or null if there is none. The result maps the path the document uses, without its
// leading slash, to the contents. Remote and data: URLs are left to the renderer.

import { decode } from './html.js'

const URL_ATTRIBUTES = ['src', 'poster', 'data']

const TAG = /<([a-zA-Z][\w-]*)((?:\s+[^\s=>/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*\/?>/g
const ATTRIBUTE = /([^\s=>/]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s>]+))?/g
const STYLE_BLOCK = /<style\b[^>]*>([\s\S]*?)<\/style>/gi
const CSS_COMMENT = /\/\*[\s\S]*?\*\//g
// url(...), quoted or not, in groups 1 to 3, and @import "...", in groups 4 and 5.
const CSS_URL = /url\(\s*(?:"([^"]*)"|'([^']*)'|([^)\s]*))\s*\)|@import\s+(?:"([^"]*)"|'([^']*)')/gi
const SCHEME = /^[a-z][a-z\d+.-]*:/i

export class AssetCollector {
  constructor (read) {
    this.read = read
  }

  // The files the document refers to, by the path it uses for each.
  collect (html) {
    this.assets = new Map()
    this.seen = new Set()
    for (const [, css] of html.matchAll(STYLE_BLOCK)) this.collectCss(css)
    for (const [, name, text] of html.matchAll(TAG)) this.collectTag(name.toLowerCase(), text)
    return Object.fromEntries(this.assets)
  }

  collectTag (name, text) {
    const attributes = new Map()
    for (const [, key, value = ''] of text.matchAll(ATTRIBUTE)) {
      // The first of an attribute given twice counts, as in a browser.
      const attribute = key.toLowerCase()
      if (!attributes.has(attribute)) attributes.set(attribute, decode(value.replace(/^["']+|["']+$/g, '')))
    }
    for (const key of URL_ATTRIBUTES) {
      if (attributes.has(key)) this.add(attributes.get(key))
    }
    if (attributes.has('srcset')) {
      for (const candidate of attributes.get('srcset').split(',')) this.add(candidate.trim().split(' ')[0])
    }
    if (attributes.has('style')) this.collectCss(attributes.get('style'))
    if (attributes.has('href')) {
      const rel = (attributes.get('rel') ?? '').split(/\s+/).map(token => token.toLowerCase())
      if (name === 'link' && rel.includes('stylesheet')) {
        this.add(attributes.get('href'), { stylesheet: true })
      } else if (name === 'image' || name === 'use') {
        this.add(attributes.get('href'))
      }
    }
  }

  collectCss (css) {
    for (const match of css.replace(CSS_COMMENT, '').matchAll(CSS_URL)) {
      const imported = match[4] ?? match[5]
      if (imported !== undefined) this.add(imported, { stylesheet: true })
      else this.add(match[1] ?? match[2] ?? match[3])
    }
  }

  add (url, { stylesheet = false } = {}) {
    const path = assetPath(url)
    if (path === null || this.seen.has(path)) return
    // Seen before it is read, so a file that is not there is asked for once.
    this.seen.add(path)

    const key = unescape(path)
    const data = this.read(`/${key}`)
    if (data == null) return

    this.assets.set(key, data)
    if (stylesheet || key.toLowerCase().endsWith('.css')) this.collectCss(Buffer.from(data).toString('utf8'))
  }
}

// The URL's path from the document's root, as the renderer resolves it, or null for remote,
// data: and fragment-only URLs and paths that leave the root.
function assetPath (url) {
  url = url.trim().split(/[?#]/)[0]
  if (url === '' || url.startsWith('//') || url.startsWith('\\') || SCHEME.test(url)) return null

  const segments = url.split('/').filter(segment => segment !== '' && segment !== '.')
  return segments.length === 0 || segments.includes('..') ? null : segments.join('/')
}

// `my%20logo.png` as `my logo.png`, and a malformed escape left as it is.
function unescape (path) {
  try {
    return decodeURIComponent(path)
  } catch {
    return path
  }
}
