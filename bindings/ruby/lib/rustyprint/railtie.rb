# frozen_string_literal: true

require "rails/railtie"
require_relative "pdf_renderer"

module Rustyprint
  # Adds `render pdf:` to Rails controllers. Loaded when the gem is required after Rails.
  class Railtie < ::Rails::Railtie
    initializer "rustyprint.renderer" do
      ActiveSupport.on_load(:action_controller) do
        ActionController::Renderers.add(:pdf) do |name, options|
          Rustyprint::PdfRenderer.new(self).render(name, options)
        end
      end
    end
  end
end
