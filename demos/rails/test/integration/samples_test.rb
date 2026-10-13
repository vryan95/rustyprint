require "test_helper"

class SamplesTest < ActionDispatch::IntegrationTest
  test "the home page lists every sample" do
    get root_path
    assert_response :success
    assert_select ".sample-card", 3
    %w[invoice report letter].each do |id|
      assert_select "a[href=?]", sample_path(id)
      assert_select "a[href=?]", sample_path(id, format: :pdf)
    end
  end

  test "a sample's page shows its PDF, how it renders in a browser, and its source" do
    get sample_path("report")
    assert_response :success
    assert_select "h1", "Quarterly report, Q3 2026"
    assert_select "iframe[src=?]", sample_path("report", format: :pdf)
    assert_select "iframe[src=?]", sample_file_path("report", "index.html")
    assert_select ".stat", /8\s*pages/
    assert_select ".source code", /charts\/revenue.png/
  end

  test "each sample renders to a PDF with the pages it is meant to have" do
    { "invoice" => 3, "report" => 8, "letter" => 1 }.each do |id, pages|
      get sample_path(id, format: :pdf)
      assert_response :success
      assert_equal "application/pdf", response.media_type
      assert_match(/\Ainline; filename="#{id}.pdf"/, response.headers["Content-Disposition"])
      assert response.body.start_with?("%PDF-")
      assert_equal pages, response.body.scan(%r{/MediaBox \[}).size, "#{id} pages"
    end
  end

  test "a PDF can be downloaded" do
    get sample_path("letter", format: :pdf, download: 1)
    assert_match(/\Aattachment; filename="letter.pdf"/, response.headers["Content-Disposition"])
  end

  test "a sample's own files are served, for the browser preview" do
    get sample_file_path("letter", "index.html")
    assert_response :success
    assert_equal "text/html", response.media_type

    get sample_file_path("letter", "fonts/Caveat.ttf")
    assert_response :success
    assert_equal File.binread(Sample.find("letter").dir.join("fonts/Caveat.ttf")), response.body
  end

  test "nothing outside a sample is served" do
    get sample_file_path("letter", "../invoice/logo.png")
    assert_response :not_found
    get sample_file_path("letter", "missing.css")
    assert_response :not_found
    get sample_path("nope")
    assert_response :not_found
  end
end
