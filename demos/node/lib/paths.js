// Where the demo finds the repository's files.

import fs from 'node:fs'
import path from 'node:path'

// The repository, found from the demo's directory by its Cargo.toml.
function findRepository () {
  for (let dir = import.meta.dirname; ; dir = path.dirname(dir)) {
    if (fs.existsSync(path.join(dir, 'Cargo.toml')) && fs.existsSync(path.join(dir, 'demos'))) return dir
    if (path.dirname(dir) === dir) throw new Error('The demo runs from inside the rustyprint repository.')
  }
}

export const REPOSITORY = findRepository()

// The demo's own directory.
export const DEMO = path.resolve(import.meta.dirname, '..')

// The shared sample documents, or RUSTYPRINT_SAMPLES if set.
export const SAMPLES = path.resolve(process.env.RUSTYPRINT_SAMPLES || path.join(REPOSITORY, 'demos', 'samples'))

// The app's own static files: its stylesheet, icons and the playground's script.
export const PUBLIC = path.join(DEMO, 'public')
