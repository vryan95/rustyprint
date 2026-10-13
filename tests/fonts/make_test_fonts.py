"""Builds the small test fonts the font fixtures use, with fontTools.

    python -m pip install fonttools brotli
    python tests/fonts/make_test_fonts.py

Every glyph is a rectangle; what a fixture measures is how wide its glyphs are, so each
font gives the glyphs it switches between distinct advance widths. Units per em are 1000.
"""

import os
import struct
import zlib

from fontTools.designspaceLib import AxisDescriptor, DesignSpaceDocument, SourceDescriptor
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables import _s_b_i_x, sbixGlyph, sbixStrike
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools import varLib

ROOT = os.path.join(os.path.dirname(__file__), '..', 'fixtures')
OUT = [os.path.join(ROOT, tracker, 'assets')
       for tracker in ('css-at-rules', 'css-properties', 'css-values')]


def rect(advance, left=50, bottom=0, right=None, top=700):
    pen = TTGlyphPen(None)
    right = advance - 50 if right is None else right
    if right > left:
        pen.moveTo((left, bottom))
        pen.lineTo((left, top))
        pen.lineTo((right, top))
        pen.lineTo((right, bottom))
        pen.closePath()
    return pen.glyph()


def build(family, glyphs, cmap, ascent=800, descent=-200, line_gap=0, style='Regular',
          outlines=None):
    """`glyphs`: name to advance; `cmap`: code point to name; `outlines`: glyph name to
    `(left, right)` for glyphs that are not the usual rectangle. Spaces have no outline."""
    order = ['.notdef'] + [name for name in glyphs if name != '.notdef']
    fb = FontBuilder(1000, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    advances = {'.notdef': 500, **glyphs}
    outlines = {'space': (0, 0), **(outlines or {})}
    shapes = {}
    for name in order:
        left, right = outlines.get(name, (50, advances[name] - 50))
        shapes[name] = rect(advances[name], left=left, right=right)
    fb.setupGlyf(shapes)
    # The left side bearing is where the outline starts, or TrueType moves the outline there.
    fb.setupHorizontalMetrics({
        name: (advances[name], outlines.get(name, (50, 0))[0]) for name in order
    })
    fb.setupHorizontalHeader(ascent=ascent, descent=descent, lineGap=line_gap)
    fb.setupNameTable({'familyName': family, 'styleName': style,
                       'psName': family.replace(' ', '') + '-' + style})
    fb.setupOS2(
        sTypoAscender=ascent,
        sTypoDescender=descent,
        sTypoLineGap=line_gap,
        usWinAscent=ascent,
        usWinDescent=-descent,
        fsSelection=0x80,
        version=4,
    )
    fb.setupPost()
    return fb.font


def save(font, name):
    for out in OUT:
        os.makedirs(out, exist_ok=True)
        font.save(os.path.join(out, name))


# A plain font, also as WOFF and WOFF2.
simple = build('Test Simple', {'space': 250, 'A': 700, 'B': 700, 'heart': 700},
               {0x20: 'space', 0x41: 'A', 0x42: 'B', 0x2764: 'heart'})
save(simple, 'simple.ttf')
for flavor in ('woff', 'woff2'):
    simple.flavor = flavor
    save(simple, f'simple.{flavor}')
simple.flavor = None

# A narrow font, for faces that cover different characters.
save(build('Test Narrow', {'space': 250, 'A': 300, 'B': 300, 'C': 300},
           {0x20: 'space', 0x41: 'A', 0x42: 'B', 0x43: 'C'}), 'narrow.ttf')

# A variable font: each axis changes the advance of `A` by its own amount.
AXES = [
    # tag, min, default, max, the master's position, the advance of `A` there
    ('wght', 100, 100, 900, 900, 1300),
    ('wdth', 50, 100, 100, 50, 250),
    ('opsz', 8, 8, 72, 72, 820),
    ('slnt', -15, 0, 0, -15, 650),
    ('ital', 0, 0, 1, 1, 600),
    ('TEST', 0, 0, 100, 100, 800),
]
doc = DesignSpaceDocument()
for tag, lo, default, hi, _, _ in AXES:
    axis = AxisDescriptor()
    axis.tag, axis.name, axis.minimum, axis.default, axis.maximum = tag, tag, lo, default, hi
    doc.addAxis(axis)


def var_master(advance):
    return build('Test Var', {'space': 250, 'A': advance}, {0x20: 'space', 0x41: 'A'})


masters = [({}, var_master(500))]
for tag, _, _, _, at, advance in AXES:
    masters.append(({tag: at}, var_master(advance)))
for i, (location, font) in enumerate(masters):
    source = SourceDescriptor()
    source.font = font
    source.location = {tag: location.get(tag, default) for tag, _, default, *_ in AXES}
    source.name = f'master{i}'
    doc.addSource(source)
variable, _, _ = varLib.build(doc)
save(variable, 'var.ttf')

# A color font: `A` is two colored layers, `B` one in the text color, `C` a gradient
# (COLRv1), and U+2764 a colored heart. Palette 1 is for light backgrounds, 2 for dark.
color = build('Test Color', {'space': 250, 'A': 1000, 'B': 1000, 'C': 1000, 'heart': 1000,
                             'left': 1000, 'right': 1000},
              {0x20: 'space', 0x41: 'A', 0x42: 'B', 0x43: 'C', 0x2764: 'heart'},
              outlines={'left': (0, 500), 'right': (500, 1000)})
color['COLR'] = buildCOLR({
    'A': [('left', 0), ('right', 1)],
    'B': [('left', 0xFFFF)],
    'heart': [('left', 0), ('right', 1)],
    'C': {
        'Format': 10,  # PaintGlyph
        'Glyph': 'left',
        'Paint': {
            'Format': 4,  # PaintLinearGradient
            'ColorLine': {'ColorStop': [(0.0, 0), (1.0, 1)]},
            'x0': 0, 'y0': 0, 'x1': 500, 'y1': 0, 'x2': 0, 'y2': 500,
        },
    },
})
color['CPAL'] = buildCPAL(
    [
        [(1, 0, 0, 1), (0, 0, 1, 1)],
        [(0, 1, 0, 1), (1, 1, 0, 1)],
        [(0, 0, 0, 1), (1, 1, 1, 1)],
    ],
    paletteTypes=[0, 1, 2],
)
save(color, 'color.ttf')

# A bitmap font: `A` is a 100 × 100 red PNG at 100 pixels per em: one em square.
bitmap = build('Test Bitmap', {'space': 250, 'A': 1000}, {0x20: 'space', 0x41: 'A'})


def solid_png(size, rgb):
    """A `size` × `size` PNG of one color."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack('>I', len(data)) + body + struct.pack('>I', zlib.crc32(body))
    rows = b''.join(b'\x00' + bytes(rgb) * size for _ in range(size))
    header = struct.pack('>IIBBBBB', size, size, 8, 2, 0, 0, 0)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(rows))
            + chunk(b'IEND', b''))


png = solid_png(100, (255, 0, 0))
sbix = newTable('sbix')
strike = sbixStrike.Strike(ppem=100, resolution=72)
for name in bitmap.getGlyphOrder():
    glyph = sbixGlyph.Glyph(glyphName=name, graphicType='png ' if name == 'A' else None,
                            imageData=png if name == 'A' else b'')
    strike.glyphs[name] = glyph
sbix.strikes[100] = strike
bitmap['sbix'] = sbix
save(bitmap, 'bitmap.ttf')

# Alternates for `font-variant-alternates`, each a different width of `a` (or `b`).
features = build('Test Features', {
    'space': 250, 'a': 500, 'b': 500,
    'a.salt1': 600, 'a.salt2': 650, 'a.ss01': 700, 'b.ss02': 750, 'a.swsh1': 800,
    'a.swsh2': 850, 'a.cv1': 900, 'a.cv2': 950, 'a.ornm1': 1000, 'a.nalt1': 1100,
    'a.hist': 1200,
}, {0x20: 'space', 0x61: 'a', 0x62: 'b'})
addOpenTypeFeaturesFromString(features, '''
languagesystem DFLT dflt;
languagesystem latn dflt;
feature salt { sub a from [a.salt1 a.salt2]; } salt;
feature ss01 { sub a by a.ss01; } ss01;
feature ss02 { sub b by b.ss02; } ss02;
feature swsh { sub a from [a.swsh1 a.swsh2]; } swsh;
feature cv01 { sub a from [a.cv1 a.cv2]; } cv01;
feature ornm { sub a from [a.ornm1]; } ornm;
feature nalt { sub a from [a.nalt1]; } nalt;
feature hist { sub a by a.hist; } hist;
''')
save(features, 'features.ttf')
print('wrote test fonts to', ', '.join(OUT))
