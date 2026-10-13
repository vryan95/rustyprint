# Edit HTML, CSS and the render options, and see the PDF and the warnings at once.
class PlaygroundController < ApplicationController
  PAGE_SIZES = %w[A4 A5 A3 Letter Legal].freeze
  # A playground renders whatever it is given, so it gets tighter limits than the defaults.
  LIMITS = { max_pages: 20, timeout_ms: 10_000 }.freeze

  def show
    @html = EXAMPLE_HTML
    @css = EXAMPLE_CSS
  end

  def render_pdf
    started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    output = Rustyprint.render(params[:html].to_s, **options)
    elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC) - started
    render json: {
      pdf: Base64.strict_encode64(output.pdf),
      pages: output.pdf.scan(%r{/MediaBox \[}).size,
      page_mm: page_mm(output.pdf),
      milliseconds: (elapsed * 1000).round,
      warnings: output.warnings
    }
  rescue Rustyprint::Error, ArgumentError => error
    render json: { error: error.message }, status: :unprocessable_content
  end

  private

  # The first page's width and height in millimetres.
  def page_mm(pdf)
    box = pdf.match(%r{/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]}) or return
    box.captures.map { |points| (points.to_f * 25.4 / 72).round }
  end

  def options
    {
      stylesheet: params[:css].presence,
      page_size: params[:page_size].presence_in(PAGE_SIZES),
      landscape: params[:landscape] == "1",
      margin: params[:margin].presence,
      limits: LIMITS
    }.compact
  end

  EXAMPLE_HTML = <<~HTML.freeze
    <h1>Hello from the playground</h1>
    <p>
      Edit the HTML and CSS, choose the page options, and the PDF on the right updates.
      Problems found in the CSS show as warnings below.
    </p>
    <table>
      <thead><tr><th>Item</th><th>Qty</th><th>Price</th></tr></thead>
      <tbody>
        <tr><td>Oak desk</td><td>2</td><td>€ 978.00</td></tr>
        <tr><td>Office chair</td><td>4</td><td>€ 1,316.00</td></tr>
        <tr><td>Desk lamp</td><td>4</td><td>€ 259.80</td></tr>
      </tbody>
    </table>
  HTML

  EXAMPLE_CSS = <<~CSS.freeze
    @page {
      @bottom-right { content: "Page " counter(page) " of " counter(pages); font-size: 9pt; }
    }
    body { font-family: "Noto Sans", sans-serif; color: #222; }
    h1 { color: #b7410e; }
    table { width: 100%; border-collapse: collapse; }
    th, td { padding: 6px 8px; border-bottom: 1px solid #ddd; text-align: left; }
    th { background: #f4f1ea; }
  CSS
end
