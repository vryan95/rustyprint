'use strict'

const assert = require('node:assert/strict')
const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const { after, before, describe, it } = require('node:test')
const { PLATFORMS } = require('../lib/platforms')
const manifest = require('../package.json')

const SCRIPT = path.join(__dirname, '../scripts/pack.js')

describe('the packages a release publishes', () => {
  let work
  let out

  // Lays out the packages from a stand-in library per platform.
  before(() => {
    work = fs.mkdtempSync(path.join(os.tmpdir(), 'rustyprint-pack-'))
    const libraries = path.join(work, 'libraries')
    out = path.join(work, 'out')
    for (const platform of PLATFORMS) {
      fs.mkdirSync(path.join(libraries, platform.target), { recursive: true })
      fs.writeFileSync(path.join(libraries, platform.target, platform.library), platform.target)
    }
    execFileSync(process.execPath, [SCRIPT, libraries, out])
  })

  after(() => fs.rmSync(work, { recursive: true, force: true }))

  const read = (dir, file = 'package.json') => fs.readFileSync(path.join(out, dir, file), 'utf8')

  it('are rustyprint and one per platform', () => {
    assert.deepEqual(fs.readdirSync(out).sort(), [
      'darwin-arm64', 'darwin-x64', 'linux-arm64-gnu', 'linux-arm64-musl', 'linux-x64-gnu',
      'linux-x64-musl', 'rustyprint', 'win32-arm64-msvc', 'win32-x64-msvc'
    ])
  })

  for (const platform of PLATFORMS) {
    const dir = platform.name.split('/')[1]

    it(`${platform.name} holds the library for ${platform.target}, for npm to install only there`, () => {
      const pkg = JSON.parse(read(dir))
      assert.equal(pkg.name, platform.name)
      assert.equal(pkg.version, manifest.version)
      assert.deepEqual(pkg.os, [platform.os])
      assert.deepEqual(pkg.cpu, [platform.cpu])
      assert.deepEqual(pkg.libc, platform.libc && [platform.libc])
      assert.deepEqual(pkg.repository, manifest.repository)
      assert.equal(pkg.license, manifest.license)
      assert.deepEqual(pkg.publishConfig, { access: 'public' })
      assert.deepEqual(pkg.files, [platform.library, 'LICENSE-MIT', 'LICENSE-APACHE', 'OFL.txt'])
      for (const file of pkg.files) assert.ok(fs.existsSync(path.join(out, dir, file)), `${file} is missing`)
      assert.equal(read(dir, platform.library), platform.target)
    })
  }

  it('rustyprint depends on every platform package at its own version', () => {
    const pkg = JSON.parse(read('rustyprint'))
    assert.equal(pkg.version, manifest.version)
    assert.deepEqual(pkg.optionalDependencies,
      Object.fromEntries(PLATFORMS.map(platform => [platform.name, manifest.version])))
    assert.deepEqual(pkg.dependencies, manifest.dependencies)
    assert.equal(pkg.scripts, undefined)
    assert.equal(pkg.devDependencies, undefined)
    for (const file of ['index.js', 'index.d.ts', 'lib/library.js', 'LICENSE-MIT', 'LICENSE-APACHE']) {
      assert.ok(fs.existsSync(path.join(out, 'rustyprint', file)), `${file} is missing`)
    }
  })
})

describe('package.json', () => {
  it('does not ship a native library of its own', () => {
    assert.ok(!manifest.files.some(file => file.startsWith('native')))
    assert.equal(manifest.optionalDependencies, undefined)
  })
})
