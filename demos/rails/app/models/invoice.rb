# An invoice the demo makes up, for the ERB invoice view.
class Invoice
  Line = Data.define(:description, :detail, :quantity, :unit_price) do
    def amount = quantity * unit_price
  end

  Party = Data.define(:name, :address)

  VAT_RATE = BigDecimal("0.21")

  PRODUCTS = [
    [ "Oak desk, 160 × 80 cm", "Solid oak top, steel frame", "489.00" ],
    [ "Ergonomic office chair", "Mesh back, adjustable armrests", "329.00" ],
    [ "Monitor arm, dual", "Gas spring, VESA 75/100", "119.50" ],
    [ "Desk lamp, LED", "Dimmable, 2700–6500 K", "64.95" ],
    [ "Filing cabinet, 3 drawers", "Lockable, white", "229.00" ],
    [ "Whiteboard, 120 × 90 cm", "Magnetic, aluminium frame", "149.00" ],
    [ "Keyboard, wireless", "Low profile, US international", "79.90" ],
    [ "Mouse, wireless", "Rechargeable, silent clicks", "44.90" ],
    [ "USB-C dock", "2 × DisplayPort, 100 W charging", "199.00" ],
    [ "Headset", "Noise cancelling, Bluetooth", "159.00" ],
    [ "Cable trays, set of 4", "Under-desk mounting", "46.00" ],
    [ "Standing mat", "Anti-fatigue, 90 × 60 cm", "59.00" ]
  ].freeze

  attr_reader :id, :date, :order, :customer, :lines

  def self.all
    [
      new(id: "2026-0412", date: Date.new(2026, 10, 12), order: "PO-88213",
          customer: Party.new("Lindqvist & Partners", [ "Kungsgatan 44", "111 35 Stockholm", "Sweden" ]),
          lines: 36),
      new(id: "2026-0413", date: Date.new(2026, 10, 13), order: "PO-1207",
          customer: Party.new("Café Moreau", [ "12 rue des Martyrs", "75009 Paris", "France" ]),
          lines: 4)
    ]
  end

  def self.first = all.first

  def self.find(id)
    all.find { |invoice| invoice.id == id.to_s } or raise ActiveRecord::RecordNotFound, "no invoice #{id}"
  end

  def initialize(id:, date:, order:, customer:, lines:)
    @id = id
    @date = date
    @order = order
    @customer = customer
    @lines = Array.new(lines) do |i|
      description, detail, price = PRODUCTS[(i * 5 + id.hash.abs) % PRODUCTS.size]
      Line.new(description, detail, (i * 7 % 9) + 1, BigDecimal(price))
    end
  end

  def to_param = id

  def due = date + 30

  def subtotal = lines.sum(&:amount)

  def vat = (subtotal * VAT_RATE).round(2)

  def total = subtotal + vat
end
