# frozen_string_literal: true

module Rustyprint
  # The platforms with a prebuilt gem, which holds the native library. Elsewhere Bundler installs
  # the source gem, which has no library: set RUSTYPRINT_LIBRARY to one built from source.
  PLATFORMS = %w[
    x86_64-linux-gnu aarch64-linux-gnu x86_64-linux-musl aarch64-linux-musl
    x86_64-darwin arm64-darwin x64-mingw-ucrt aarch64-mingw-ucrt
  ].freeze
end
