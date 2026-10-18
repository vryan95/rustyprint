// The app's static files: public/, and the shared invoice sample's stylesheet and logo under
// /invoice. The PDF views read the files the HTML refers to from the same places a browser
// gets them.

import fs from 'node:fs'
import path from 'node:path'
import express from 'express'
import { PUBLIC, SAMPLES } from './paths.js'

export const MOUNTS = [
  ['/invoice', path.join(SAMPLES, 'invoice')],
  ['', PUBLIC]
]

export function serveStaticFiles (app) {
  for (const [prefix, dir] of MOUNTS) app.use(prefix || '/', express.static(dir))
}

// The file at a root-relative path, such as /invoice/logo.png, or null if there is none.
export function readStaticFile (file) {
  for (const [prefix, dir] of MOUNTS) {
    if (!file.startsWith(`${prefix}/`)) continue
    const full = path.join(dir, file.slice(prefix.length))
    // AssetCollector never asks for a path with `..`, but the files stay in their directory anyway.
    if (!full.startsWith(dir + path.sep)) continue
    if (fs.statSync(full, { throwIfNoEntry: false })?.isFile()) return fs.readFileSync(full)
  }
  return null
}
