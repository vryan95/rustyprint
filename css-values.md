# CSS value tracker

Implementation status of every CSS data type, unit and function in scope for rustyprint. See [plan.md](plan.md) for the plan and milestones.

**Source:** [MDN CSS reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference), using the machine-readable [`mdn/data`](https://github.com/mdn/data) package, version 2.37.2. Units and functions that this `mdn/data` version does not include come from MDN's [`<length>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/length) and [functions](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/Functions) reference pages; they are marked *Not in mdn/data*.

## Status values

| Status | Meaning |
| --- | --- |
| `todo` | Not started |
| `partial` | Some forms supported (list the gaps in Notes) |
| `done` | Parsed, computed and applied as described, with fixtures |

## CSS-wide keywords

| Keyword | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`initial`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/initial) | 2 | done |  |
| [`inherit`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/inherit) | 2 | done |  |
| [`unset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/unset) | 2 | done |  |
| [`revert`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/revert) | 2 | done |  |
| [`revert-layer`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/revert-layer) | 2 | done |  |

## Data types

| Type | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`<angle>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/angle) | 2 | done |  |
| [`<angle-percentage>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/angle-percentage) | 2 | done | Used by conic gradients, whose stops compute to angles. |
| [`<basic-shape>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape) | 10 | done | Used by `shape-outside`, `clip-path`, `offset-path` and `border-shape`. |
| [`<blend-mode>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/blend-mode) | 15 | done |  |
| [`<color>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value) | 2 | done |  |
| [`<custom-ident>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/custom-ident) | 2 | done |  |
| [`<dashed-ident>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/dashed-ident) | 2 | done |  |
| [`<dimension>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/dimension) | 2 | done |  |
| [`<display-box>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-box) | 3 | done |  |
| [`<display-inside>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-inside) | 3 | done |  |
| [`<display-internal>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-internal) | 3 | done |  |
| [`<display-legacy>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-legacy) | 3 | done |  |
| [`<display-listitem>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-listitem) | 3 | done |  |
| [`<display-outside>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/display-outside) | 3 | done |  |
| [`<easing-function>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function) | 17 | done | Used for the state of animations at time zero. |
| [`<filter-function>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function) | 15 | done | Color filters change the colors of what the element paints, so text and shapes stay vectors; colors are filtered in sRGB. |
| [`<flex>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/flex_value) | 12 | done |  |
| [`<frequency>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/frequency) | 2 | done | No property takes it: frequencies only appear in math functions. |
| [`<frequency-percentage>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/frequency-percentage) | 2 | done | No property takes it. |
| [`<gradient>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient) | 7 | done | In backgrounds, `list-style-image` and `content`. Colors interpolate in the `in` color space, in sRGB for legacy colors and otherwise in Oklab. |
| [`<ident>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/ident) | 2 | done |  |
| [`<image>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/image) | 7 | done | URLs, gradients and the image functions. Invalid images paint nothing. |
| [`<integer>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/integer) | 2 | done |  |
| [`<length>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/length) | 2 | done |  |
| [`<length-percentage>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/length-percentage) | 2 | done |  |
| [`<line-style>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/line-style) | 3 | done |  |
| [`<number>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/number) | 2 | done |  |
| [`<overflow>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/overflow_value) | 3 | done |  |
| [`<percentage>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/percentage) | 2 | done |  |
| [`<position>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/position_value) | 2 | done | Used by backgrounds, masks, gradients, `object-position` and the motion path properties. |
| [`<ratio>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/ratio) | 2 | done |  |
| [`<resolution>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/resolution) | 2 | done |  |
| [`<string>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/string) | 2 | done |  |
| [`<text-edge>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/text-edge) | 4 | done |  |
| [`<time>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/time) | 2 | done |  |
| [`<time-percentage>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/time-percentage) | 2 | done | No property takes it. |
| [`<transform-function>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function) | 15 | done |  |
| [`<url>`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/url_value) | 2 | done | Request assets, `data:` URLs and allowed remote hosts; relative URLs resolve against `<base>`. |

## Units

### Absolute lengths

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `px` | 2 | done |  |
| `cm` | 2 | done |  |
| `mm` | 2 | done |  |
| `Q` | 2 | done |  |
| `in` | 2 | done |  |
| `pc` | 2 | done |  |
| `pt` | 2 | done |  |

### Font-relative lengths

