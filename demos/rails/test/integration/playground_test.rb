require "test_helper"

class PlaygroundTest < ActionDispatch::IntegrationTest
  test "the playground starts with an example to edit" do
    get playground_path
    assert_response :success
    assert_select "form[data-controller=playground]"
    assert_select "textarea[name=html]", /<h1>/
    assert_select "textarea[name=css]", /@page/
    assert_select "select[name=page_size] option", minimum: 3
    assert_select "iframe.preview"
  end

  test "rendering returns the PDF, its pages and the warnings" do
    post playground_path(format: :json), params: {
      html: "<h1>Hello</h1><p style='colr: red'>x</p>",
      css: "@page { margin: 1cm } h1 { color: teal }",
      page_size: "A5", landscape: "1", margin: ""
    }
    assert_response :success
    body = response.parsed_body
    pdf = Base64.strict_decode64(body["pdf"])
    assert pdf.start_with?("%PDF-")
    assert_equal 1, body["pages"]
    assert_kind_of Integer, body["milliseconds"]
    assert body["warnings"].any? { |warning| warning.include?("colr") }, body["warnings"].inspect
    assert_match %r{/MediaBox \[0 0 595\.\d+ 419\.\d+\]}, pdf, "A5 landscape"
    assert_equal [ 210, 148 ], body["page_mm"]
  end

  test "the example leaves the page size and margin to the options" do
    example = { html: PlaygroundController::EXAMPLE_HTML, css: PlaygroundController::EXAMPLE_CSS }
    pdfs = [ {}, { margin: "5mm" }, { page_size: "Letter" } ].map do |options|
      post playground_path(format: :json), params: example.merge(options)
      response.parsed_body
    end
    assert_equal [ 210, 297 ], pdfs[0]["page_mm"]
    assert_not_equal pdfs[0]["pdf"], pdfs[1]["pdf"], "the margin option changes the PDF"
    assert_equal [ 216, 279 ], pdfs[2]["page_mm"]
  end

  test "invalid options and documents over the limits are reported, not raised" do
    post playground_path(format: :json), params: { html: "<p>x</p>", margin: "wide" }
    assert_response :unprocessable_content
    assert_match(/margin/, response.parsed_body["error"])

    post playground_path(format: :json), params: { html: "<div style='break-after: page'>x</div>" * 30 }
    assert_response :unprocessable_content
    assert_match(/page/, response.parsed_body["error"])
  end
end
