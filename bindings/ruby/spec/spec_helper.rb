# frozen_string_literal: true

require "json"
require "pathname"
require "tmpdir"
require "rustyprint"

module SpecHelpers
  REPO = Pathname(__dir__).join("../../..").expand_path

  # The page sizes of a PDF in points, from its /MediaBox entries.
  def page_sizes(pdf)
    pdf.scan(%r{/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]}).map { |w, h| [w.to_f.round(1), h.to_f.round(1)] }
  end
end

RSpec.configure do |config|
  config.include SpecHelpers
  config.disable_monkey_patching!
  config.order = :random
end
