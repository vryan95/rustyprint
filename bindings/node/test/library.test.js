'use strict'

const assert = require('node:assert/strict')
const path = require('node:path')
const { describe, it } = require('node:test')
const { NATIVE_DIR, platformFor, findLibrary } = require('../lib/library')
const { PLATFORMS } = require('../lib/platforms')

// A machine where only the given packages are installed, each under node_modules/<name>.
function machine (os, cpu, libc, installed = [], extra = {}) {
  return {
    env: {},
    os,
    cpu,
    libc,
    exists: () => false,
    resolve: request => {
      const name = request.replace(/\/package\.json$/, '')
      if (!installed.includes(name)) throw Object.assign(new Error(`Cannot find ${request}`), { code: 'MODULE_NOT_FOUND' })
      return path.join('/app/node_modules', name, 'package.json')
    },
    ...extra
  }
}

describe('platforms', () => {
  it('are the eight the native library is built for, with the names npm knows them by', () => {
    assert.deepEqual(PLATFORMS.map(({ target, name, os, cpu, libc }) => [target, name, os, cpu, libc]), [
      ['x86_64-unknown-linux-gnu', '@rustyprint/linux-x64-gnu', 'linux', 'x64', 'glibc'],
      ['aarch64-unknown-linux-gnu', '@rustyprint/linux-arm64-gnu', 'linux', 'arm64', 'glibc'],
      ['x86_64-unknown-linux-musl', '@rustyprint/linux-x64-musl', 'linux', 'x64', 'musl'],
      ['aarch64-unknown-linux-musl', '@rustyprint/linux-arm64-musl', 'linux', 'arm64', 'musl'],
      ['x86_64-apple-darwin', '@rustyprint/darwin-x64', 'darwin', 'x64', undefined],
      ['aarch64-apple-darwin', '@rustyprint/darwin-arm64', 'darwin', 'arm64', undefined],
      ['x86_64-pc-windows-msvc', '@rustyprint/win32-x64-msvc', 'win32', 'x64', undefined],
      ['aarch64-pc-windows-msvc', '@rustyprint/win32-arm64-msvc', 'win32', 'arm64', undefined]
    ])
  })

  it('are found by operating system, CPU and, on Linux, C library', () => {
    assert.equal(platformFor('linux', 'x64', 'glibc').name, '@rustyprint/linux-x64-gnu')
    assert.equal(platformFor('linux', 'arm64', 'musl').name, '@rustyprint/linux-arm64-musl')
    assert.equal(platformFor('darwin', 'arm64', null).name, '@rustyprint/darwin-arm64')
    assert.equal(platformFor('win32', 'arm64', null).name, '@rustyprint/win32-arm64-msvc')
    assert.equal(platformFor('linux', 'ia32', 'glibc'), undefined)
    assert.equal(platformFor('freebsd', 'x64', null), undefined)
  })
})

describe('the native library', () => {
  it('is RUSTYPRINT_LIBRARY when it is set', () => {
    const found = findLibrary(machine('linux', 'x64', 'glibc', ['@rustyprint/linux-x64-gnu'], {
      env: { RUSTYPRINT_LIBRARY: '/opt/rustyprint/librustyprint_ffi.so' },
      exists: () => true
    }))
    assert.equal(found, '/opt/rustyprint/librustyprint_ffi.so')
  })

  it('is the build in native/ in a checkout', () => {
    const found = findLibrary(machine('win32', 'x64', null, ['@rustyprint/win32-x64-msvc'], { exists: () => true }))
    assert.equal(found, path.join(NATIVE_DIR, 'rustyprint_ffi.dll'))
  })

  it("is in the platform's package once installed", () => {
    const found = findLibrary(machine('darwin', 'arm64', null, ['@rustyprint/darwin-arm64']))
    assert.equal(found, path.join('/app/node_modules/@rustyprint/darwin-arm64', 'librustyprint_ffi.dylib'))
  })

  it('is the one for the C library in use when both Linux packages are installed', () => {
    const both = ['@rustyprint/linux-x64-gnu', '@rustyprint/linux-x64-musl']
    assert.match(findLibrary(machine('linux', 'x64', 'musl', both)), /linux-x64-musl/)
    assert.match(findLibrary(machine('linux', 'x64', 'glibc', both)), /linux-x64-gnu/)
  })

  it('is in the other Linux package when the C library cannot be told or only that one is installed', () => {
    assert.match(findLibrary(machine('linux', 'arm64', null, ['@rustyprint/linux-arm64-musl'])), /linux-arm64-musl/)
    assert.match(findLibrary(machine('linux', 'arm64', 'musl', ['@rustyprint/linux-arm64-gnu'])), /linux-arm64-gnu/)
  })

  it('names the package to install when it is missing', () => {
    assert.throws(() => findLibrary(machine('linux', 'x64', 'musl')),
      error => /@rustyprint\/linux-x64-musl/.test(error.message) && /--omit=optional/.test(error.message) &&
        /npm\/cli\/issues\/4828/.test(error.message) && /RUSTYPRINT_LIBRARY/.test(error.message))
  })

  it('names the platforms it is built for on any other', () => {
    assert.throws(() => findLibrary(machine('freebsd', 'x64', null)),
      error => /freebsd-x64/.test(error.message) && /Windows on Arm/.test(error.message) &&
        /RUSTYPRINT_LIBRARY/.test(error.message))
  })
})
