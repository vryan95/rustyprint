'use strict'

// Lays out the npm packages a release publishes, ready for `npm pack`: one per platform with its
// native library, and rustyprint itself, depending on them all as optional dependencies.
//
//   node scripts/pack.js <libraries> <out>
//
// <libraries> holds each platform's library as <target>/<file>, as the release workflow unpacks
// the rustyprint-native archives. <out> gets a directory per package: rustyprint/,
// linux-x64-gnu/ and so on.
//
// The optional dependencies are written here rather than in package.json: a release raises the
// version before those packages exist on npm, and lockfiles made in this repository would
// otherwise depend on them.

const fs = require('node:fs')
const path = require('node:path')
const { PLATFORMS } = require('../lib/platforms')

const PACKAGE = path.resolve(__dirname, '..')
const REPO = path.resolve(PACKAGE, '../..')
const LICENSES = ['LICENSE-MIT', 'LICENSE-APACHE']

function readJson (file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'))
}

function writeJson (file, value) {
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n')
}

function copyLicenses (dir, { fonts = false } = {}) {
  for (const license of LICENSES) fs.copyFileSync(path.join(REPO, license), path.join(dir, license))
  // The fonts are built into the native library, so their licence goes with it.
  if (fonts) fs.copyFileSync(path.join(REPO, 'fonts/OFL.txt'), path.join(dir, 'OFL.txt'))
}

function layOutPlatform (main, platform, libraries, out) {
  const dir = path.join(out, platform.name.split('/')[1])
  fs.mkdirSync(dir, { recursive: true })
  fs.copyFileSync(path.join(libraries, platform.target, platform.library), path.join(dir, platform.library))
  copyLicenses(dir, { fonts: true })
  fs.writeFileSync(path.join(dir, 'README.md'), `# ${platform.name}

The native library of [rustyprint](https://www.npmjs.com/package/rustyprint) for ${platform.label}.
npm installs it along with \`rustyprint\` on that platform; there is no need to install it yourself.

MIT or Apache-2.0, at your option. The fonts built into the library are under the SIL Open Font
License (OFL.txt).
`)
  writeJson(path.join(dir, 'package.json'), {
    name: platform.name,
    version: main.version,
    description: `rustyprint's native library for ${platform.label}.`,
    license: main.license,
    // npm checks it against the repository the release workflow runs in.
    repository: main.repository,
    os: [platform.os],
    cpu: [platform.cpu],
    ...(platform.libc && { libc: [platform.libc] }),
    files: [platform.library, ...LICENSES, 'OFL.txt'],
    publishConfig: { access: 'public' }
  })
}

function layOutMain (main, out) {
  const dir = path.join(out, 'rustyprint')
  fs.mkdirSync(dir, { recursive: true })
  for (const file of ['index.js', 'index.d.ts', 'README.md']) {
    fs.copyFileSync(path.join(PACKAGE, file), path.join(dir, file))
  }
  fs.cpSync(path.join(PACKAGE, 'lib'), path.join(dir, 'lib'), { recursive: true })
  copyLicenses(dir)

  const { scripts, devDependencies, ...manifest } = main
  manifest.optionalDependencies = Object.fromEntries(PLATFORMS.map(platform => [platform.name, main.version]))
  writeJson(path.join(dir, 'package.json'), manifest)
}

function main ([libraries, out] = process.argv.slice(2)) {
  if (!libraries || !out) {
    console.error('usage: node scripts/pack.js <libraries> <out>')
    process.exit(2)
  }
  const manifest = readJson(path.join(PACKAGE, 'package.json'))
  for (const platform of PLATFORMS) layOutPlatform(manifest, platform, libraries, out)
  layOutMain(manifest, out)
}

main()