Need font metrics; `em` and `rem` work from milestone 2.

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `em` | 2 | done |  |
| `rem` | 2 | done |  |
| `ex` | 4 | done |  |
| `rex` | 4 | done | Not in mdn/data. |
| `ch` | 4 | done |  |
| `rch` | 4 | done | Not in mdn/data. |
| `cap` | 4 | done |  |
| `rcap` | 4 | done | Not in mdn/data. |
| `ic` | 4 | done | Uses the advance of 水, or 1em when the font has none (no bundled font does). |
| `ric` | 4 | done | Not in mdn/data. Uses the advance of 水, or 1em when the font has none. |
| `lh` | 4 | done | Not in mdn/data. |
| `rlh` | 4 | done | Not in mdn/data. |

### Viewport lengths

Resolved against the page area; the `sv*`, `lv*` and `dv*` variants equal the plain units.

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `vw` | 5 | done |  |
| `vh` | 5 | done |  |
| `vi` | 5 | done | Not in mdn/data. |
| `vb` | 5 | done | Not in mdn/data. |
| `vmin` | 5 | done |  |
| `vmax` | 5 | done |  |
| `svw` | 5 | done | Not in mdn/data. |
| `svh` | 5 | done | Not in mdn/data. |
| `svi` | 5 | done | Not in mdn/data. |
| `svb` | 5 | done | Not in mdn/data. |
| `svmin` | 5 | done | Not in mdn/data. |
| `svmax` | 5 | done | Not in mdn/data. |
| `lvw` | 5 | done | Not in mdn/data. |
| `lvh` | 5 | done | Not in mdn/data. |
| `lvi` | 5 | done | Not in mdn/data. |
| `lvb` | 5 | done | Not in mdn/data. |
| `lvmin` | 5 | done | Not in mdn/data. |
| `lvmax` | 5 | done | Not in mdn/data. |
| `dvw` | 5 | done | Not in mdn/data. |
| `dvh` | 5 | done | Not in mdn/data. |
| `dvi` | 5 | done | Not in mdn/data. |
| `dvb` | 5 | done | Not in mdn/data. |
| `dvmin` | 5 | done | Not in mdn/data. |
| `dvmax` | 5 | done | Not in mdn/data. |

### Container query lengths

Resolved against the nearest size container, or the page area when there is none.

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `cqw` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |
| `cqh` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |
| `cqi` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |
| `cqb` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |
| `cqmin` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |
| `cqmax` | 17 | done | Not in mdn/data. Resolved against the content box of the nearest size container in the axis. |

### Angles

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `deg` | 2 | done |  |
| `grad` | 2 | done |  |
| `rad` | 2 | done |  |
| `turn` | 2 | done |  |

### Times

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `s` | 2 | done |  |
| `ms` | 2 | done |  |

### Frequencies

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `Hz` | 2 | done |  |
| `kHz` | 2 | done |  |

### Resolutions

Used by `image-set()`, `image-resolution` and the `resolution` media feature.

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `dpi` | 7 | done |  |
| `dpcm` | 7 | done |  |
| `dppx` | 7 | done |  |
| `x` | 7 | done |  |

### Flex

Grid track sizing.

| Unit | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `fr` | 12 | done |  |

## Functions

### Math

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`calc()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/calc) | 2 | done |  |
| [`calc-size()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/calc-size) | 2 | partial | Experimental. In sizing properties; with an `auto` or intrinsic basis, `size` is the stretch-fit or intrinsic width, or the height of the content as laid out. Flex and grid layout size a `calc-size()` height as `auto`. |
| [`min()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/min) | 2 | done |  |
| [`max()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/max) | 2 | done |  |
| [`clamp()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/clamp) | 2 | done |  |
| [`round()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/round) | 2 | done |  |
| [`mod()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/mod) | 2 | done |  |
| [`rem()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/rem) | 2 | done |  |
| `progress()` | 2 | done | Not in mdn/data. |
| [`sin()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/sin) | 2 | done |  |
| [`cos()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/cos) | 2 | done |  |
| [`tan()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/tan) | 2 | done |  |
| [`asin()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/asin) | 2 | done |  |
| [`acos()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/acos) | 2 | done |  |
| [`atan()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/atan) | 2 | done |  |
| [`atan2()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/atan2) | 2 | done |  |
| [`pow()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/pow) | 2 | done |  |
| [`sqrt()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/sqrt) | 2 | done |  |
| [`hypot()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/hypot) | 2 | done |  |
| [`log()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/log) | 2 | done |  |
| [`exp()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/exp) | 2 | done |  |
| [`abs()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/abs) | 2 | done |  |
| [`sign()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/sign) | 2 | done |  |
| `random()` | 2 | partial | Not in mdn/data. Seeded from the element and the expression, so output stays deterministic. It resolves where lengths do; in numbers, angles and times, which compute when parsed, it makes the declaration invalid. |

