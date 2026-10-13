class InvoicesController < ApplicationController
  def index
    @invoices = Invoice.all
  end

  def show
    @invoice = Invoice.find(params[:id])
    @title = "Invoice #{@invoice.id}"

    respond_to do |format|
      format.html do
        render layout: "pdf"
      end
      format.pdf do
        render pdf: "invoice #{@invoice.id}",
          template: "invoices/show",
          layout: "pdf",
          disposition: "inline"
      end
    end
  end
end
