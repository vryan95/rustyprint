'use strict'

// Tries the packed packages as an app would install them, before a release publishes them.
//
//   node scripts/try.js setup <tarballs> <dir>   writes an app in <dir> that depends on rustyprint
//   (cd <dir> && npm install)
//   node scripts/try.js check <dir>              renders with it
//
// The app depends on rustyprint's tarball alone. Its `overrides` point each platform package at
// its tarball, since none is on npm yet, so npm picks the platform package by `os`, `cpu` and
// `libc`, as it will from npm. `check` fails unless exactly this machine's package was installed
// and both kinds of render work.

const assert = require('node:assert/strict')
const fs = require('node:fs')
const { createRequire } = require('node:module')
const path = require('node:path')
const { PLATFORMS } = require('../lib/platforms')
const { platformFor } = require('../lib/library')

const { version } = require('../package.json')

// A tarball's path as a `file:` specifier, with forward slashes on Windows too.
function file (tarball) {
  return `file:${path.resolve(tarball).split(path.sep).join('/')}`
}

function setup (tarballs, dir) {
  const tarball = name => path.join(tarballs, `${name.replace('@', '').replace('/', '-')}-${version}.tgz`)
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({
    private: true,
    dependencies: { rustyprint: file(tarball('rustyprint')) },
    overrides: Object.fromEntries(PLATFORMS.map(platform => [platform.name, file(tarball(platform.name))]))
  }, null, 2) + '\n')
}

async function check (dir) {
  // From the app, which has it as rustyprint's dependency: the checkout need not be installed.
  const appRequire = createRequire(path.resolve(dir, 'package.json'))
  const libc = process.platform === 'linux' ? appRequire('detect-libc').familySync() : null
  const expected = platformFor(process.platform, process.arch, libc)
  assert.ok(expected, `no rustyprint package for ${process.platform}-${process.arch} ${libc ?? ''}`)

  const scope = path.join(dir, 'node_modules/@rustyprint')
  assert.deepEqual(fs.existsSync(scope) ? fs.readdirSync(scope) : [], [expected.name.split('/')[1]],
    `npm should have installed ${expected.name} alone`)

  delete process.env.RUSTYPRINT_LIBRARY
  const rustyprint = appRequire('rustyprint')
  const { pdf } = await rustyprint.render('<h1>Hello</h1>')
  assert.equal(pdf.subarray(0, 5).toString('latin1'), '%PDF-')
  assert.ok(rustyprint.renderSync('<h1>Hello</h1>').pdf.equals(pdf))
  assert.equal(rustyprint.nativeVersion(), version)
  console.log(`${expected.name} ${version}: rendered ${pdf.length} bytes on ${process.platform}-${process.arch}` +
    (libc ? ` (${libc})` : ''))
}

const [command, ...args] = process.argv.slice(2)
if (command === 'setup' && args.length === 2) {
  setup(...args)
} else if (command === 'check' && args.length === 1) {
  check(...args).catch(error => {
    console.error(error)
    process.exitCode = 1
  })
} else {
  console.error('usage: node scripts/try.js setup <tarballs> <dir> | check <dir>')
  process.exitCode = 2
}
