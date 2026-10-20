'use strict'

// Where the native library is. The main thread looks for it once and hands the path to each
// render worker, so a missing library fails each render with one clear error instead of each
// worker dying in turn.

const fs = require('node:fs')
const path = require('node:path')
const { PLATFORMS } = require('./platforms')

// The directory a checkout of the repository keeps its build of the library in, which
// `npm run compile` fills. The published package has no such directory.
const NATIVE_DIR = path.join(__dirname, '..', 'native')

// The native library's file name on a platform.
function fileName (platform = process.platform) {
  switch (platform) {
    case 'win32': return 'rustyprint_ffi.dll'
    case 'darwin': return 'librustyprint_ffi.dylib'
    default: return 'librustyprint_ffi.so'
  }
}

// The platform for an operating system, a CPU and, on Linux, a C library ('glibc' or 'musl').
function platformFor (os, cpu, libc) {
  return PLATFORMS.find(platform =>
    platform.os === os && platform.cpu === cpu && (os !== 'linux' || platform.libc === libc))
}

// The packages that may hold the library for this machine, the likeliest first. On Linux that
// is the one for the C library in use, then the other: Bun, and npm before 11.11 reinstalling
// from a lockfile, install both whatever the machine, so which one is installed proves nothing.
function packagesFor (os, cpu, libc) {
  if (os !== 'linux') return PLATFORMS.filter(platform => platform.os === os && platform.cpu === cpu)
  const order = libc === 'musl' ? ['musl', 'glibc'] : ['glibc', 'musl']
  return order.map(each => platformFor(os, cpu, each)).filter(Boolean)
}

// The C library this process runs on, on Linux: 'glibc', 'musl', or null if it cannot tell.
function currentLibc () {
  return process.platform === 'linux' ? require('detect-libc').familySync() : null
}

// Finds the library: RUSTYPRINT_LIBRARY if set, else the build in native/, else the library in
// this platform's package. Everything it consults can be given, for the tests.
function findLibrary ({
  env = process.env,
  os = process.platform,
  cpu = process.arch,
  libc = currentLibc(),
  exists = fs.existsSync,
  resolve = require.resolve
} = {}) {
  if (env.RUSTYPRINT_LIBRARY) return env.RUSTYPRINT_LIBRARY

  const build = path.join(NATIVE_DIR, fileName(os))
  if (exists(build)) return build

  const packages = packagesFor(os, cpu, libc)
  for (const platform of packages) {
    let manifest
    try {
      manifest = resolve(`${platform.name}/package.json`)
    } catch {
      continue
    }
    return path.join(path.dirname(manifest), platform.library)
  }
  throw missing(os, cpu, libc, packages)
}

function missing (os, cpu, libc, packages) {
  const machine = `${os}-${cpu}${libc ? ` (${libc})` : ''}`
  if (packages.length === 0) {
    return new Error(`rustyprint has no native library for ${machine}. It is built for ` +
      `${PLATFORMS.map(platform => platform.label).join(', ')}. Elsewhere, build the library with ` +
      '`cargo build --release -p rustyprint-ffi` and set RUSTYPRINT_LIBRARY to its path.')
  }
  return new Error(`rustyprint's native library for ${machine} is in the package ${packages[0].name}, ` +
    'which npm installs with rustyprint as an optional dependency, but it is not installed. ' +
    'Optional dependencies are left out when installing with --omit=optional or --no-optional, ' +
    'and when node_modules or the lockfile were made on another platform ' +
    '(https://github.com/npm/cli/issues/4828): remove both and install again. Or set ' +
    'RUSTYPRINT_LIBRARY to the library in a rustyprint-native release archive.')
}

let found = null

// The library's path, found on first use.
function libraryPath () {
  found ??= findLibrary()
  return found
}

// Uses the library the main thread found, in a render worker.
function useLibrary (file) {
  found = file
}

module.exports = { NATIVE_DIR, fileName, platformFor, packagesFor, findLibrary, libraryPath, useLibrary }