### Tree counting

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `sibling-index()` | 2 | done | Not in mdn/data. |
| `sibling-count()` | 2 | done | Not in mdn/data. |

### References and conditionals

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`var()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/var) | 2 | done |  |
| [`attr()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/attr) | 2 | done |  |
| [`env()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/env) | 2 | done | Safe-area insets are `0`. |
| `if()` | 2 | done | Not in mdn/data. `media()`, `supports()` and `style()` tests of custom properties, with `not`, `and` and `or`. |
| `type()` | 2 | done | Not in mdn/data. In `attr()`, where the property validates the value, and in `@function` parameter and result types. |
| `url()` | 2 | done | Not in mdn/data. Request assets, `data:` URLs and allowed remote hosts; relative URLs resolve against `<base>`. |
| [`layer()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/@import/layer_function) | 2 | done | Used in `@import`. |
| [`param()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/param) | 2 | parsed | Used by `link-parameters`. |

### Color

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`rgb()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/rgb) | 3 | done | Includes the legacy `rgba()` alias. |
| [`hsl()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/hsl) | 3 | done | Includes the legacy `hsla()` alias. |
| [`hwb()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/hwb) | 3 | done |  |
| [`lab()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/lab) | 3 | done |  |
| [`lch()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/lch) | 3 | done |  |
| [`oklab()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/oklab) | 3 | done |  |
| [`oklch()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/oklch) | 3 | done |  |
| [`color()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/color) | 3 | done |  |
| [`color-mix()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/color-mix) | 3 | done |  |
| [`alpha()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/alpha) | 3 | done |  |
| `contrast-color()` | 3 | done | Not in mdn/data. |
| `device-cmyk()` | 3 | done | Not in mdn/data. Written to the PDF as native CMYK, or in the profile of `@color-profile device-cmyk`. |
| [`light-dark()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/light-dark) | 3 | done | Follows the used color scheme: light, unless `color-scheme` allows only dark. |
| `dynamic-range-limit-mix()` | 3 | done | Not in mdn/data. No visual effect: the PDF is standard dynamic range. |

### Images

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`linear-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/linear-gradient) | 7 | done | PDF axial shading. |
| [`radial-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/radial-gradient) | 7 | done | PDF radial shading; ellipses are scaled circles. |
| [`conic-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/conic-gradient) | 7 | done | Drawn as an image at twice the box resolution. |
| [`repeating-linear-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/repeating-linear-gradient) | 7 | done |  |
| [`repeating-radial-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/repeating-radial-gradient) | 7 | done |  |
| [`repeating-conic-gradient()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/gradient/repeating-conic-gradient) | 7 | done | Drawn as an image at twice the box resolution. |
| [`image()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/image/image) | 7 | done | `ltr`/`rtl` flip the image in elements of the other direction; `#xywh=` media fragments (pixels or `percent:`) crop it; the color shows when the image fails, or alone as a solid color image. |
| [`image-set()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/image/image-set) | 7 | done | Print has no device resolution, so the sharpest option whose `type()` is supported wins, falling back to the next if it does not load. Also `-webkit-image-set()`. |
| [`cross-fade()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/cross-fade) | 7 | done | Images and colors, drawn over each other with opacities that give each its fraction; exact for opaque images. The natural size is the weighted average. |
| [`element()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/element) | 7 | partial | Experimental. Paints the element's first box, clipped to its border box, in backgrounds. Not in `content` or `list-style-image`, which are laid out before the element. |
| [`paint()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/image/paint) | 7 | done | Always an invalid image, which paints nothing: paint worklets need scripting. |

### Counters and generated content

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`counter()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/counter) | 6 | done |  |
| [`counters()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/counters) | 6 | done |  |
| [`symbols()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/symbols) | 6 | partial | Strings. Image symbols work in `@counter-style` but not here. |
| `target-counter()` | 6 | done | Experimental. Page numbers in tables of contents and cross-references; layout runs again until they settle. |
| `target-counters()` | 6 | done | Experimental. |
| `target-text()` | 6 | done | Experimental. |
| `leader()` | 6 | done | Experimental. Leaders share the space left on their line; the pattern repeats a whole number of times. |

### Shapes

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`circle()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/circle) | 10 | done |  |
| [`ellipse()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/ellipse) | 10 | done |  |
| [`inset()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/inset) | 10 | done | With `round` corners. |
| [`rect()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/rect) | 10 | done | `auto` edges are on the reference box. |
| [`xywh()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/xywh) | 10 | done |  |
| [`polygon()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/polygon) | 10 | done | The fill rule is parsed; only the shape's extent matters for floats. |
| [`path()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/basic-shape/path) | 10 | done | SVG path data, with curves and arcs flattened. |
| `shape()` | 10 | done | Not in mdn/data. All commands, with curves and arcs flattened. |

