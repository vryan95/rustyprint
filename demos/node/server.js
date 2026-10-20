// `npm start`: serves the demo on PORT, 3000 by default.

import { createApp } from './app.js'

const port = Number(process.env.PORT || 3000)
createApp().listen(port, () => console.log(`Listening on http://localhost:${port}`))
