namespace :samples do
  desc "Render every sample to a PDF, in tmp/samples or the directory in OUT"
  task render: :environment do
    out = Pathname(ENV.fetch("OUT") { Rails.root.join("tmp/samples") })
    out.mkpath
    Sample.all.each do |sample|
      rendering = sample.render
      path = out.join("#{sample.id}.pdf")
      path.binwrite(rendering.pdf)
      puts format("%-14s %2d %-5s %5d ms  %s", path.basename, rendering.pages,
                  rendering.pages == 1 ? "page" : "pages", rendering.seconds * 1000,
                  ActiveSupport::NumberHelper.number_to_human_size(rendering.bytes))
      rendering.warnings.each { |warning| puts "  warning: #{warning}" }
    end
  end
end
