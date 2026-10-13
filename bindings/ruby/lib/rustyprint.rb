# frozen_string_literal: true

require "json"
require_relative "rustyprint/version"
require_relative "rustyprint/errors"
require_relative "rustyprint/native"
require_relative "rustyprint/railtie" if defined?(Rails::Railtie)

# Renders HTML and CSS to PDF.
#
#   output = Rustyprint.render(html, page_size: "A4", margin: "15mm",
#                                    assets: { "logo.png" => File.binread("logo.png") })
#   File.binwrite("invoice.pdf", output.pdf)
module Rustyprint
  # A rendered document: the PDF as a binary String, and the warnings, problems found in the
  # document's CSS and assets, as Strings.
  Output = Data.define(:pdf, :warnings)

  ERRORS = {
    invalid_options: InvalidOptionsError,
    limit_exceeded: LimitExceededError,
    timeout: TimeoutError
  }.freeze
  private_constant :ERRORS

  class << self
    # Renders an HTML document to PDF and returns an Output.
    #
    # The options are those of schema/render-options.schema.json, as keywords: page_size, landscape,
    # margin, title, stylesheet, remote and limits. assets maps the paths the document uses, such as
    # "img/logo.png", to the files' contents, as Strings, Pathnames or IO objects. Nothing else is
    # read, and nothing is fetched from the network unless remote allows the host.
    #
    # Raises ArgumentError for HTML that is not valid in its encoding, and a Rustyprint::Error
    # subclass when rendering fails. Rendering releases the GVL.
    def render(html, assets: {}, **options)
      html = utf8(html, "html")
      with_request(options) do |request|
        assets.each { |path, data| add_asset(request, path, data) }
        render_request(request, html)
      end
    end

    # The native library's version, the same as VERSION.
    def native_version
      Native.rp_version
    end

    private

    def utf8(text, what)
      text = String(text)
      raise ArgumentError, "#{what} is not valid #{text.encoding}" unless text.valid_encoding?

      text = text.encode(Encoding::UTF_8)
      raise ArgumentError, "#{what} is not valid UTF-8" unless text.valid_encoding?

      text
    end

    def bytes(text)
      pointer = FFI::MemoryPointer.new(:char, [text.bytesize, 1].max)
      pointer.put_bytes(0, text)
      pointer
    end

    def with_request(options)
      json = JSON.generate(options)
      request = Native.rp_request_new(bytes(json), json.bytesize)
      raise InternalError, "the native library could not make a request" if request.null?

      begin
        yield request
      ensure
        Native.rp_request_free(request)
      end
    end

    def add_asset(request, path, data)
      path = utf8(path.to_s, "asset path")
      data = if data.respond_to?(:to_path) then File.binread(data.to_path)
             elsif data.respond_to?(:read) then data.read
             else String(data)
             end.b
      status = Native.rp_request_add_asset(request, path, bytes(data), data.bytesize)
      raise ArgumentError, "invalid asset path #{path.inspect}" unless status == :ok
    end

    def render_request(request, html)
      out = FFI::MemoryPointer.new(:pointer)
      status = Native.rp_render(request, bytes(html), html.bytesize, out)
      result = out.read_pointer
      raise InternalError, "the native library could not make a result" if result.null?

      begin
        raise error(status, result) unless status == :ok

        Output.new(pdf: pdf(result), warnings: JSON.parse(string(result, :rp_result_warnings_json)).freeze)
      ensure
        Native.rp_result_free(result)
      end
    end

    def pdf(result)
      len = FFI::MemoryPointer.new(:size_t)
      pointer = Native.rp_result_pdf(result, len)
      pointer.read_bytes(len.read(:size_t)).force_encoding(Encoding::BINARY)
    end

    def string(result, function)
      len = FFI::MemoryPointer.new(:size_t)
      pointer = Native.public_send(function, result, len)
      pointer.null? ? nil : pointer.read_bytes(len.read(:size_t)).force_encoding(Encoding::UTF_8)
    end

    def error(status, result)
      message = string(result, :rp_result_error)
      ERRORS.fetch(status) { InternalError }.new(message)
    end
  end
end
