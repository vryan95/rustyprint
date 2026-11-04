# frozen_string_literal: true

require "ffi"
require_relative "platforms"

module Rustyprint
  # The native library's C ABI (crates/rustyprint-ffi/include/rustyprint.h).
  #
  # @private
  module Native
    extend FFI::Library

    # The library's file name on this platform.
    def self.file_name
      case RbConfig::CONFIG["host_os"]
      when /mswin|mingw|cygwin/ then "rustyprint_ffi.dll"
      when /darwin/ then "librustyprint_ffi.dylib"
      else "librustyprint_ffi.so"
      end
    end

    # Where the library is: RUSTYPRINT_LIBRARY if set, else the copy in the gem, which a
    # platform gem ships and `rake compile` builds.
    def self.path
      ENV.fetch("RUSTYPRINT_LIBRARY") { File.expand_path("native/#{file_name}", __dir__) }
    end

    # Why the library is missing, and what to do about it.
    def self.missing_message
      if ENV.key?("RUSTYPRINT_LIBRARY")
        "rustyprint's native library is missing: RUSTYPRINT_LIBRARY is #{path}, which doesn't exist."
      elsif File.exist?(File.expand_path("../../rustyprint.gemspec", __dir__))
        "rustyprint's native library is missing at #{path}. Run `bundle exec rake compile` in " \
          "bindings/ruby to build it, or set RUSTYPRINT_LIBRARY to a library built elsewhere."
      else
        "rustyprint has no prebuilt native library for #{RUBY_PLATFORM}, so the source gem, which " \
          "has none, was installed. Prebuilt gems exist for #{PLATFORMS.join(', ')}. If this " \
          "machine is one of those, run `bundle lock --add-platform <platform>` and `bundle " \
          "install` (and check that force_ruby_platform isn't set). Otherwise build the library " \
          "from source with `cargo build --release -p rustyprint-ffi` in a checkout of " \
          "https://github.com/vryan95/rustyprint and set RUSTYPRINT_LIBRARY to its path."
      end
    end

    raise LoadError, missing_message unless File.exist?(path)

    ffi_lib path

    Status = enum(:status, %i[ok invalid_argument invalid_options limit_exceeded timeout internal])

    attach_function :rp_version, [], :string
    attach_function :rp_request_new, %i[pointer size_t], :pointer
    attach_function :rp_request_add_asset, %i[pointer string pointer size_t], :status
    # Renders release the GVL, so other Ruby threads run meanwhile.
    attach_function :rp_render, %i[pointer pointer size_t pointer], :status, blocking: true
    attach_function :rp_result_pdf, %i[pointer pointer], :pointer
    attach_function :rp_result_warnings_json, %i[pointer pointer], :pointer
    attach_function :rp_result_error, %i[pointer pointer], :pointer
    attach_function :rp_result_free, [:pointer], :void
    attach_function :rp_request_free, [:pointer], :void
  end
end
