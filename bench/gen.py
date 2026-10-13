"""Generates the benchmark reports: a long multi-page report and a one-page report.

    python bench/gen.py
"""

import os
import random

OUT = os.path.dirname(os.path.abspath(__file__))
WORDS = ('lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor '
         'incididunt ut labore et dolore magna aliqua enim ad minim veniam quis nostrud '
         'exercitation ullamco laboris nisi aliquip ex ea commodo consequat duis aute irure '
         'in reprehenderit voluptate velit esse cillum fugiat nulla pariatur excepteur sint '
         'occaecat cupidatat non proident sunt culpa qui officia deserunt mollit anim id est '
         'laborum').split()

STYLE = '''
@page {
  size: A4;
  margin: 22mm 18mm 20mm;
  @top-left { content: "Quarterly operations report"; font: 9pt "Noto Sans", sans-serif; color: #666 }
  @bottom-center { content: "Page " counter(page) " of " counter(pages); font: 9pt "Noto Sans", sans-serif }
}
body { font: 10.5pt/1.45 "Noto Serif", serif; color: #222 }
h1 { font: bold 20pt "Noto Sans", sans-serif; color: #12406b;
     break-before: page; border-bottom: 2px solid #12406b; padding-bottom: 4pt }
h1:first-of-type { break-before: auto }
h2 { font: bold 13pt "Noto Sans", sans-serif; color: #12406b; margin: 14pt 0 6pt; break-after: avoid }
p { margin: 0 0 7pt; text-align: justify; hyphens: auto }
table { width: 100%; border-collapse: collapse; margin: 8pt 0 12pt; font: 9pt "Noto Sans", sans-serif }
th { background: #12406b; color: white; text-align: left; padding: 4pt 6pt }
td { padding: 3pt 6pt; border-bottom: 1px solid #ccd }
tr:nth-child(even) td { background: #f1f4f8 }
td.num, th.num { text-align: right }
thead { display: table-header-group }
tr { break-inside: avoid }
ul { margin: 0 0 8pt 18pt; padding: 0 }
li { margin-bottom: 2pt }
.callout { border-left: 4px solid #e08a1e; background: #fdf3e6; padding: 6pt 10pt;
           margin: 8pt 0; border-radius: 0 4pt 4pt 0 }
.kpis { display: flex; gap: 8pt; margin: 8pt 0 }
.kpi { flex: 1; border: 1px solid #ccd; border-radius: 4pt; padding: 6pt; text-align: center }
.kpi b { display: block; font: bold 16pt "Noto Sans", sans-serif; color: #12406b }
'''


def sentence(rng, words=None):
    n = words or rng.randint(8, 22)
    s = ' '.join(rng.choice(WORDS) for _ in range(n))
    return s[0].upper() + s[1:] + '.'


def paragraph(rng):
    return '<p>' + ' '.join(sentence(rng) for _ in range(rng.randint(3, 6))) + '</p>'


def table(rng, rows):
    head = ('<thead><tr><th>Region</th><th>Product</th><th class=num>Units</th>'
            '<th class=num>Revenue</th><th class=num>Margin</th></tr></thead>')
    body = ''.join(
        f'<tr><td>{rng.choice(["North", "South", "East", "West", "Central"])}</td>'
        f'<td>{rng.choice(WORDS).title()} {rng.choice(WORDS)}</td>'
        f'<td class=num>{rng.randint(10, 9999):,}</td>'
        f'<td class=num>€ {rng.randint(1000, 999999):,}.{rng.randint(0, 99):02}</td>'
        f'<td class=num>{rng.uniform(2, 45):.1f}%</td></tr>'
        for _ in range(rows)
    )
    return f'<table>{head}<tbody>{body}</tbody></table>'


def chapter(rng, number):
    parts = [f'<h1>Chapter {number}: {sentence(rng, 3)[:-1]}</h1>']
    parts.append('<div class=kpis>' + ''.join(
        f'<div class=kpi><b>{rng.randint(10, 999)}</b>{rng.choice(WORDS)}</div>'
        for _ in range(4)) + '</div>')
    for section in range(1, 4):
        parts.append(f'<h2>{number}.{section} {sentence(rng, 4)[:-1]}</h2>')
        parts.extend(paragraph(rng) for _ in range(rng.randint(2, 4)))
        if section == 2:
            parts.append(table(rng, rng.randint(12, 30)))
        if section == 3:
            parts.append('<ul>' + ''.join(f'<li>{sentence(rng)}</li>' for _ in range(5)) + '</ul>')
            parts.append(f'<div class=callout>{sentence(rng, 30)}</div>')
    return '\n'.join(parts)


def document(title, body):
    return (f'<!DOCTYPE html><html lang=en><head><meta charset=utf-8><title>{title}</title>'
            f'<style>{STYLE}</style></head><body>{body}</body></html>')


def main():
    rng = random.Random(42)
    chapters = int(os.environ.get('CHAPTERS', '174'))
    body = '\n'.join(chapter(rng, n) for n in range(1, chapters + 1))
    with open(os.path.join(OUT, 'report-long.html'), 'w', encoding='utf-8') as f:
        f.write(document('Quarterly operations report', body))

    rng = random.Random(7)
    one = ('<h1>Weekly summary</h1>'
           + '<div class=kpis>' + ''.join(
               f'<div class=kpi><b>{rng.randint(10, 999)}</b>{rng.choice(WORDS)}</div>'
               for _ in range(4)) + '</div>'
           + '<h2>Highlights</h2>' + paragraph(rng) + paragraph(rng)
           + table(rng, 14)
           + '<div class=callout>' + sentence(rng, 25) + '</div>')
    with open(os.path.join(OUT, 'report-one.html'), 'w', encoding='utf-8') as f:
        f.write(document('Weekly summary', one))
    print('wrote report-long.html with', chapters, 'chapters and report-one.html')


if __name__ == '__main__':
    main()
