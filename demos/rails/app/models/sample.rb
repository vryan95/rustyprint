# A sample document: one of the shared samples in demos/samples, an index.html with the
# stylesheets, images and fonts it refers to by relative URL beside it.
class Sample
  ROOT = Pathname(ENV.fetch("RUSTYPRINT_SAMPLES") { Rails.root.join("../samples") }).expand_path

  # What each sample shows off, in the order the home page lists them.
  CATALOG = {
    "invoice" => {
      summary: "A three-page invoice with a logo, a long table and totals.",
      features: [ "Logo image", "40-line table whose header row repeats on every page",
                  "Page numbers in the page footer" ]
    },
    "report" => {
      summary: "An eight-page quarterly report with a title page, a table of contents and charts.",
      features: [ "Title page with its own page style", "Table of contents with page numbers and leaders",
                  "Page headers and footers", "Charts as images, and tables" ]
    },
    "letter" => {
      summary: "A one-page letter signed in a handwriting web font.",
      features: [ "Web font loaded with @font-face (Caveat)" ]
    }
  }.freeze

  class NotFound < StandardError; end

  attr_reader :id, :summary, :features, :dir

  def self.all
    CATALOG.keys.map { |id| find(id) }
  end

  def self.find(id)
    entry = CATALOG.fetch(id.to_s) { raise NotFound, "no sample #{id.inspect}" }
    new(id.to_s, **entry)
  end

  def initialize(id, summary:, features:)
    @id = id
    @summary = summary
    @features = features
    @dir = ROOT.join(id)
  end

  def to_param = id

  def html = dir.join("index.html").read

  def title
    html[%r{<title>(.*?)</title>}m, 1].then { |text| CGI.unescapeHTML(text.to_s.strip) }
  end

  # The files the document uses, by the relative path it uses for each: every file beside it.
  def asset_paths
    @asset_paths ||= dir.glob("**/*").select(&:file?).map { |path| path.relative_path_from(dir).to_s }
                        .reject { |path| path == "index.html" }.sort
  end

  # An asset the document refers to by a relative path, or nil for anything else.
  def asset(path)
    dir.join(path) if asset_paths.include?(path)
  end

  def render
    started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    output = Rustyprint.render(html, assets: asset_paths.to_h { |path| [ path, dir.join(path) ] })
    elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
    Rendering.new(pdf: output.pdf, warnings: output.warnings, seconds: elapsed)
  end

  # A sample rendered to PDF, with what it took.
  Rendering = Data.define(:pdf, :warnings, :seconds) do
    def pages = pdf.scan(%r{/MediaBox \[}).size

    def bytes = pdf.bytesize
  end
end
