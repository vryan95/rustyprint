class SamplesController < ApplicationController
  rescue_from Sample::NotFound, with: -> { head :not_found }

  def index
    @samples = Sample.all
  end

  # The sample's page, or with .pdf the sample rendered to PDF: inline, or as a download
  # with ?download=1.
  def show
    @sample = Sample.find(params[:id])
    @rendering = @sample.render

    respond_to do |format|
      format.html
      format.pdf do
        send_data @rendering.pdf, filename: "#{@sample.id}.pdf", type: :pdf,
                                  disposition: params[:download] ? :attachment : :inline
      end
    end
  end

  # The sample's document, as index.html, and its assets, so a browser can show the HTML as
  # rustyprint gets it.
  def file
    sample = Sample.find(params[:sample_id])
    return send_data(sample.html, type: :html, disposition: :inline) if params[:path] == "index.html"

    path = sample.asset(params[:path]) or return head(:not_found)
    send_file path, disposition: :inline, type: Marcel::MimeType.for(path, name: path.basename.to_s)
  end
end
