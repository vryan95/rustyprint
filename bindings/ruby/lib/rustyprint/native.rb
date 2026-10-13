# frozen_string_literal: true

require "ffi"

module Rustyprint
  # The native library's C ABI (crates/rustyprint-ffi/include/rustyprint.h).
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

    unless File.exist?(path)
      raise LoadError, "rustyprint's native library is missing at #{path}. Install a platform gem of " \
                       "rustyprint, run `rake compile` in bindings/ruby, or set RUSTYPRINT_LIBRARY."
    end

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
