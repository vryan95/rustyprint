# frozen_string_literal: true

require "cgi"

module Rustyprint
  # Finds the files an HTML document refers to, in its tags, its style attributes and blocks,
  # and the stylesheets it loads, and fetches each once, for {Rustyprint.render}'s assets.
  #
  # @example
  #   collector = AssetCollector.new { |path| File.binread("public#{path}") if File.file?("public#{path}") }
  #   collector.collect(html) # => { "assets/app-1a2b.css" => "...", "images/logo.png" => "..." }
  #
  # The block gets a root-relative path, such as "/assets/app-1a2b.css", and returns the file's
  # contents, or nil if there is none. The result maps the path the document uses, without its
  # leading slash, to the contents. Remote and data: URLs are left to the renderer.
  class AssetCollector
    TAG = %r{<([a-zA-Z][\w-]*)((?:\s+[^\s=>/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*/?>}
    ATTRIBUTE = %r{([^\s=>/]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s>]+))?}
    STYLE_BLOCK = %r{<style\b[^>]*>(.*?)</style>}mi
    CSS_COMMENT = %r{/\*.*?\*/}m
    CSS_URL = /url\(\s*(?:"([^"]*)"|'([^']*)'|([^)\s]*))\s*\)|@import\s+(?:"([^"]*)"|'([^']*)')/i
    URL_ATTRIBUTES = %w[src poster data].freeze
    STYLESHEET = /\A\s*(\S+\s+)*stylesheet(\s+\S+)*\s*\z/i
    private_constant :TAG, :ATTRIBUTE, :STYLE_BLOCK, :CSS_COMMENT, :CSS_URL, :URL_ATTRIBUTES, :STYLESHEET

    # @yieldparam path [String] a root-relative path the document uses, such as
    #   `"/assets/app-1a2b.css"`
    # @yieldreturn [String, nil] the file's contents, or nil if there is none
    def initialize(&fetch)
      @fetch = fetch
    end

    # The files the document refers to.
    #
    # @param html [String] the document
    # @return [Hash{String => String}] each file's contents, by the path the document uses for it,
    #   without its leading slash
    def collect(html)
      @assets = {}
      @seen = {}
      html.scan(STYLE_BLOCK) { |(css)| collect_css(css) }
      html.scan(TAG) { |name, attributes| collect_tag(name.downcase, attributes) }
      @assets
    end

    private

    def collect_tag(name, attributes)
      attributes = attributes.scan(ATTRIBUTE).to_h do |key, value|
        [key.downcase, CGI.unescapeHTML(value.to_s.delete_prefix('"').delete_prefix("'").chomp('"').chomp("'"))]
      end
      URL_ATTRIBUTES.each { |key| add(attributes[key]) if attributes[key] }
      attributes["srcset"]&.split(",")&.each { |candidate| add(candidate.split.first) }
      collect_css(attributes["style"]) if attributes["style"]
      add(attributes["href"], stylesheet: true) if name == "link" && STYLESHEET.match?(attributes["rel"].to_s)
      add(attributes["href"]) if %w[image use].include?(name) && attributes["href"]
    end

    def collect_css(css)
      css.gsub(CSS_COMMENT, "").scan(CSS_URL) do |url, *rest|
        import = rest[2] || rest[3]
        add(url || rest[0] || rest[1] || import, stylesheet: !import.nil?)
      end
    end

    def add(url, stylesheet: false)
      path = asset_path(url) or return
      return if @seen.key?(path)

      @seen[path] = true
      data = @fetch.call("/#{path}") or return
      key = decode(path)
      @assets[key] = data
      collect_css(data.dup.force_encoding(Encoding::UTF_8).scrub) if stylesheet || key.end_with?(".css")
    end

    # The URL's path from the document's root, as the renderer resolves it, or nil for remote,
    # data: and fragment-only URLs and paths that leave the root.
    def asset_path(url)
      url = url.to_s.strip.split(/[?#]/, 2).first.to_s
      return if url.empty? || url.start_with?("//", "\\") || url.match?(/\A[a-z][a-z\d+.-]*:/i)

      segments = url.split("/").reject { |segment| segment.empty? || segment == "." }
      return if segments.empty? || segments.include?("..")

      segments.join("/")
    end

    def decode(path)
      path.b.gsub(/%\h\h/n) { |escape| escape[1..].hex.chr }.force_encoding(Encoding::UTF_8)
    end
  end
end
