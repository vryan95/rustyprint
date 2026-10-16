# frozen_string_literal: true

require "spec_helper"

RSpec.describe Rustyprint do
  describe ".render" do
    it "renders HTML to a PDF" do
      output = described_class.render("<h1>Hello</h1>")
      expect(output.pdf).to start_with("%PDF-")
      expect(output.pdf.encoding).to eq(Encoding::BINARY)
      expect(output.warnings).to eq([])
      # A4 with the default options.
      expect(page_sizes(output.pdf)).to eq([[595.3, 841.9]])
    end

    it "takes the options as keywords" do
      output = described_class.render("<p>Hello</p>", page_size: "A5", landscape: true, margin: "1cm",
                                                       title: "Hello")
      expect(page_sizes(output.pdf)).to eq([[595.3, 419.5]])
    end

    it "takes nested options" do
      output = described_class.render("<p>x</p>", page_size: { width: "100mm", height: "150mm" },
                                                  limits: { max_pages: 5, timeout_ms: nil })
      expect(page_sizes(output.pdf)).to eq([[283.5, 425.2]])
    end

    it "uses assets by the path the document gives" do
      css = "@page { size: 300px 200px }"
      output = described_class.render('<link rel="stylesheet" href="css/page.css"><p>x</p>',
                                      assets: { "css/page.css" => css })
      expect(page_sizes(output.pdf)).to eq([[225.0, 150.0]])
    end

    it "reads assets given as paths and IO" do
      Dir.mktmpdir do |dir|
        path = Pathname(dir).join("page.css")
        path.write("@page { size: 400px 400px }")
        from_path = described_class.render('<link rel="stylesheet" href="page.css">',
                                           assets: { "page.css" => path })
        from_io = File.open(path) do |file|
          described_class.render('<link rel="stylesheet" href="page.css">', assets: { "page.css" => file })
        end
        expect(page_sizes(from_path.pdf)).to eq([[300.0, 300.0]])
        expect(page_sizes(from_io.pdf)).to eq([[300.0, 300.0]])
      end
    end

    it "accepts symbols as asset paths" do
      output = described_class.render('<link rel="stylesheet" href="page.css">',
                                      assets: { "page.css": "@page { size: 400px 400px }" })
      expect(page_sizes(output.pdf)).to eq([[300.0, 300.0]])
    end

    it "returns the warnings" do
      output = described_class.render("<style>p { colör: red; colr: blue }</style><p>x</p>")
      expect(output.warnings.size).to eq(2)
      expect(output.warnings).to all(be_a(String))
      expect(output.warnings.join).to include("colör")
    end

    it "renders HTML in other encodings" do
      html = "<p>Café</p>".encode(Encoding::ISO_8859_1)
      expect(described_class.render(html).pdf).to start_with("%PDF-")
    end

    it "refuses HTML that is not valid in its encoding" do
      expect { described_class.render("<p>\xff</p>") }.to raise_error(ArgumentError, /UTF-8/)
    end

    it "gives the same bytes for the same input, on any thread" do
      dir = SpecHelpers::REPO.join("demos/samples/invoice")
      html = dir.join("index.html").read
      assets = { "invoice.css" => dir.join("invoice.css"), "logo.png" => dir.join("logo.png") }
      first = described_class.render(html, assets: assets).pdf
      again = described_class.render(html, assets: assets).pdf
      threaded = Array.new(4) { Thread.new { described_class.render(html, assets: assets).pdf } }.map(&:value)
      expect([again, *threaded]).to all(eq(first))
    end

    it "renders on several threads at once" do
      outputs = Array.new(4) { |i| Thread.new { described_class.render("<p>#{i}</p>", page_size: "A5") } }.map(&:value)
      expect(outputs.map { |o| page_sizes(o.pdf) }.uniq).to eq([[[419.5, 595.3]]])
    end

    it "lets other Ruby threads run while it renders" do
      html = "<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>" * 10_000
      ticks = []
      render = Thread.new do
        started = Process.clock_gettime(Process::CLOCK_MONOTONIC)
        described_class.render(html)
        [started, Process.clock_gettime(Process::CLOCK_MONOTONIC)]
      end
      while render.alive?
        ticks << Process.clock_gettime(Process::CLOCK_MONOTONIC)
        sleep 0.005
      end
      started, finished = render.value
      expect(finished - started).to be > 0.1
      # With the GVL held, this thread could not have run in the middle of the render, so any
      # tick there proves it was released. Sleeps overshoot a lot on some hosts (macOS), so
      # don't ask for more than one.
      middle = ticks.count { |t| t > started + 0.02 && t < finished - 0.02 }
      expect(middle).to be_positive
    end
  end

  describe "errors" do
    it "raises InvalidOptionsError for options the renderer does not know" do
      expect { described_class.render("<p>x</p>", colour: "red") }
        .to raise_error(Rustyprint::InvalidOptionsError, /colour/)
    end

    it "raises InvalidOptionsError for invalid values" do
      expect { described_class.render("<p>x</p>", page_size: "B99") }
        .to raise_error(Rustyprint::InvalidOptionsError) { |e| expect(e.kind).to eq(:invalid_options) }
    end

    it "raises LimitExceededError" do
      expect { described_class.render("<p>More than ten bytes</p>", limits: { max_input_bytes: 10 }) }
        .to raise_error(Rustyprint::LimitExceededError, /input size/)
    end

    it "raises TimeoutError when the deadline passes" do
      html = "<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit.</p>" * 5_000
      expect { described_class.render(html, limits: { timeout_ms: 1 }) }
        .to raise_error(Rustyprint::TimeoutError, /deadline/)
    end

    it "makes every error a Rustyprint::Error" do
      [Rustyprint::InvalidOptionsError, Rustyprint::LimitExceededError, Rustyprint::TimeoutError,
       Rustyprint::InternalError].each do |error|
        expect(error.ancestors).to include(Rustyprint::Error, StandardError)
      end
    end
  end

  describe "versions" do
    it "matches the workspace version" do
      cargo = SpecHelpers::REPO.join("Cargo.toml").read[/^\[workspace\.package\]\s+version = "([^"]+)"/, 1]
      expect(Rustyprint::VERSION).to eq(cargo)
    end

    it "matches the native library's version" do
      expect(described_class.native_version).to eq(Rustyprint::VERSION)
    end
  end
end
