module SamplesHelper
  # The Ruby a Rails controller would use to render the sample.
  def render_snippet(sample)
    assets = sample.asset_paths.map { |path| %(  "#{path}" => dir.join("#{path}")) }.join(",\n")
    <<~RUBY
      dir = Rails.root.join("samples/#{sample.id}")
      output = Rustyprint.render(dir.join("index.html").read, assets: {
      #{assets}
      })
      send_data output.pdf, filename: "#{sample.id}.pdf", type: :pdf, disposition: :inline
    RUBY
  end
end
