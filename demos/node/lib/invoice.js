// The invoices the demo makes up, for the invoice document. Amounts are in cents, so the
// totals add up to the cent.

export const VAT_PERCENT = 21

const PRODUCTS = [
  ['Oak desk, 160 × 80 cm', 'Solid oak top, steel frame', 48900],
  ['Ergonomic office chair', 'Mesh back, adjustable armrests', 32900],
  ['Monitor arm, dual', 'Gas spring, VESA 75/100', 11950],
  ['Desk lamp, LED', 'Dimmable, 2700–6500 K', 6495],
  ['Filing cabinet, 3 drawers', 'Lockable, white', 22900],
  ['Whiteboard, 120 × 90 cm', 'Magnetic, aluminium frame', 14900],
  ['Keyboard, wireless', 'Low profile, US international', 7990],
  ['Mouse, wireless', 'Rechargeable, silent clicks', 4490],
  ['USB-C dock', '2 × DisplayPort, 100 W charging', 19900],
  ['Headset', 'Noise cancelling, Bluetooth', 15900],
  ['Cable trays, set of 4', 'Under-desk mounting', 4600],
  ['Standing mat', 'Anti-fatigue, 90 × 60 cm', 5900]
]

export class Invoice {
  constructor (id, date, order, customer, lines) {
    this.id = id
    this.date = date
    this.order = order
    this.customer = customer
    // The same lines as the .NET demo's: the sum of the id's characters picks the first product.
    const seed = [...id].reduce((sum, char) => sum + char.charCodeAt(0), 0)
    this.lines = Array.from({ length: lines }, (_, i) => {
      const [description, detail, unitPrice] = PRODUCTS[(i * 5 + seed) % PRODUCTS.length]
      const quantity = i * 7 % 9 + 1
      return { description, detail, quantity, unitPrice, amount: quantity * unitPrice }
    })
  }

  static find (id) {
    return INVOICES.find(invoice => invoice.id === id) ?? null
  }

  get due () {
    return new Date(this.date.getTime() + 30 * 24 * 60 * 60 * 1000)
  }

  get subtotal () {
    return this.lines.reduce((sum, line) => sum + line.amount, 0)
  }

  // Rounded to the cent half to even, as .NET's Math.Round does a decimal.
  get vat () {
    const hundredths = this.subtotal * VAT_PERCENT
    const cents = Math.floor(hundredths / 100)
    const rest = hundredths % 100
    return rest > 50 || (rest === 50 && cents % 2 === 1) ? cents + 1 : cents
  }

  get total () {
    return this.subtotal + this.vat
  }
}

export const INVOICES = [
  new Invoice('2026-0412', new Date(Date.UTC(2026, 9, 12)), 'PO-88213',
    { name: 'Lindqvist & Partners', address: ['Kungsgatan 44', '111 35 Stockholm', 'Sweden'] }, 36),
  new Invoice('2026-0413', new Date(Date.UTC(2026, 9, 13)), 'PO-1207',
    { name: 'Café Moreau', address: ['12 rue des Martyrs', '75009 Paris', 'France'] }, 4)
]

// An amount in cents as € 1,316.00.
export function euros (cents) {
  const whole = Math.floor(cents / 100).toLocaleString('en-US')
  return `€ ${whole}.${String(cents % 100).padStart(2, '0')}`
}

// A date as October 12, 2026.
export function longDate (date) {
  return date.toLocaleDateString('en-US', { timeZone: 'UTC', month: 'long', day: 'numeric', year: 'numeric' })
}
