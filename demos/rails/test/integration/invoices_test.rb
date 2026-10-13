require "test_helper"

class InvoicesTest < ActionDispatch::IntegrationTest
  test "the invoices are listed" do
    get invoices_path
    assert_response :success
    Invoice.all.each do |invoice|
      assert_select "a[href=?]", invoice_path(invoice)
      assert_select "a[href=?]", invoice_path(invoice, format: :pdf)
    end
  end

  test "an invoice shows as HTML in the PDF layout" do
    get invoice_path(Invoice.first)
    assert_response :success
    assert_select "h1", "INVOICE"
    assert_select "link[rel=stylesheet][href^='/assets/invoice-']"
    assert_select "img[src^='/assets/logo-']"
    assert_select ".site-header", 0
  end

  test "an invoice renders as a PDF with the stylesheet and logo from the asset pipeline" do
    invoice = Invoice.first
    events = []
    subscriber = ActiveSupport::Notifications.subscribe("render.rustyprint") { |event| events << event }
    get invoice_path(invoice, format: :pdf)
    ActiveSupport::Notifications.unsubscribe(subscriber)

    assert_response :success
    assert_equal "application/pdf", response.content_type
    assert_equal %(inline; filename="invoice #{invoice.id}.pdf"; filename*=UTF-8''invoice%20#{invoice.id}.pdf),
                 response.headers["Content-Disposition"]
    assert response.body.start_with?("%PDF-")
    assert_match %r{/Subtype\s*/Image}, response.body, "the logo is in the PDF"

    assert_equal 1, events.size
    assets = events.first.payload[:assets]
    assert assets.any? { |path| path.match?(%r{\Aassets/invoice-\w+\.css\z}) }, assets.inspect
    assert assets.any? { |path| path.match?(%r{\Aassets/logo-\w+\.png\z}) }, assets.inspect
    assert_equal [], events.first.payload[:warnings]
  end

  test "an unknown invoice is not found" do
    get invoice_path(id: "nope", format: :pdf)
    assert_response :not_found
  end
end
