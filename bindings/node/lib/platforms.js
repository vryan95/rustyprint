'use strict'

// The platforms the native library is built for, the targets of the `build` job in
// .github/workflows/release.yml, each with the npm package that holds its library. The package
// lists them as optional dependencies, and npm installs only the one whose `os`, `cpu` and
// `libc` match the machine.
const PLATFORMS = [
  {
    target: 'x86_64-unknown-linux-gnu',
    name: '@rustyprint/linux-x64-gnu',
    os: 'linux',
    cpu: 'x64',
    libc: 'glibc',
    library: 'librustyprint_ffi.so',
    label: 'Linux x64 with glibc 2.17 or newer'
  },
  {
    target: 'aarch64-unknown-linux-gnu',
    name: '@rustyprint/linux-arm64-gnu',
    os: 'linux',
    cpu: 'arm64',
    libc: 'glibc',
    library: 'librustyprint_ffi.so',
    label: 'Linux arm64 with glibc 2.17 or newer'
  },
  {
    target: 'x86_64-unknown-linux-musl',
    name: '@rustyprint/linux-x64-musl',
    os: 'linux',
    cpu: 'x64',
    libc: 'musl',
    library: 'librustyprint_ffi.so',
    label: 'Linux x64 with musl, such as Alpine'
  },
  {
    target: 'aarch64-unknown-linux-musl',
    name: '@rustyprint/linux-arm64-musl',
    os: 'linux',
    cpu: 'arm64',
    libc: 'musl',
    library: 'librustyprint_ffi.so',
    label: 'Linux arm64 with musl, such as Alpine'
  },
  {
    target: 'x86_64-apple-darwin',
    name: '@rustyprint/darwin-x64',
    os: 'darwin',
    cpu: 'x64',
    library: 'librustyprint_ffi.dylib',
    label: 'macOS on Intel'
  },
  {
    target: 'aarch64-apple-darwin',
    name: '@rustyprint/darwin-arm64',
    os: 'darwin',
    cpu: 'arm64',
    library: 'librustyprint_ffi.dylib',
    label: 'macOS on Apple silicon'
  },
  {
    target: 'x86_64-pc-windows-msvc',
    name: '@rustyprint/win32-x64-msvc',
    os: 'win32',
    cpu: 'x64',
    library: 'rustyprint_ffi.dll',
    label: 'Windows x64'
  },
  {
    target: 'aarch64-pc-windows-msvc',
    name: '@rustyprint/win32-arm64-msvc',
    os: 'win32',
    cpu: 'arm64',
    library: 'rustyprint_ffi.dll',
    label: 'Windows on Arm'
  }
]

module.exports = { PLATFORMS }
