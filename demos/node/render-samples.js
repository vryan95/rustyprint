// `npm run render-samples [-- directory]`: renders every sample to a PDF, in the directory given
// or out/samples.

import path from 'node:path'
import { renderSamples } from './lib/samples.js'

await renderSamples(process.argv[2] ?? path.join('out', 'samples'))
