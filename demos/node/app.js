// The demo app: the samples, the invoices and the playground.

import express from 'express'
import { mapInvoices } from './lib/invoices.js'
import { mapPlayground } from './lib/playground.js'
import { mapSamples } from './lib/samples.js'
import { serveStaticFiles } from './lib/static-files.js'

export function createApp () {
  const app = express()
  app.disable('x-powered-by')
  serveStaticFiles(app)

  app.get('/up', (req, res) => res.status(200).end())
  mapSamples(app)
  mapInvoices(app)
  mapPlayground(app)
  return app
}
