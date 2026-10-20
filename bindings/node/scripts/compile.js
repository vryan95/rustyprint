'use strict'

// Builds the native library for this platform with Cargo and copies it into native/, as
// `rake compile` does for the gem. Does nothing when RUSTYPRINT_LIBRARY names a library.

const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')
const { fileName, NATIVE_DIR } = require('../lib/library')

if (process.env.RUSTYPRINT_LIBRARY) {
  console.log(`Using the native library at ${process.env.RUSTYPRINT_LIBRARY}`)
  process.exit(0)
}

const repo = path.resolve(__dirname, '../../..')
execFileSync('cargo', ['build', '--release', '--locked', '-p', 'rustyprint-ffi'], { cwd: repo, stdio: 'inherit' })

const target = process.env.CARGO_TARGET_DIR || path.join(repo, 'target')
fs.mkdirSync(NATIVE_DIR, { recursive: true })
fs.copyFileSync(path.join(target, 'release', fileName()), path.join(NATIVE_DIR, fileName()))
