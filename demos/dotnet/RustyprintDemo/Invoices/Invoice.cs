using System.Globalization;

namespace RustyprintDemo.Invoices;

// An invoice the demo makes up, for the invoice document.
public sealed class Invoice
{
    public const decimal VatRate = 0.21m;

    private static readonly (string Description, string Detail, decimal Price)[] Products =
    [
        ("Oak desk, 160 × 80 cm", "Solid oak top, steel frame", 489.00m),
        ("Ergonomic office chair", "Mesh back, adjustable armrests", 329.00m),
        ("Monitor arm, dual", "Gas spring, VESA 75/100", 119.50m),
        ("Desk lamp, LED", "Dimmable, 2700–6500 K", 64.95m),
        ("Filing cabinet, 3 drawers", "Lockable, white", 229.00m),
        ("Whiteboard, 120 × 90 cm", "Magnetic, aluminium frame", 149.00m),
        ("Keyboard, wireless", "Low profile, US international", 79.90m),
        ("Mouse, wireless", "Rechargeable, silent clicks", 44.90m),
        ("USB-C dock", "2 × DisplayPort, 100 W charging", 199.00m),
        ("Headset", "Noise cancelling, Bluetooth", 159.00m),
        ("Cable trays, set of 4", "Under-desk mounting", 46.00m),
        ("Standing mat", "Anti-fatigue, 90 × 60 cm", 59.00m),
    ];

    public static IReadOnlyList<Invoice> All { get; } =
    [
        new("2026-0412", new DateOnly(2026, 10, 12), "PO-88213",
            new Party("Lindqvist & Partners", ["Kungsgatan 44", "111 35 Stockholm", "Sweden"]), lines: 36),
        new("2026-0413", new DateOnly(2026, 10, 13), "PO-1207",
            new Party("Café Moreau", ["12 rue des Martyrs", "75009 Paris", "France"]), lines: 4),
    ];

    private Invoice(string id, DateOnly date, string order, Party customer, int lines)
    {
        Id = id;
        Date = date;
        Order = order;
        Customer = customer;
        var seed = id.Sum(c => c);
        Lines = Enumerable.Range(0, lines).Select(i =>
        {
            var (description, detail, price) = Products[(i * 5 + seed) % Products.Length];
            return new Line(description, detail, i * 7 % 9 + 1, price);
        }).ToArray();
    }

    public static Invoice? Find(string id) => All.FirstOrDefault(invoice => invoice.Id == id);

    public string Id { get; }

    public DateOnly Date { get; }

    public string Order { get; }

    public Party Customer { get; }

    public IReadOnlyList<Line> Lines { get; }

    public DateOnly Due => Date.AddDays(30);

    public decimal Subtotal => Lines.Sum(line => line.Amount);

    public decimal Vat => Math.Round(Subtotal * VatRate, 2);

    public decimal Total => Subtotal + Vat;

    public static string Euros(decimal amount) => "€ " + amount.ToString("#,##0.00", CultureInfo.InvariantCulture);

    public static string LongDate(DateOnly date) => date.ToString("MMMM d, yyyy", CultureInfo.InvariantCulture);

    public sealed record Line(string Description, string Detail, int Quantity, decimal UnitPrice)
    {
        public decimal Amount => Quantity * UnitPrice;
    }

    public sealed record Party(string Name, IReadOnlyList<string> Address);
}
