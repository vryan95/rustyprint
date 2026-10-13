# frozen_string_literal: true

require "spec_helper"
require "rustyprint/asset_collector"

RSpec.describe Rustyprint::AssetCollector do
  let(:files) do
    {
      "/assets/app-1a2b.css" => '@import "/assets/base-3c4d.css"; h1 { background: url("/assets/bg-5e6f.png") }',
      "/assets/base-3c4d.css" => "@font-face { font-family: X; src: url(/assets/x-7a8b.ttf) format('truetype') }",
      "/assets/bg-5e6f.png" => "png",
      "/assets/x-7a8b.ttf" => "ttf",
      "/assets/logo-9c0d.png" => "logo",
      "/images/photo.jpg" => "jpg",
      "/hero.png" => "hero"
    }
  end
  let(:requested) { [] }
  let(:collector) do
    described_class.new do |path|
      requested << path
      files[path]
    end
  end

  it "collects what the HTML and its stylesheets refer to, by the path the document uses" do
    html = <<~HTML
      <link rel="stylesheet" href="/assets/app-1a2b.css?v=1">
      <img src='/assets/logo-9c0d.png' alt="">
      <img srcset="/images/photo.jpg 1x, /images/photo.jpg 2x">
      <div style="background: url(&quot;hero.png&quot;)"></div>
    HTML
    expect(collector.collect(html)).to eq(
      "assets/app-1a2b.css" => files["/assets/app-1a2b.css"],
      "assets/base-3c4d.css" => files["/assets/base-3c4d.css"],
      "assets/bg-5e6f.png" => "png",
      "assets/x-7a8b.ttf" => "ttf",
      "assets/logo-9c0d.png" => "logo",
      "images/photo.jpg" => "jpg",
      "hero.png" => "hero"
    )
  end

  it "fetches each file once" do
    collector.collect('<img src="/hero.png"><img src="hero.png"><img src="./hero.png#x">')
    expect(requested).to eq(["/hero.png"])
  end

  it "leaves remote, data, fragment and escaping URLs to the renderer" do
    html = <<~HTML
      <img src="https://cdn.example.com/a.png"><img src="//cdn.example.com/b.png">
      <img src="data:image/png;base64,AAAA"><a href="#top">top</a><a href="mailto:x@example.com">x</a>
      <img src="/../secret.png"><img src="/">
    HTML
    expect(collector.collect(html)).to eq({})
    expect(requested).to eq([])
  end

  it "skips files that cannot be found" do
    expect(collector.collect('<img src="/missing.png"><img src="/hero.png">')).to eq("hero.png" => "hero")
  end

  it "skips links that are not resources" do
    html = '<a href="/invoices/1">Invoice</a><link rel="canonical" href="/hero.png">'
    expect(collector.collect(html)).to eq({})
    expect(requested).to eq([])
  end
end
