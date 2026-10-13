require "test_helper"
require "rake"

class SamplesTaskTest < ActiveSupport::TestCase
  setup do
    Rails.application.load_tasks if Rake::Task.tasks.none? { |task| task.name == "samples:render" }
    Rake::Task["samples:render"].reenable
  end

  test "samples:render writes every sample to a PDF" do
    Dir.mktmpdir do |dir|
      output = with_env("OUT" => dir) { capture_io { Rake::Task["samples:render"].invoke }.first }
      Sample.all.each do |sample|
        pdf = File.binread(File.join(dir, "#{sample.id}.pdf"))
        assert pdf.start_with?("%PDF-"), sample.id
        assert_match(/#{sample.id}\.pdf\s+#{sample.render.pages} pages?/, output)
      end
    end
  end

  private

  def with_env(values)
    saved = values.keys.to_h { |key| [ key, ENV[key] ] }
    values.each { |key, value| ENV[key] = value }
    yield
  ensure
    saved.each { |key, value| ENV[key] = value }
  end
end
