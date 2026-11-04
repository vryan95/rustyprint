# frozen_string_literal: true

require "fileutils"
require "open3"
require "rbconfig"
require "rubygems/package"
require "spec_helper"

RSpec.describe "Packaging" do
  gem_dir = Pathname(__dir__).join("..").expand_path

  # Runs Ruby on a copy of lib/ without the native library, as the source gem installs it.
  def run_source_gem(env, script)
    Dir.mktmpdir do |dir|
      FileUtils.cp_r(Pathname(__dir__).join("../lib").to_s, dir)
      FileUtils.rm_rf(File.join(dir, "lib/rustyprint/native"))
      Open3.capture3(env, RbConfig.ruby, "-I", File.join(dir, "lib"), "-e", script)
    end
  end

  describe "the source gem" do
    let(:spec) { Dir.chdir(gem_dir) { Gem::Specification.load("rustyprint.gemspec") } }

    it "is for the ruby platform" do
      expect(spec.platform).to eq(Gem::Platform::RUBY)
    end

    it "leaves out the native library, even when one is built in lib/rustyprint/native" do
      expect(spec.files.grep(%r{native/})).to be_empty
      expect(spec.files).to include("lib/rustyprint/native.rb")
    end
  end

  describe "the native library" do
    it "is missing from the source gem with a message naming the platform and the prebuilt ones" do
      _, err, status = run_source_gem({ "RUSTYPRINT_LIBRARY" => nil }, 'require "rustyprint"')

      expect(status).not_to be_success
      expect(err).to include("LoadError", "no prebuilt native library for #{RUBY_PLATFORM}")
      expect(err).to include(*Rustyprint::PLATFORMS)
      expect(err).to include("bundle lock --add-platform", "RUSTYPRINT_LIBRARY")
    end

    it "says so when RUSTYPRINT_LIBRARY names a file that doesn't exist" do
      missing = File.join(Dir.tmpdir, "rustyprint-missing", "librustyprint_ffi.so")
      _, err, status = run_source_gem({ "RUSTYPRINT_LIBRARY" => missing }, 'require "rustyprint"')

      expect(status).not_to be_success
      expect(err).to include("LoadError", "RUSTYPRINT_LIBRARY is #{missing}")
    end

    it "renders from the source gem when RUSTYPRINT_LIBRARY names a library" do
      library = Rustyprint::Native.path
      script = 'require "rustyprint"; print Rustyprint.render("<h1>Hi</h1>").pdf[0, 5]'
      out, err, status = run_source_gem({ "RUSTYPRINT_LIBRARY" => library }, script)

      expect(status).to be_success, err
      expect(out).to eq("%PDF-")
    end
  end

  describe "Rustyprint::PLATFORMS" do
    it "lists the platform gems the release workflow builds" do
      workflow = SpecHelpers::REPO.join(".github/workflows/release.yml").read
      targets = workflow[/<<'TARGETS'\n(.*?)\n\s*TARGETS/m, 1].lines.map { |line| line.split[1] }

      expect(Rustyprint::PLATFORMS).to eq(targets)
    end
  end
end
