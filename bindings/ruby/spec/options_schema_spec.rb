# frozen_string_literal: true

require "spec_helper"

# The keyword options are the JSON options of schema/render-options.schema.json, which every
# front end shares.
RSpec.describe "render options" do
  schema = JSON.parse(SpecHelpers::REPO.join("schema/render-options.schema.json").read)

  schema.fetch("examples").each do |example|
    it "accepts the schema example #{JSON.generate(example)}" do
      options = JSON.parse(JSON.generate(example), symbolize_names: true)
      expect(Rustyprint.render("<p>x</p>", **options).pdf).to start_with("%PDF-")
    end
  end

  it "covers every option in the schema" do
    used = schema.fetch("examples").flat_map(&:keys).uniq
    expect(used).to match_array(schema.fetch("properties").keys)
  end

  it "refuses what the schema does not allow" do
    expect { Rustyprint.render("<p>x</p>", landscape: "yes") }.to raise_error(Rustyprint::InvalidOptionsError)
    expect { Rustyprint.render("<p>x</p>", limits: { max_pages: -1 }) }.to raise_error(Rustyprint::InvalidOptionsError)
  end
end
