# CSS at-rule tracker

Implementation status of every CSS at-rule in scope for rustyprint, with their descriptors and the media features that `@media` evaluates. See [plan.md](plan.md) for the plan and milestones.

**Source:** [MDN CSS reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference), using the machine-readable [`mdn/data`](https://github.com/mdn/data) package, version 2.37.2. `@custom-media`, `@function` and `@color-profile`, the `@color-profile` and `@function` descriptors, the page margin boxes, the feature value blocks and the media features come from the MDN reference pages, because that `mdn/data` version does not include them.

## Status values

| Status | Meaning |
| --- | --- |
| `todo` | Not started |
| `partial` | Some forms supported (list the gaps in Notes) |
| `done` | Parsed and applied as described, with fixtures |

## At-rules

| At-rule | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`@charset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@charset) | 2 | done | Sets the encoding of linked stylesheets. |
| [`@import`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@import) | 2 | done | Loads stylesheets from the request assets only. Supports `layer()`, `supports()` and media queries. |
| [`@namespace`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@namespace) | 2 | done |  |
| [`@media`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media) | 2 | done | Evaluated for the `print` media type; see [Media features](#media-features). |
| [`@custom-media`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@custom-media) | 2 | done | Experimental. |
| [`@supports`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@supports) | 2 | done | Answers according to what the engine actually supports. |
| [`@layer`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@layer) | 2 | done |  |
| [`@scope`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@scope) | 2 | done |  |
| [`@page`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@page) | 5 | done | Page selectors `:first`, `:left`, `:right`, `:blank` and named pages. Accepts margin, padding, border, background and other box properties, plus the descriptors and margin boxes below. |
| [`@counter-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style) | 6 | done | The predefined styles are `@counter-style` rules in the UA stylesheet. Later origins, layers and rules win. |
| [`@position-try`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@position-try) | 10 | done | Experimental. Insets, margins, sizes, `position-anchor` and `position-area`; other properties and `!important` are ignored with a warning. |
| [`@font-face`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face) | 16 | done | Font files load like other assets: from the request, the asset directory or allowed hosts. TrueType, OpenType, collections, WOFF and WOFF2. At most 64 files per document. |
| [`@font-feature-values`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-feature-values) | 16 | done |  |
| [`@font-palette-values`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-palette-values) | 16 | done |  |
| [`@container`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@container) | 17 | done | Size, style and scroll-state queries. Container sizes come from laying the document out, which is then styled again until they settle. `style()` tests custom properties; tests of other properties are unknown. Nothing scrolls, sticks or snaps, so `scroll-state()` is false. |
| [`@property`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@property) | 17 | done | Registered properties compute by their syntax, inherit as declared and start at their initial value. Those with numeric, color or transform syntaxes interpolate in animations; others are discrete. |
| [`@function`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@function) | 17 | done | Experimental. Typed parameters with defaults, local custom properties, `result`, and `@media`, `@supports` and `@container` in the body. Locals and results apply in order, and the last `result` wins. |
| [`@color-profile`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@color-profile) | 17 | done | Experimental. ICC profiles of gray, RGB and CMYK colors from the request assets. `color()` converts with them for anything drawn in sRGB, and the PDF paints those colors in the profile itself, embedded as an ICC-based color space. `@color-profile device-cmyk` gives `device-cmyk()` colors its profile in the PDF. |
| [`@keyframes`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@keyframes) | 17 | done | Used only for the state of animations at time zero, with `animation-fill-mode` applied. Keyframe values are computed and then interpolated: numbers in the same places of both values, lengths and percentages through `calc()`, colors in Oklab, transforms and filters from `none` through their identity; other values are discrete. Keyframes on view timeline ranges never apply. |
| [`@starting-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@starting-style) | 17 | done | Never applies: no transitions run. Its rules are checked and dropped. |
| [`@view-transition`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@view-transition) | 17 | done | Never applies: there is no navigation. Its descriptors are checked. |

## Descriptors

### @counter-style

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`additive-symbols`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/additive-symbols) | 6 | done | Strings, identifiers and `url()` images, sized like marker images. Margin boxes leave image symbols out. |
| [`fallback`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/fallback) | 6 | done |  |
| [`negative`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/negative) | 6 | done |  |
| [`pad`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/pad) | 6 | done |  |
| [`prefix`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/prefix) | 6 | done |  |
| [`range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/range) | 6 | done |  |
| [`speak-as`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/speak-as) | 6 | done | No visual effect (parse and compute only) |
| [`suffix`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/suffix) | 6 | done |  |
| [`symbols`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/symbols) | 6 | done | Strings, identifiers and `url()` images, sized like marker images. Margin boxes leave image symbols out. |
| [`system`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@counter-style/system) | 6 | done |  |

### @font-face

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`ascent-override`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/ascent-override) | 16 | done |  |
| [`descent-override`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/descent-override) | 16 | done |  |
| [`font-display`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-display) | 16 | done | No effect: fonts are always loaded before layout. |
| [`font-family`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-family) | 16 | done |  |
| [`font-feature-settings`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-feature-settings) | 16 | done |  |
| [`font-stretch`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-stretch) | 16 | done | `auto` uses the width class of the font file, or the range of its `wdth` axis. |
| [`font-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-style) | 16 | done | A face declared italic or oblique is not slanted again. |
| [`font-variation-settings`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-variation-settings) | 16 | done |  |
| [`font-weight`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/font-weight) | 16 | done | `auto` uses the weight of the font file, or the range of its `wght` axis. |
| [`line-gap-override`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/line-gap-override) | 16 | done |  |
| [`size-adjust`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/size-adjust) | 16 | done |  |
| [`src`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/src) | 16 | done | Experimental. A request asset or `data:` URL. |
| [`unicode-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-face/unicode-range) | 16 | done |  |

