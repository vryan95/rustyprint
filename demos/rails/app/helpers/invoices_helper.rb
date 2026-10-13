module InvoicesHelper
  def euros(amount)
    number_to_currency(amount, unit: "€ ", delimiter: ",", separator: ".")
  end
end