### Anchor positioning

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`anchor()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/anchor) | 10 | done | Experimental. In insets, also inside math functions, with every side keyword and a fallback; without an anchor or fallback, the inset is `auto`. It parses wherever lengths do, and resolves only for absolutely positioned boxes. |
| [`anchor-size()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/anchor-size) | 10 | done | Experimental. In sizes, insets and margins; without a size keyword, the size along the property's axis. |

### Grid

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `repeat()` | 12 | done | Not in mdn/data. |
| [`minmax()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/minmax) | 12 | done |  |
| [`fit-content()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/fit-content_function) | 12 | done |  |

### Transforms

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`translate()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/translate) | 15 | done |  |
| [`translate3d()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/translate3d) | 15 | done |  |
| [`translateX()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/translateX) | 15 | done |  |
| [`translateY()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/translateY) | 15 | done |  |
| [`translateZ()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/translateZ) | 15 | done |  |
| [`rotate()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/rotate) | 15 | done |  |
| [`rotate3d()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/rotate3d) | 15 | done |  |
| [`rotateX()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/rotateX) | 15 | done |  |
| [`rotateY()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/rotateY) | 15 | done |  |
| [`rotateZ()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/rotateZ) | 15 | done |  |
| [`scale()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/scale) | 15 | done |  |
| [`scale3d()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/scale3d) | 15 | done |  |
| [`scaleX()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/scaleX) | 15 | done |  |
| [`scaleY()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/scaleY) | 15 | done |  |
| [`scaleZ()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/scaleZ) | 15 | done |  |
| [`skew()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/skew) | 15 | done |  |
| [`skewX()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/skewX) | 15 | done |  |
| [`skewY()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/skewY) | 15 | done |  |
| [`matrix()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/matrix) | 15 | done |  |
| [`matrix3d()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/matrix3d) | 15 | done |  |
| [`perspective()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/transform-function/perspective) | 15 | done | Drawn with the affine map that best fits the projected corners of the box. |

### Filters

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`blur()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/blur) | 15 | done | Rasterised. |
| [`brightness()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/brightness) | 15 | done |  |
| [`contrast()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/contrast) | 15 | done |  |
| [`drop-shadow()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/drop-shadow) | 15 | done | Rasterised when blurred. |
| [`grayscale()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/grayscale) | 15 | done |  |
| [`hue-rotate()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/hue-rotate) | 15 | done |  |
| [`invert()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/invert) | 15 | done |  |
| [`opacity()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/opacity) | 15 | done |  |
| [`saturate()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/saturate) | 15 | done |  |
| [`sepia()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/filter-function/sepia) | 15 | done |  |

### Other visual effects

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`ray()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/ray) | 15 | done | Motion path offsets. `contain` is ignored. |
| `superellipse()` | 15 | done | Not in mdn/data. Corner shapes, drawn as polylines. |

### Fonts

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `stylistic()` | 16 | done | Not in mdn/data. |
| `styleset()` | 16 | done | Not in mdn/data. |
| `character-variant()` | 16 | done | Not in mdn/data. |
| `swash()` | 16 | done | Not in mdn/data. |
| `ornaments()` | 16 | done | Not in mdn/data. |
| `annotation()` | 16 | done | Not in mdn/data. |
| [`palette-mix()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/font-palette/palette-mix) | 16 | done |  |

### Easing

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`linear()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function/linear) | 17 | done | Used for the state of animations at time zero. |
| [`cubic-bezier()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function/cubic-bezier) | 17 | done | Used for the state of animations at time zero. |
| [`steps()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function/steps) | 17 | done | Used for the state of animations at time zero. |

### Scroll-driven animation

| Function | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`scroll()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/animation-timeline/scroll) | 17 | done | Experimental. Nothing scrolls, so animations on scroll timelines do not apply. |
| [`view()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/animation-timeline/view) | 17 | done | Experimental. Nothing scrolls, so animations on view timelines do not apply. |