### @font-palette-values

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`base-palette`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-palette-values/base-palette) | 16 | done |  |
| [`font-family`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-palette-values/font-family) | 16 | done |  |
| [`override-colors`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@font-palette-values/override-colors) | 16 | done |  |

### @page

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `bleed` | 5 | done | Extends the PDF `BleedBox`. |
| `marks` | 5 | done | Crop and cross marks drawn in the bleed area. |
| [`page-margin-safety`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@page/page-margin-safety) | 5 | done |  |
| [`page-orientation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@page/page-orientation) | 5 | done | Rotates the page in the PDF viewer (`/Rotate`). |
| [`size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@page/size) | 5 | done | Page size and orientation. |

### @property

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`inherits`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@property/inherits) | 17 | done |  |
| [`initial-value`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@property/initial-value) | 17 | done | Must match the syntax without depending on the element. |
| [`syntax`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@property/syntax) | 17 | done | Every syntax component name, keywords, `+` and `#` lists, and `*`. |

### @color-profile

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `components` | 17 | done | Experimental. Checked; relative colors from profiles are not supported. |
| `rendering-intent` | 17 | done | Experimental. A profile without tables for the intent uses its perceptual ones. |
| `src` | 17 | done | Experimental. A request asset or `data:` URL. |

### @function

| Descriptor | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `result` | 17 | done | Experimental. |

## Page margin boxes

Used inside `@page`. Content can use `counter(page)` and `counter(pages)`.

| Margin box | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `@top-left-corner` | 5 | done |  |
| `@top-left` | 5 | done |  |
| `@top-center` | 5 | done |  |
| `@top-right` | 5 | done |  |
| `@top-right-corner` | 5 | done |  |
| `@right-top` | 5 | done |  |
| `@right-middle` | 5 | done |  |
| `@right-bottom` | 5 | done |  |
| `@bottom-right-corner` | 5 | done |  |
| `@bottom-right` | 5 | done |  |
| `@bottom-center` | 5 | done |  |
| `@bottom-left` | 5 | done |  |
| `@bottom-left-corner` | 5 | done |  |
| `@left-bottom` | 5 | done |  |
| `@left-middle` | 5 | done |  |
| `@left-top` | 5 | done |  |

## Feature value blocks

Used inside `@font-feature-values`.

| Block | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `@swash` | 16 | done |  |
| `@annotation` | 16 | done |  |
| `@ornaments` | 16 | done |  |
| `@stylistic` | 16 | done |  |
| `@styleset` | 16 | done |  |
| `@character-variant` | 16 | done |  |
| `@historical-forms` | 16 | done | Its values are not used: `historical-forms` takes no name. |

## Media features

Media queries are evaluated once, against the default page size from the request options. The value column is what each feature reports.

### Media types

| Media type | Milestone | Status | Value |
| --- | --- | --- | --- |
| `all` | 2 | done | Matches. |
| `print` | 2 | done | Matches. |
| `screen` | 2 | done | Does not match. |

### Features

| Feature | Milestone | Status | Value |
| --- | --- | --- | --- |
| [`any-hover`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/any-hover) | 2 | done | `none` |
| [`any-pointer`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/any-pointer) | 2 | done | `none` |
| [`aspect-ratio`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/aspect-ratio) | 2 | done | Width and height of the page area. |
| [`color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/color) | 2 | done | `8` |
| [`color-gamut`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/color-gamut) | 2 | done | `srgb` |
| [`color-index`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/color-index) | 2 | done | `0` |
| [`device-aspect-ratio`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/device-aspect-ratio) | 2 | done | Deprecated. Width and height of the page box. |
| [`device-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/device-height) | 2 | done | Deprecated. Height of the page box. |
| [`device-posture`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/device-posture) | 2 | done | `continuous` |
| [`device-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/device-width) | 2 | done | Deprecated. Width of the page box. |
| [`display-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/display-mode) | 2 | done | `browser` |
| [`dynamic-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/dynamic-range) | 2 | done | `standard` |
| [`forced-colors`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/forced-colors) | 2 | done | `none` |
| [`grid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/grid) | 2 | done | `0` |
| [`height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/height) | 2 | done | Height of the page area. |
| [`hover`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/hover) | 2 | done | `none` |
| [`inverted-colors`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/inverted-colors) | 2 | done | `none` |
| [`monochrome`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/monochrome) | 2 | done | `0` |
| [`orientation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/orientation) | 2 | done | From the page area: `portrait` or `landscape`. |
| [`overflow-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/overflow-block) | 2 | done | `paged` |
| [`overflow-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/overflow-inline) | 2 | done | `none` |
| [`pointer`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/pointer) | 2 | done | `none` |
| [`prefers-color-scheme`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-color-scheme) | 2 | done | `light` |
| [`prefers-contrast`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-contrast) | 2 | done | `no-preference` |
| [`prefers-reduced-data`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-data) | 2 | done | `no-preference` |
| [`prefers-reduced-motion`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-motion) | 2 | done | `no-preference` |
| [`prefers-reduced-transparency`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-transparency) | 2 | done | `no-preference` |
| [`resolution`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/resolution) | 2 | done | `1dppx` (96dpi) |
| [`scan`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/scan) | 2 | done | Never matches: not a TV display. |
| [`scripting`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/scripting) | 2 | done | `none` |
| [`update`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/update) | 2 | done | `none` |
| [`video-dynamic-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/video-dynamic-range) | 2 | done | `standard` |
| [`width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/width) | 2 | done | Width of the page area. |
