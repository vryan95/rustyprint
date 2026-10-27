# rustyprint for Node.js

Renders HTML and CSS to PDF with [rustyprint](https://github.com/vryan95/rustyprint), a layout engine of its own written in Rust. It needs no browser, no external runtime and no network access, and the fonts it needs are built in.

```sh
npm install rustyprint
```

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

`import { render } from 'rustyprint'` works the same from ES modules, and the package has TypeScript types.

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

## Platforms

The native library, about 31 MB with its fonts, comes in a package of its own for each platform. `rustyprint` lists them all as optional dependencies, and npm installs only the one whose operating system, CPU and C library match the machine:

| Platform | Package |
| --- | --- |
| Linux x64 and arm64, glibc 2.17 or newer | `@rustyprint/linux-x64-gnu`, `@rustyprint/linux-arm64-gnu` |
| Linux x64 and arm64, musl (Alpine) | `@rustyprint/linux-x64-musl`, `@rustyprint/linux-arm64-musl` |
| macOS on Intel and on Apple silicon | `@rustyprint/darwin-x64`, `@rustyprint/darwin-arm64` |
| Windows x64 and on Arm | `@rustyprint/win32-x64-msvc`, `@rustyprint/win32-arm64-msvc` |

If it cannot find the library, rustyprint says which package it expected. The usual reasons:

- **Optional dependencies left out**, by `npm install --omit=optional` or `--no-optional`.
- **`node_modules` or the lockfile from another platform**, such as a lockfile made on macOS and installed in a Linux container by npm before 11.3 ([npm/cli#4828](https://github.com/npm/cli/issues/4828)). Remove both and install again, or install inside the container.
- **A Docker image built on one platform and run on another**: install the dependencies in the image's own platform.

Bun, and npm before 11.11 installing from a lockfile, install both Linux packages, glibc and musl. That does no harm: rustyprint uses the one for the C library it runs on.

koffi has an install script that only checks its prebuilt binary, and npm 12, pnpm and Bun skip dependencies' install scripts unless allowed. rustyprint works either way.

`RUSTYPRINT_LIBRARY` overrides it all: the path of a native library to use, such as one from a `rustyprint-native` archive on the [Releases page](https://github.com/vryan95/rustyprint/releases) or one built with `cargo build --release -p rustyprint-ffi`.

## Develop

```sh
npm install
npm test
npm run check-types
```

`npm test` builds the native library for this platform with Cargo, copies it into `native/`, then runs the tests against it; with `RUSTYPRINT_LIBRARY` set, it uses that library instead. `npm run compile` builds the library on its own, for [demos/node](../../demos/node), an Express app that uses the package.

The release workflow publishes the platform packages from the `rustyprint-native` archives, with [scripts/pack.js](scripts/pack.js), and tries each on a machine of its platform with [scripts/try.js](scripts/try.js) before publishing.

## License

MIT or Apache-2.0, at your option. The bundled Noto fonts are under the SIL Open Font License.
