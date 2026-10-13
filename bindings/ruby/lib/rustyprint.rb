# frozen_string_literal: true

require "json"
require_relative "rustyprint/version"
require_relative "rustyprint/errors"
require_relative "rustyprint/native"
require_relative "rustyprint/railtie" if defined?(Rails::Railtie)

# Renders HTML and CSS to PDF.
#
# @example
#   output = Rustyprint.render(html, page_size: "A4", margin: "15mm",
#                                    assets: { "logo.png" => File.binread("logo.png") })
#   File.binwrite("invoice.pdf", output.pdf)
module Rustyprint
  # A rendered document, as {Rustyprint.render} returns it.
  #
  # @!attribute [r] pdf
  #   @return [String] the PDF, a binary String
  # @!attribute [r] warnings
  #   @return [Array<String>] problems found in the document's CSS and assets, such as
  #     ``"invoice.css:12: unsupported property `colr`"``
  Output = Data.define(:pdf, :warnings)

  ERRORS = {
    invalid_options: InvalidOptionsError,
    limit_exceeded: LimitExceededError,
    timeout: TimeoutError
  }.freeze
  private_constant :ERRORS

  class << self
    # Renders an HTML document to PDF.
    #
    # The options are those of
    # [render-options.schema.json](https://github.com/vryan95/rustyprint/blob/master/schema/render-options.schema.json),
    # as keywords. An option left out keeps its default. Nothing is read but the assets, and
    # nothing is fetched from the network unless `remote` allows the host. Rendering releases the
    # GVL, so other threads run meanwhile, and renders on several threads run at once.
    #
    # @example A4 with a logo and a stylesheet
    #   output = Rustyprint.render(html, page_size: "A4", margin: "15mm", title: "Invoice 42",
    #                                    assets: { "logo.png" => File.binread("logo.png"),
    #                                              "invoice.css" => Pathname("app/pdf/invoice.css") })
    #   File.binwrite("invoice.pdf", output.pdf)
    #
    # @param html [String] the document, in any encoding Ruby can convert to UTF-8
    # @param assets [Hash{String => String, Pathname, IO}] the files the document refers to by
    #   relative or root-relative URL, by the path it uses without a leading slash (`/css/app.css`
    #   is `"css/app.css"`): their contents as a String, a Pathname to read, or an IO to read
    # @param options [Hash] the render options
    # @option options [String, Hash] :page_size `"A4"` (the default), `"Letter"` or another named
    #   size, or `{ width: "100mm", height: "150mm" }`
    # @option options [Boolean] :landscape turns the page so that it is wider than it is tall;
    #   `false` by default
    # @option options [String] :margin the page margins, as CSS: `"15mm"` or `"10mm 20mm"`;
    #   `"2cm"` by default
    # @option options [String] :title the PDF's title; the document's `<title>` by default
    # @option options [String] :stylesheet CSS applied after the document's own
    # @option options [Hash] :remote the hosts to fetch from, such as
    #   `{ allowed_hosts: ["cdn.example.com"] }`; nothing is fetched by default
    # @option options [Hash] :limits bounds on the render, such as
    #   `{ max_pages: 50, timeout_ms: 10_000 }`; by default 20 MB of HTML, 500,000 nodes, 2,000
    #   pages and 60 seconds. Exceeding one fails the render, except that larger images are
    #   skipped with a warning
    # @return [Output] the PDF and the warnings
    # @raise [ArgumentError] if the HTML or an asset path is not valid in its encoding, or an asset
    #   path is not valid
    # @raise [InvalidOptionsError] if an option is unknown or has an invalid value
    # @raise [LimitExceededError] if the document exceeds a limit
    # @raise [TimeoutError] if rendering takes longer than the deadline
    # @raise [InternalError] if rustyprint has a bug
    def render(html, assets: {}, **options)
      html = utf8(html, "html")
      with_request(options) do |request|
        assets.each { |path, data| add_asset(request, path, data) }
        render_request(request, html)
      end
    end

    # The native library's version.
    #
    # @return [String] the version, the same as {VERSION}
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
