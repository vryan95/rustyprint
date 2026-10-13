# frozen_string_literal: true

require "rack/mock_request"
require_relative "asset_collector"

module Rustyprint
  # Renders a controller's view to PDF, for `render pdf:`:
  #
  #   format.pdf do
  #     render pdf: "invoice #{@invoice.id}", template: "invoices/show", layout: "pdf",
  #            page_size: "A4", margin: "15mm", disposition: "attachment"
  #   end
  #
  # The view renders as HTML, unless `formats` says otherwise, with the other options a
  # `render` takes, such as `template`, `layout` and `locals`. The PDF options are those of
  # Rustyprint.render. `pdf` names the file, with `.pdf` added, and `disposition` is "inline"
  # (the default) or "attachment".
  #
  # The stylesheets, images and fonts the HTML refers to by path, such as those
  # `stylesheet_link_tag` and `image_tag` give, are read from `public/` or else served by the
  # app's asset pipeline. `assets` adds files of your own, by the path the HTML uses. Each
  # render is instrumented as "render.rustyprint", with the asset paths and warnings.
  class PdfRenderer
    PDF_OPTIONS = %i[page_size landscape margin title stylesheet remote limits].freeze

    def initialize(controller)
      @controller = controller
    end

    def render(name, options)
      view = options.except(:pdf, :assets, :disposition, *PDF_OPTIONS)
      view[:formats] ||= [:html]
      html = @controller.render_to_string(view)

      payload = { name: name.to_s }
      output = ActiveSupport::Notifications.instrument("render.rustyprint", payload) do
        assets = collect(html).merge(options.fetch(:assets, {}).transform_keys(&:to_s))
        payload[:assets] = assets.keys
        Rustyprint.render(html, assets: assets, **options.slice(*PDF_OPTIONS)).tap do |rendered|
          payload[:warnings] = rendered.warnings
        end
      end
      output.warnings.each { |warning| @controller.logger&.warn("rustyprint: #{warning}") }

      @controller.response.charset = false
      @controller.content_type = Mime[:pdf]
      @controller.headers["Content-Disposition"] = ActionDispatch::Http::ContentDisposition.format(
        disposition: options.fetch(:disposition, "inline").to_s, filename: file_name(name)
      )
      output.pdf
    end

    private

    def file_name(name)
      name = name.to_s
      name.end_with?(".pdf") ? name : "#{name}.pdf"
    end

    def collect(html)
      AssetCollector.new { |path| public_file(path) || pipeline_file(path) }.collect(html)
    end

    # A file under public/, such as a precompiled asset.
    def public_file(path)
      file = ::Rails.public_path.join(path.delete_prefix("/"))
      File.binread(file) if file.file?
    end

    # A file the app's asset pipeline serves, as Propshaft and Sprockets do in development.
    def pipeline_file(path)
      config = ::Rails.application.config
      prefix = config.respond_to?(:assets) ? config.assets.prefix : "/assets"
      return unless path.start_with?("#{prefix}/")

      request = @controller.request
      # The request's host, so the app's host authorization allows it as it did the request.
      env = Rack::MockRequest.env_for("#{request.protocol}#{request.host_with_port}#{path}",
                                      "HTTP_HOST" => request.host_with_port)
      status, _headers, body = ::Rails.application.call(env)
      return unless status == 200

      data = +""
      body.each { |chunk| data << chunk }
      data
    ensure
      body.close if body.respond_to?(:close)
    end
  end
end
