# frozen_string_literal: true

require_relative "lib/rustyprint/version"

Gem::Specification.new do |spec|
  spec.name = "rustyprint"
  spec.version = Rustyprint::VERSION
  spec.authors = ["Ryan Verschuren"]
  spec.summary = "Render HTML and CSS to PDF"
  spec.description = "rustyprint renders HTML and CSS to PDF with a layout engine of its own, written in " \
                     "Rust: no browser, no external runtime and no network access unless allowed."
  spec.homepage = "https://github.com/vryan95/rustyprint"
  spec.metadata = {
    "source_code_uri" => "https://github.com/vryan95/rustyprint/tree/master/bindings/ruby",
    "rubygems_mfa_required" => "true"
  }

  spec.required_ruby_version = ">= 3.2"
  # Tells glibc and musl Linux platform gems apart.
  spec.required_rubygems_version = ">= 3.3.22"

  # Platform gems add the native library, lib/rustyprint/native/ (see `rake native`).
  spec.files = Dir["lib/**/*.rb", "README.md"]
  spec.require_paths = ["lib"]

  spec.add_dependency "ffi", "~> 1.17"
end
