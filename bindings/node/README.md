# rustyprint for Node.js

Renders HTML and CSS to PDF with [rustyprint](https://github.com/vryan95/rustyprint), a layout engine of its own written in Rust. It needs no browser, no external runtime and no network access, and the fonts it needs are built in.

> This package is a proposal ([bindings/node](.)). It is not published to npm yet; see [Develop](#develop) to try it.

Node.js 22 or newer. It loads rustyprint's native library, the one the Ruby gem and the NuGet package use, through [koffi](https://koffi.dev).

## Render

```js
const fs = require('node:fs')
const { render } = require('rustyprint')

const { pdf, warnings } = await render(html, {
  pageSize: 'A4',
  margin: '15mm',
  title: 'Invoice 42',
  assets: {
    'logo.png': fs.readFileSync('logo.png'),
    'invoice.css': fs.readFileSync('app/pdf/invoice.css')
  }
})
fs.writeFileSync('invoice.pdf', pdf)
warnings // => ["invoice.css:12: unsupported property `colr`"]
```

`render` resolves to `pdf`, a Buffer, and `warnings`, problems found in the document's CSS and assets, as an array of strings. The HTML is a string, or UTF-8 bytes.

**Options** are the JSON options every rustyprint front end shares ([schema/render-options.schema.json](../../schema/render-options.schema.json)), with camelCase names: `pageSize`, `landscape`, `margin`, `title`, `stylesheet`, `remote` and `limits`. Names in snake_case, as in the schema, work too. An option left out keeps its default.

**Assets** are the files the document refers to by relative or root-relative URL, by the path it uses without a leading slash (`/css/app.css` is `css/app.css`): strings, Buffers, typed arrays or ArrayBuffers, in an object or a Map. Nothing else is read, and nothing is fetched from the network unless `remote.allowedHosts` allows the host.

## Threads

`render` runs each render on a worker thread, so the event loop keeps running meanwhile. It stays off libuv's thread pool too, which file system, DNS, crypto and zlib calls share. At most `os.availableParallelism()` renders run at once; the others wait their turn.

```js
const { createRenderer } = require('rustyprint')

const renderer = createRenderer({ concurrency: 2 })
const { pdf } = await renderer.render(html, { signal: AbortSignal.timeout(5000) })
await renderer.close()
```

`createRenderer` makes a renderer with its own workers. A `signal` stops a render that is still waiting for a worker; one that has started runs to the end, bounded by `limits.timeoutMs`.

`renderSync` renders on the calling thread and blocks the event loop until it is done, for scripts and command-line tools.

## Errors

Every failure is a `RustyprintError`, with a subclass and a `kind` for each kind: `InvalidOptionsError` (`'invalid_options'`), `LimitExceededError` (`'limit_exceeded'`), `TimeoutError` (`'timeout'`) and `InternalError` (`'internal'`, a bug in rustyprint). Arguments that are not valid, such as HTML bytes that are not UTF-8, are a `TypeError`.

## Develop

```sh
npm install
npm test
```

`npm test` builds the native library for this platform with Cargo, copies it into `native/`, then runs the tests against it. `npm run render-samples` then renders every document in [demos/samples](../../demos/samples) to `samples/`, all at once, and prints what each took. Set `RUSTYPRINT_LIBRARY` to use a library from elsewhere, such as a `rustyprint-native` release archive, without building one.

## License

MIT or Apache-2.0, at your option. The bundled Noto fonts are under the SIL Open Font License.
