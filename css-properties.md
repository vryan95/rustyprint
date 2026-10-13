# CSS property tracker

Implementation status of every CSS property in scope for rustyprint. See [plan.md](plan.md) for the plan and milestones.

**Source:** [MDN CSS reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference), using the machine-readable [`mdn/data`](https://github.com/mdn/data) package, version 2.37.2.

**Scope:** 505 properties: the standard and experimental properties that apply to HTML content, plus legacy aliases still in common use.

## Status values

| Status | Meaning |
| --- | --- |
| `todo` | Not started |
| `parsed` | Parsed and computed; no layout or paint yet |
| `partial` | Some values supported (list the gaps in Notes) |
| `done` | All values supported, with fixtures |

The PDF is a static snapshot. Animations and transitions render in their state at time zero. Properties marked *No visual effect* only matter for scrolling, interaction or browser UI, so they are `done` once they are parsed and computed.

## CSS Anchor Positioning

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`anchor-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/anchor-name) | 10 | done | Experimental. Anchors are found among the fragments of the positioned box's containing block, so an anchor on another page, or outside the containing block, is not found; the last one in tree order wins. |
| `anchor-scope` | 10 | done | Experimental. Scoped names are only visible to positioned boxes inside the scoping element. |
| [`position-anchor`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-anchor) | 10 | done | Experimental. `auto` is the implicit anchor, which a static document never has. |
| [`position-area`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-area) | 10 | done | Experimental. The area of the grid around the default anchor becomes the containing block, and the box aligns toward the anchor, or centers on it along the anchor or across the whole axis. `align-self` and `justify-self` do not change it. |
| [`position-try`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-try) | 10 | done | Shorthand; Experimental |
| [`position-try-fallbacks`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-try-fallbacks) | 10 | done | Experimental. When the box overflows its containing block, `@position-try` rules, `flip-block`, `flip-inline` and `flip-start` (which also flip the sides `anchor()` names) and `position-area` options are tried in order; the base position stays if none fits. |
| [`position-try-order`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-try-order) | 10 | done | Experimental. Options with the most room in the inset-modified containing block are tried first. |
| [`position-visibility`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-visibility) | 10 | done | Experimental. `anchors-visible` checks the default anchor's `visibility`, not whether it is clipped. |

## CSS Animations

**PDF implementation:** Animations render in their state at time zero, with `animation-fill-mode` applied.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`animation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation) | 17 | done | Shorthand. Rendered in its time-zero state (see `@keyframes`). It resets `animation-composition`, `animation-range` and `animation-trigger`, and sets `animation-timeline` only to `scroll()` or `view()`. |
| [`animation-composition`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-composition) | 17 | done | Rendered in its time-zero state. `add` and `accumulate` add numbers in the same places of both values and append transform lists; other values replace. |
| [`animation-delay`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-delay) | 17 | done | Rendered in its time-zero state: a negative delay starts the animation part-way through. |
| [`animation-direction`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-direction) | 17 | done | Rendered in its time-zero state |
| [`animation-duration`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-duration) | 17 | done | Rendered in its time-zero state. `auto` is `0s` on the document timeline. |
| [`animation-fill-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-fill-mode) | 17 | done | Rendered in its time-zero state: it decides whether an animation that starts later, or has ended, shows a keyframe. |
| [`animation-iteration-count`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-iteration-count) | 17 | done | Rendered in its time-zero state |
| [`animation-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-name) | 17 | done | Rendered in its time-zero state. Names without an `@keyframes` rule run no animation. |
| [`animation-play-state`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-play-state) | 17 | done | Rendered in its time-zero state, whether paused or running. |
| [`animation-timeline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-timeline) | 17 | done | Experimental. Rendered in its time-zero state on the document timeline (`auto`). Scroll and view timelines never move without scrolling, so animations on them, or on `none`, do not apply. |
| [`animation-timing-function`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-timing-function) | 17 | done | Rendered in its time-zero state. It eases each interval between keyframes, unless the keyframe that starts the interval sets its own. |
| [`animation-trigger`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-trigger) | 17 | done | Rendered in its time-zero state: triggers never fire. |
| [`timeline-trigger`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger) | 17 | done | Shorthand; Rendered in its time-zero state: triggers never fire. |
| [`timeline-trigger-activation-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-activation-range) | 17 | done | Shorthand; Rendered in its time-zero state |
| [`timeline-trigger-activation-range-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-activation-range-end) | 17 | done | Rendered in its time-zero state |
| [`timeline-trigger-activation-range-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-activation-range-start) | 17 | done | Rendered in its time-zero state |
| [`timeline-trigger-active-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-active-range) | 17 | done | Shorthand; Rendered in its time-zero state |
| [`timeline-trigger-active-range-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-active-range-end) | 17 | done | Rendered in its time-zero state |
| [`timeline-trigger-active-range-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-active-range-start) | 17 | done | Rendered in its time-zero state |
| [`timeline-trigger-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-name) | 17 | done | Rendered in its time-zero state |
| [`timeline-trigger-source`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-trigger-source) | 17 | done | Rendered in its time-zero state |
| [`trigger-scope`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/trigger-scope) | 17 | done | Rendered in its time-zero state |

## CSS Backgrounds and Borders

**PDF implementation:** Paint. Shadow blur is rasterised with `tiny-skia`.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`background`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background) | 3 | done | Shorthand |
| [`background-attachment`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-attachment) | 3 | done | `fixed` layers are positioned in the page area, so they repeat on every page. |
| [`background-clip`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-clip) | 3 | done |  |
| [`background-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-color) | 3 | done |  |
| [`background-image`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-image) | 3 | done |  |
| [`background-origin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-origin) | 3 | done |  |
| [`background-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-position) | 3 | done | Shorthand |
| [`background-position-x`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-position-x) | 3 | done |  |
| [`background-position-y`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-position-y) | 3 | done |  |
| [`background-repeat`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-repeat) | 3 | done |  |
| [`background-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-size) | 3 | done |  |
| [`border`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border) | 3 | done | Shorthand |
| [`border-bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom) | 3 | done | Shorthand |
| [`border-bottom-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom-color) | 3 | done |  |
| [`border-bottom-left-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom-left-radius) | 3 | done |  |
| [`border-bottom-right-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom-right-radius) | 3 | done |  |
| [`border-bottom-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom-style) | 3 | done |  |
| [`border-bottom-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-bottom-width) | 3 | done |  |
| [`border-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-color) | 3 | done | Shorthand |
| [`border-image`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image) | 15 | done | Shorthand. `border` resets it. |
| [`border-image-outset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image-outset) | 15 | done |  |
| [`border-image-repeat`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image-repeat) | 15 | done |  |
| [`border-image-slice`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image-slice) | 15 | done |  |
| [`border-image-source`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image-source) | 15 | done | Images and gradients. A border image replaces the border styles; the border widths still size it. |
| [`border-image-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-image-width) | 15 | done |  |
| [`border-left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-left) | 3 | done | Shorthand |
| [`border-left-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-left-color) | 3 | done |  |
| [`border-left-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-left-style) | 3 | done |  |
| [`border-left-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-left-width) | 3 | done |  |
| [`border-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-radius) | 3 | done | Shorthand |
| [`border-right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-right) | 3 | done | Shorthand |
| [`border-right-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-right-color) | 3 | done |  |
| [`border-right-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-right-style) | 3 | done |  |
| [`border-right-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-right-width) | 3 | done |  |
| [`border-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-shape) | 15 | done | Experimental. With one shape the border is stroked along it, with two it fills the area between them; either way in the top border's width and color. The background is clipped to the shape, or the inner one. |
| [`border-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-style) | 3 | done | Shorthand |
| [`border-top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top) | 3 | done | Shorthand |
| [`border-top-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top-color) | 3 | done |  |
| [`border-top-left-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top-left-radius) | 3 | done |  |
| [`border-top-right-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top-right-radius) | 3 | done |  |
| [`border-top-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top-style) | 3 | done |  |
| [`border-top-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-top-width) | 3 | done |  |
| [`border-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-width) | 3 | done | Shorthand |
| [`box-shadow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/box-shadow) | 15 | done | Blurred shadows are rasterised. |
| [`corner-block-end-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-block-end-shape) | 15 | done | Shorthand |
| [`corner-block-start-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-block-start-shape) | 15 | done | Shorthand |
| [`corner-bottom-left-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-bottom-left-shape) | 15 | done |  |
| [`corner-bottom-right-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-bottom-right-shape) | 15 | done |  |
| [`corner-bottom-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-bottom-shape) | 15 | done | Shorthand |
| [`corner-end-end-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-end-end-shape) | 15 | done |  |
| [`corner-end-start-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-end-start-shape) | 15 | done |  |
| [`corner-inline-end-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-inline-end-shape) | 15 | done | Shorthand |
| [`corner-inline-start-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-inline-start-shape) | 15 | done | Shorthand |
| [`corner-left-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-left-shape) | 15 | done | Shorthand |
| [`corner-right-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-right-shape) | 15 | done | Shorthand |
| [`corner-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-shape) | 15 | done | Shorthand; Experimental. Corners other than `round` are drawn as polylines. |
| [`corner-start-end-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-start-end-shape) | 15 | done |  |
| [`corner-start-start-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-start-start-shape) | 15 | done |  |
| [`corner-top-left-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-top-left-shape) | 15 | done |  |
| [`corner-top-right-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-top-right-shape) | 15 | done |  |
| [`corner-top-shape`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/corner-top-shape) | 15 | done | Shorthand |

## CSS Basic User Interface

**PDF implementation:** Form control drawing. Interaction properties have no visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`accent-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/accent-color) | 9 | done | Colors checked checkboxes and radio buttons, `progress` bars and range sliders; marks on it are black or white for contrast. |
| [`appearance`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/appearance) | 9 | done | No native widgets: `auto` and the compatibility keywords draw controls like browser defaults; `none` leaves checkboxes, radio buttons, sliders and drop-down arrows undrawn; `base` shows `::checkmark` in checkboxes and radio buttons. |
| `caret` | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`caret-animation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/caret-animation) | 17 | done | No visual effect (parse and compute only) |
| [`caret-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/caret-color) | 17 | done | No visual effect (parse and compute only) |
| `caret-shape` | 17 | done | No visual effect (parse and compute only) |
| [`cursor`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/cursor) | 17 | done | No visual effect (parse and compute only) |
| [`field-sizing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/field-sizing) | 9 | done | Experimental. `content` sizes text fields, text areas and drop-downs to what they show instead of `size`, `cols` and `rows`. |
| [`interactivity`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interactivity) | 17 | done | No visual effect (parse and compute only) |
| [`interest-delay`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interest-delay-end) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`interest-delay-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interest-delay-end) | 17 | done | No visual effect (parse and compute only) |
| [`interest-delay-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interest-delay-start) | 17 | done | No visual effect (parse and compute only) |
| [`outline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/outline) | 3 | done | Shorthand |
| [`outline-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/outline-color) | 3 | done |  |
| [`outline-offset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/outline-offset) | 3 | done |  |
| [`outline-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/outline-style) | 3 | done |  |
| [`outline-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/outline-width) | 3 | done |  |
| [`pointer-events`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events) | 17 | done | No visual effect (parse and compute only) |
| [`resize`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/resize) | 17 | done | No visual effect (parse and compute only) |
| [`user-select`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/user-select) | 17 | done | No visual effect (parse and compute only) |
| [`window-drag`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/window-drag) | 17 | done | No visual effect (parse and compute only) |

## CSS Box Alignment

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`align-content`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/align-content) | 11 | done | Flex lines of multi-line containers, grid rows, and the content of block containers taller than it. |
| [`align-items`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/align-items) | 11 | done | Flex and grid items; `anchor-center` centers them. |
| [`align-self`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/align-self) | 11 | done | Flex and grid items, with `normal` stretching boxes without an aspect ratio; absolutely positioned boxes between two insets. `anchor-center` centers items; a `position-area` aligns its box itself. |
| [`column-gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-gap) | 11 | done | Between flex items in a row, between the lines of a column flex container, between grid columns, and between the columns of a multi-column container, where `normal` is 1em. |
| [`gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/gap) | 11 | done | Shorthand |
| [`justify-content`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/justify-content) | 11 | done | Flex items along the main axis, after auto margins, and grid columns. |
| [`justify-items`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/justify-items) | 11 | partial | The default `justify-self` of grid items, with `legacy` inherited as specified. Block-level boxes and absolutely positioned boxes do not take it from their parent. |
| [`justify-self`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/justify-self) | 11 | done | Grid items, block-level boxes and absolutely positioned boxes between two insets; flex layout ignores it, as specified. A `position-area` aligns its box itself. |
| [`place-content`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/place-content) | 11 | done | Shorthand |
| [`place-items`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/place-items) | 11 | done | Shorthand |
| [`place-self`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/place-self) | 11 | done | Shorthand |
| [`row-gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/row-gap) | 11 | done | Between flex lines in a row container, between items in a column, between grid rows, and between rows of columns, where `normal` is 1em. Percentages of an indefinite height are 0. |

## CSS Box Model

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`margin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin) | 3 | done | Shorthand |
| [`margin-bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-bottom) | 3 | done |  |
| [`margin-left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-left) | 3 | done |  |
| [`margin-right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-right) | 3 | done |  |
| [`margin-top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-top) | 3 | done |  |
| [`margin-trim`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-trim) | 3 | done | Experimental. Block containers trim their block-start and block-end children's margins, and flex and grid containers the margins of items against their edges. |
| [`padding`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding) | 3 | done | Shorthand |
| [`padding-bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-bottom) | 3 | done |  |
| [`padding-left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-left) | 3 | done |  |
| [`padding-right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-right) | 3 | done |  |
| [`padding-top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-top) | 3 | done |  |

## CSS Box Sizing

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`aspect-ratio`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/aspect-ratio) | 3 | done |  |
| [`box-sizing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/box-sizing) | 3 | done |  |
| [`contain-intrinsic-block-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain-intrinsic-block-size) | 3 | done | Sizes boxes with size containment. `auto` has no remembered size on paper, so the length applies. |
| [`contain-intrinsic-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain-intrinsic-height) | 3 | done | Sizes boxes with size containment. `auto` has no remembered size on paper, so the length applies. |
| [`contain-intrinsic-inline-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain-intrinsic-inline-size) | 3 | done | Sizes boxes with size or inline-size containment. `auto` has no remembered size on paper, so the length applies. |
| [`contain-intrinsic-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain-intrinsic-size) | 3 | done | Shorthand. Sizes boxes with size containment. |
| [`contain-intrinsic-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain-intrinsic-width) | 3 | done | Sizes boxes with size or inline-size containment. `auto` has no remembered size on paper, so the length applies. |
| [`frame-sizing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/frame-sizing) | 3 | done | Experimental. An `iframe` with an auto width or height takes the framed document's max-content width or laid-out height. |
| [`height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/height) | 3 | done |  |
| [`max-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/max-height) | 3 | done |  |
| [`max-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/max-width) | 3 | done |  |
| [`min-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/min-height) | 3 | done |  |
| [`min-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/min-width) | 3 | done |  |
| [`width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/width) | 3 | done |  |

## CSS Cascading and Inheritance

**PDF implementation:** Cascade.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`all`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/all) | 2 | done |  |

## CSS Color

**PDF implementation:** Paint.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/color) | 3 | done |  |
| [`color-scheme`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/color-scheme) | 3 | done |  |
| [`dynamic-range-limit`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/dynamic-range-limit) | 3 | done | No visual effect: the PDF is standard dynamic range. |
| [`forced-color-adjust`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/forced-color-adjust) | 3 | done | No visual effect: forced colors are never active. |
| [`opacity`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/opacity) | 15 | done | A transparency group, so overlapping descendants do not show through each other. |
| [`print-color-adjust`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/print-color-adjust) | 3 | done | Backgrounds and colors always print, as with `exact`. |

## CSS Conditional Rules

**PDF implementation:** Container queries, evaluated against layout results.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`container`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/container) | 17 | done | Shorthand. |
| [`container-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/container-name) | 17 | done | Names the element for `@container` conditions. |
| [`container-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/container-type) | 17 | done | `size` and `inline-size` make the element a size container, with size or inline-size containment and style containment; queries see its laid out content box. `scroll-state` queries are always false. |

## CSS Containment

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`contain`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/contain) | 17 | done | Size and inline-size containment size the box as if empty, from `contain-intrinsic-*`; layout and paint containment make it a formatting context, a stacking context and the containing block of positioned descendants; paint containment clips at its overflow clip edge; style containment keeps counter changes and quote depth inside it. Size containment does not apply to tables. |
| [`content-visibility`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/content-visibility) | 17 | done | `hidden` skips the contents, with size, layout, style and paint containment. `auto` renders the contents, which are all relevant on paper, with layout, style and paint containment. |

## CSS Custom Properties for Cascading Variables

**PDF implementation:** Cascade.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`--*`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/--*) | 2 | done |  |

## CSS Display

**PDF implementation:** Box tree generation.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`display`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/display) | 3 | partial | Block, inline, flow-root, inline-block, list-item, contents, none, table, inline-table and the table-internal values lay out; flex, inline-flex, grid and inline-grid lay out their containers; ruby lays out ruby containers, whose `ruby-text` children annotate their bases. |
| [`order`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/order) | 11 | done | Orders flex and grid items, for placement and for painting. |
| [`reading-flow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/reading-flow) | 11 | done | Experimental. No visual effect (parse and compute only) |
| [`reading-order`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/reading-order) | 11 | done | Experimental. No visual effect (parse and compute only) |
| [`visibility`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/visibility) | 3 | done |  |

## CSS Flexible Box Layout

**PDF implementation:** Layout, including fragmentation across pages.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`flex`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex) | 11 | done | Shorthand |
| [`flex-basis`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-basis) | 11 | done |  |
| [`flex-direction`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-direction) | 11 | done |  |
| [`flex-flow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-flow) | 11 | done | Shorthand |
| [`flex-grow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-grow) | 11 | done |  |
| [`flex-line-count`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-line-count) | 11 | done | Experimental. Lines are balanced so the sum of their squared lengths is least. |
| [`flex-shrink`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-shrink) | 11 | done |  |
| [`flex-wrap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/flex-wrap) | 11 | done | Includes the experimental `balance`. Lines break across pages, and so do the items in them. |

## CSS Fonts

**PDF implementation:** Shaping with `rustybuzz`. Variable fonts are instanced and color glyphs are drawn.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`font`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font) | 4 | done | Shorthand |
| [`font-family`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-family) | 4 | done | The bundled Noto Serif, Sans and Mono and `@font-face` families, with per-character fallback through the list and then the bundled families. |
| [`font-feature-settings`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-feature-settings) | 4 | done |  |
| [`font-kerning`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-kerning) | 4 | done |  |
| [`font-language-override`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-language-override) | 4 | done |  |
| [`font-optical-sizing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-optical-sizing) | 16 | done | `auto` sets the `opsz` axis to the font size in CSS pixels. |
| [`font-palette`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-palette) | 16 | done | For `COLR` fonts; `light` and `dark` use the first palette of that type. |
| [`font-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-size) | 4 | done |  |
| [`font-size-adjust`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-size-adjust) | 4 | done |  |
| [`font-stretch`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-stretch) | 4 | done | Legacy alias of `font-width`. |
| [`font-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-style) | 4 | done | Italic faces, or a synthesized slant where a family has none (Noto Sans Mono). |
| [`font-synthesis`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-synthesis) | 4 | done |  |
| [`font-synthesis-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-synthesis-position) | 4 | done | Experimental |
| [`font-synthesis-small-caps`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-synthesis-small-caps) | 4 | done |  |
| [`font-synthesis-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-synthesis-style) | 4 | done |  |
| [`font-synthesis-weight`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-synthesis-weight) | 4 | done | Every bundled family has a bold face, so weight is only synthesized for fonts that lack one. |
| [`font-variant`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant) | 4 | done | Shorthand |
| [`font-variant-alternates`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-alternates) | 16 | done | Names without values in `@font-feature-values` for the font's family are ignored. |
| [`font-variant-caps`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-caps) | 4 | done |  |
| [`font-variant-east-asian`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-east-asian) | 4 | done | Applied as OpenType features; the bundled fonts have no East Asian glyphs. |
| [`font-variant-emoji`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-emoji) | 16 | done | Picks the first font in the list that draws the character in color, or without color. `unicode` uses an approximation of the `Emoji_Presentation` property. |
| [`font-variant-ligatures`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-ligatures) | 4 | done |  |
| [`font-variant-numeric`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-numeric) | 4 | done |  |
| [`font-variant-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variant-position) | 4 | done |  |
| [`font-variation-settings`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-variation-settings) | 16 | done | Values are clamped to the axes' ranges. Variable fonts are embedded at each instance they are used at. |
| [`font-weight`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/font-weight) | 4 | done | Selects `@font-face` faces and sets the `wght` axis of variable fonts. The bundled faces are regular (400) and bold (700); other weights use the nearest of the two. |
| `font-width` | 4 | done | Experimental. Selects `@font-face` faces and sets the `wdth` axis of variable fonts; the bundled faces have one width. |

## CSS Fragmentation

**PDF implementation:** Pagination.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`box-decoration-break`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/box-decoration-break) | 5 | done |  |
| [`break-after`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/break-after) | 5 | done | As `break-before`. |
| [`break-before`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/break-before) | 5 | done | `column` and `avoid-column` apply inside multi-column containers, where page breaks break columns too. |
| [`break-inside`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/break-inside) | 5 | done | `avoid-column` keeps a box in one column. |
| [`orphans`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/orphans) | 5 | done |  |
| [`widows`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/widows) | 5 | done |  |

## CSS Generated Content

**PDF implementation:** Box tree generation.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`content`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/content) | 6 | done | Strings, counters, quotes, images, targets and leaders in `::before`, `::after` and `::marker`, and strings and counters in page-margin boxes. |
| [`quotes`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/quotes) | 6 | done | `auto` uses the quotation marks of the content language. |

## CSS Grid Layout

**PDF implementation:** Layout, including subgrid and fragmentation across pages.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`grid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid) | 12 | done | Shorthand |
| [`grid-area`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-area) | 12 | done | Shorthand |
| [`grid-auto-columns`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-auto-columns) | 12 | done |  |
| [`grid-auto-flow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-auto-flow) | 12 | done |  |
| [`grid-auto-rows`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-auto-rows) | 12 | done |  |
| [`grid-column`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-column) | 12 | done | Shorthand |
| [`grid-column-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-column-end) | 12 | done |  |
| [`grid-column-gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-gap) | 12 | done | Legacy alias of `column-gap` |
| [`grid-column-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-column-start) | 12 | done |  |
| [`grid-gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/gap) | 12 | done | Shorthand; Legacy alias of `gap` |
| [`grid-row`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-row) | 12 | done | Shorthand |
| [`grid-row-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-row-end) | 12 | done |  |
| [`grid-row-gap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/row-gap) | 12 | done | Legacy alias of `row-gap` |
| [`grid-row-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-row-start) | 12 | done | Absolutely positioned children of a positioned grid container take their grid area as containing block. |
| [`grid-template`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-template) | 12 | done | Shorthand |
| [`grid-template-areas`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-template-areas) | 12 | done |  |
| [`grid-template-columns`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-template-columns) | 12 | partial | Explicit tracks with names, `repeat()`, `minmax()`, `fit-content()` and `fr`, and `subgrid`. A subgrid takes the sizes of its parent's tracks, but its items do not add to them; the subgrid contributes as a whole. |
| [`grid-template-rows`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-template-rows) | 12 | partial | As `grid-template-columns`. Rows break across pages, with the items in them; a stretched item that needs more room on a later page grows past its row. |

## CSS Images

**PDF implementation:** Paint.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`image-orientation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/image-orientation) | 7 | done |  |
| [`image-rendering`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/image-rendering) | 7 | done |  |
| [`image-resolution`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/image-resolution) | 7 | done | Experimental. `from-image` reads PNG `pHYs` and JPEG JFIF densities. |
| [`object-fit`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/object-fit) | 7 | done |  |
| [`object-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/object-position) | 7 | done |  |
| `object-view-box` | 7 | done | Experimental. `inset()`, `xywh()` and `rect()`; `round` is accepted and changes nothing, since only the region matters. |

## CSS Inline

**PDF implementation:** Line layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`alignment-baseline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/alignment-baseline) | 4 | done |  |
| `baseline-shift` | 4 | done |  |
| [`baseline-source`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/baseline-source) | 4 | done |  |
| [`dominant-baseline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/dominant-baseline) | 4 | parsed | Baselines always align on the alphabetic baseline. |
| [`initial-letter`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/initial-letter) | 4 | done | Inline boxes and `::first-letter` boxes that start a paragraph. |
| `initial-letter-align` | 4 | parsed | Experimental. Initial letters always align on the alphabetic baseline. |
| [`line-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/line-height) | 4 | done |  |
| `text-box` | 4 | partial | Shorthand. Trims the block container's own first and last lines, not those of nested blocks. |
| `text-box-edge` | 4 | partial | Trims to the text, cap, ex and alphabetic edges; the ideographic edges use the text edges. |
| `text-box-trim` | 4 | partial | Trims the block container's own first and last lines, not those of nested blocks. |
| [`vertical-align`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/vertical-align) | 4 | done |  |

## CSS Lists and Counters

**PDF implementation:** Box tree generation.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`counter-increment`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/counter-increment) | 6 | done |  |
| [`counter-reset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/counter-reset) | 6 | done | `reversed()` without a value counts the increments in its scope. |
| [`counter-set`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/counter-set) | 6 | done |  |
| [`list-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/list-style) | 6 | done | Shorthand |
| [`list-style-image`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/list-style-image) | 6 | done | The marker is the image followed by a space; markers use `list-style-type` when it does not load. |
| [`list-style-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/list-style-position) | 6 | done |  |
| [`list-style-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/list-style-type) | 6 | done | Counter styles, `symbols()` and strings. |

## CSS Logical Properties and Values

**PDF implementation:** Layout. Resolved to physical properties from the writing mode.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`block-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/block-size) | 3 | done |  |
| [`border-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block) | 3 | done | Shorthand |
| [`border-block-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-color) | 3 | done | Shorthand |
| [`border-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-end) | 3 | done | Shorthand |
| [`border-block-end-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-end-color) | 3 | done |  |
| [`border-block-end-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-end-style) | 3 | done |  |
| [`border-block-end-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-end-width) | 3 | done |  |
| [`border-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-start) | 3 | done | Shorthand |
| [`border-block-start-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-start-color) | 3 | done |  |
| [`border-block-start-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-start-style) | 3 | done |  |
| [`border-block-start-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-start-width) | 3 | done |  |
| [`border-block-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-style) | 3 | done | Shorthand |
| [`border-block-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-block-width) | 3 | done | Shorthand |
| [`border-end-end-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-end-end-radius) | 3 | done |  |
| [`border-end-start-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-end-start-radius) | 3 | done |  |
| [`border-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline) | 3 | done | Shorthand |
| [`border-inline-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-color) | 3 | done | Shorthand |
| [`border-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-end) | 3 | done | Shorthand |
| [`border-inline-end-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-end-color) | 3 | done |  |
| [`border-inline-end-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-end-style) | 3 | done |  |
| [`border-inline-end-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-end-width) | 3 | done |  |
| [`border-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-start) | 3 | done | Shorthand |
| [`border-inline-start-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-start-color) | 3 | done |  |
| [`border-inline-start-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-start-style) | 3 | done |  |
| [`border-inline-start-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-start-width) | 3 | done |  |
| [`border-inline-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-style) | 3 | done | Shorthand |
| [`border-inline-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-inline-width) | 3 | done | Shorthand |
| [`border-start-end-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-start-end-radius) | 3 | done |  |
| [`border-start-start-radius`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-start-start-radius) | 3 | done |  |
| [`inline-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inline-size) | 3 | done |  |
| [`margin-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-block) | 3 | done | Shorthand |
| [`margin-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-block-end) | 3 | done |  |
| [`margin-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-block-start) | 3 | done |  |
| [`margin-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-inline) | 3 | done | Shorthand |
| [`margin-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-inline-end) | 3 | done |  |
| [`margin-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/margin-inline-start) | 3 | done |  |
| [`max-block-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/max-block-size) | 3 | done |  |
| [`max-inline-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/max-inline-size) | 3 | done |  |
| [`min-block-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/min-block-size) | 3 | done |  |
| [`min-inline-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/min-inline-size) | 3 | done |  |
| [`padding-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-block) | 3 | done | Shorthand |
| [`padding-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-block-end) | 3 | done |  |
| [`padding-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-block-start) | 3 | done |  |
| [`padding-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-inline) | 3 | done | Shorthand |
| [`padding-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-inline-end) | 3 | done |  |
| [`padding-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/padding-inline-start) | 3 | done |  |

## CSS Masking

**PDF implementation:** PDF clipping paths, and soft masks rasterised with `tiny-skia`.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`clip`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/clip) | 15 | done | Deprecated; applies to absolutely positioned boxes and clips their positioned descendants too. |
| [`clip-path`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/clip-path) | 15 | done | `margin-box` is the border box. References to SVG `<clipPath>` elements leave the element unclipped, as for any missing reference. |
| [`mask`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask) | 15 | done | Shorthand. Masks are rasterised into soft masks. It resets `mask-border`. |
| [`mask-border`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border) | 15 | done | Shorthand. The mask border and the mask layers both mask the element. |
| [`mask-border-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-mode) | 15 | done |  |
| [`mask-border-outset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-outset) | 15 | done |  |
| [`mask-border-repeat`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-repeat) | 15 | done |  |
| [`mask-border-slice`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-slice) | 15 | done |  |
| [`mask-border-source`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-source) | 15 | done |  |
| [`mask-border-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-border-width) | 15 | done |  |
| [`mask-clip`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-clip) | 15 | done | Boxes have no fill or stroke, so `fill-box` is the content box and `stroke-box` and `view-box` the border box. |
| [`mask-composite`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-composite) | 15 | done |  |
| [`mask-image`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-image) | 15 | done | A layer that does not load, or that refers to an SVG `<mask>`, is transparent black. |
| [`mask-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-mode) | 15 | done | `match-source` is `alpha`, as for any image. |
| [`mask-origin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-origin) | 15 | done | Boxes have no fill or stroke, so `fill-box` is the content box and `stroke-box` and `view-box` the border box. |
| [`mask-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-position) | 15 | done |  |
| [`mask-repeat`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-repeat) | 15 | done |  |
| [`mask-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-size) | 15 | done |  |
| [`mask-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mask-type) | 15 | done | No visual effect: it applies to SVG `<mask>` elements. |

## CSS Mobile Text Size Adjustment

**PDF implementation:** No visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`text-size-adjust`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-size-adjust) | 17 | done | Experimental; No visual effect (parse and compute only) |

## CSS Multi-column Layout

**PDF implementation:** Layout and fragmentation; columns balance across pages.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`column-count`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-count) | 13 | done | The column count, or the most columns that fit when `column-width` is set too (CSS Multi-column 1 § 3.4). Block containers and table cells become multi-column containers; each column is a fragmentainer that block, inline, table, flex and grid content breaks into, honoring `break-*`, `orphans` and `widows`. Columns continue on the next page. |
| [`column-fill`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-fill) | 13 | done | `balance` and `balance-all` give the columns the shortest height their content fits in; `auto` fills them one after another when the height is constrained, by the container or the page. Content before a spanner always balances, and on paged media only the last page's columns balance. |
| [`column-height`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-height) | 13 | done | Experimental. Rows of columns of that height, which wrap below each other separated by `row-gap` (1em for `normal`); the last row balances. |
| [`column-rule`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-rule) | 13 | done | Shorthand |
| [`column-rule-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-rule-color) | 13 | done |  |
| [`column-rule-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-rule-style) | 13 | done | Rules are drawn like borders, in the middle of each gap between two columns, as tall as their row. |
| [`column-rule-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-rule-width) | 13 | done | Computes to 0 when the style is `none` or `hidden`. |
| [`column-span`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-span) | 13 | partial | `all` makes a child of a multi-column container span its columns, which balance before it and continue below it. Deeper descendants do not span. |
| [`column-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-width) | 13 | done | The column width is at least this, as wide as the columns that fit make it. |
| [`column-wrap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/column-wrap) | 13 | done | Experimental. `wrap` starts a new row of columns below one that is full, and `nowrap` adds overflow columns beside the container; `auto` wraps when `column-height` is set. |
| [`columns`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/columns) | 13 | done | Shorthand, with `/ <'column-height'>` |

## CSS Overflow

**PDF implementation:** Clip paths. `scroll` and `auto` clip like `hidden`.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`line-clamp`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/line-clamp) | 4 | partial | Clamps the block container's own lines; lines inside nested blocks are not counted. |
| `max-lines` | 4 | partial | Experimental. Clamps the block container's own lines, without an ellipsis. |
| [`overflow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow) | 3 | done | Shorthand |
| [`overflow-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-block) | 3 | done |  |
| [`overflow-clip-margin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-clip-margin) | 3 | done |  |
| [`overflow-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-inline) | 3 | done |  |
| [`overflow-x`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-x) | 3 | done |  |
| [`overflow-y`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-y) | 3 | done |  |
| [`scroll-axis-lock`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-axis-lock) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-behavior`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-behavior) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-marker-group`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-marker-group) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-target-group`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-target-group) | 17 | done | No visual effect (parse and compute only) |
| [`scrollbar-gutter`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scrollbar-gutter) | 17 | done | No visual effect (parse and compute only) |
| [`text-overflow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-overflow) | 4 | partial | Ellipsis or string at the end side; a start-side value is ignored. |

## CSS Overscroll Behavior

**PDF implementation:** No visual effect: there is no scrolling.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`overscroll-behavior`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overscroll-behavior) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`overscroll-behavior-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overscroll-behavior-block) | 17 | done | No visual effect (parse and compute only) |
| [`overscroll-behavior-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overscroll-behavior-inline) | 17 | done | No visual effect (parse and compute only) |
| [`overscroll-behavior-x`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overscroll-behavior-x) | 17 | done | No visual effect (parse and compute only) |
| [`overscroll-behavior-y`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overscroll-behavior-y) | 17 | done | No visual effect (parse and compute only) |

## CSS Paged Media

**PDF implementation:** Pagination.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`page`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/page) | 5 | done |  |
| [`page-break-after`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/page-break-after) | 5 | done | Legacy alias of `break-after` |
| [`page-break-before`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/page-break-before) | 5 | done | Legacy alias of `break-before` |
| [`page-break-inside`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/page-break-inside) | 5 | done | Legacy alias of `break-inside` |

## CSS Positioned Layout

**PDF implementation:** Layout and stacking contexts. `fixed` boxes repeat on every page; `sticky` behaves as `relative`.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/bottom) | 10 | done | As `top`; `top` wins over `bottom`. |
| [`clear`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/clear) | 10 | done | `left`, `right`, `both` and the inline values move a block or float below earlier floats. The block-axis values are for page floats, so they clear nothing. |
| [`float`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/float) | 10 | done | `left`, `right`, `inline-start` and `inline-end`. Floats go beside earlier floats as high as they fit (CSS 2.1 § 9.5.1); a float in a line goes at the top of its line. Lines beside floats are shortened, boxes with their own formatting context are narrowed or moved below them, and formatting context roots grow to contain them. Floats do not break across pages: one that does not fit moves to the next page or column. They paint in tree order with the blocks around them. |
| [`inset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset) | 10 | done | Shorthand |
| [`inset-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-block) | 10 | done | Shorthand |
| [`inset-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-block-end) | 10 | done |  |
| [`inset-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-block-start) | 10 | done |  |
| [`inset-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-inline) | 10 | done | Shorthand |
| [`inset-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-inline-end) | 10 | done |  |
| [`inset-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/inset-inline-start) | 10 | done |  |
| [`left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/left) | 10 | done | As `top`. |
| [`overlay`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overlay) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position) | 10 | done | `relative` moves the box from where it is laid out. `absolute` boxes are placed in the padding box of their nearest positioned block ancestor, or the page area, from their static position when insets are `auto`; positioned inline boxes are not containing blocks, and absolute boxes do not break across pages. `fixed` boxes are placed in the page area and repeated on every page. `sticky` boxes never scroll, so they stay where they are. `running()` (CSS GCPM) moves elements into page-margin boxes for `element()`. |
| [`right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/right) | 10 | done | As `top`; `left` wins over `right` in left-to-right containing blocks. |
| [`top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/top) | 10 | done | Offsets relative boxes and places absolute and fixed boxes. Percentages of a height that depends on content are `auto`. |
| [`z-index`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/z-index) | 10 | done | Positioned boxes paint in layers of their stacking context by `z-index`, below its content when negative (CSS 2.1 Appendix E); an integer makes the box a stacking context. Positioned inline boxes paint with their line. |

## CSS Rhythmic Sizing

**PDF implementation:** Line layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`line-height-step`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/line-height-step) | 4 | done | Experimental |

## CSS Ruby

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`ruby-align`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/ruby-align) | 14 | done | The narrower of a segment's base and annotations spreads across the wider, by grapheme cluster. |
| `ruby-merge` | 14 | done | Experimental. `merge` sets all annotations of a level over all the bases of a ruby container; `auto` keeps them separate. |
| `ruby-overhang` | 14 | done | With `auto`, an annotation wider than its base hangs over text right next to the ruby container, into the room the spread base leaves at its end, by up to the annotation's font size. |
| [`ruby-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/ruby-position) | 14 | done | `alternate` puts successive annotations of a base on alternating sides. `inter-character` sets annotations vertically after their base in horizontal text, and is `over` in vertical text. |

## CSS Scroll Anchoring

**PDF implementation:** No visual effect: there is no scrolling.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`overflow-anchor`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-anchor) | 17 | done | No visual effect (parse and compute only) |

## CSS Scroll Snap

**PDF implementation:** No visual effect: there is no scrolling.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `scroll-initial-target` | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`scroll-margin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-margin-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-block) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-margin-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-block-end) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-block-start) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-bottom) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-inline) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-margin-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-inline-end) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-inline-start) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-left) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-right) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-margin-top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-margin-top) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-padding-block`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-block) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-padding-block-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-block-end) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-block-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-block-start) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-bottom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-bottom) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-inline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-inline) | 17 | done | Shorthand; No visual effect (parse and compute only) |
| [`scroll-padding-inline-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-inline-end) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-inline-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-inline-start) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-left) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-right) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-padding-top`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-padding-top) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-snap-align`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-snap-align) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-snap-stop`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-snap-stop) | 17 | done | No visual effect (parse and compute only) |
| [`scroll-snap-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-snap-type) | 17 | done | No visual effect (parse and compute only) |

## CSS Scrollbars Styling

**PDF implementation:** No visual effect: there is no scrolling.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`scrollbar-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scrollbar-color) | 17 | done | No visual effect (parse and compute only) |
| [`scrollbar-width`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scrollbar-width) | 17 | done | No visual effect (parse and compute only) |

## CSS Shapes

**PDF implementation:** Layout of floats around shapes.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`shape-image-threshold`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/shape-image-threshold) | 10 | done | Pixels more opaque than the threshold, clamped to 0 to 1, are in the shape. |
| [`shape-margin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/shape-margin) | 10 | done | Widens the shape all around. |
| [`shape-outside`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/shape-outside) | 10 | done | Basic shapes in their reference box (`margin-box` unless given), a box alone with its rounded corners, or an image whose alpha channel is the shape, filling the content box. Lines beside the float wrap around the shape, within the float's margin box; floats are still placed by their margin boxes. |

## CSS Speech

**PDF implementation:** No visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `speak-as` | 17 | done | Experimental; No visual effect (parse and compute only) |

## CSS Table

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`border-collapse`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-collapse) | 8 | done | Collapsed borders resolve by width, style, then cell, row, row group, column, column group and table (CSS 2.1 § 17.6.2.1). Each cell draws half of each of its borders, so both halves of a shared border meet on the grid line. |
| [`border-spacing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/border-spacing) | 8 | done |  |
| [`caption-side`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/caption-side) | 8 | done | `top` and `bottom`. |
| [`empty-cells`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/empty-cells) | 8 | done |  |
| [`table-layout`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/table-layout) | 8 | done | `fixed` applies to tables with a width: the columns and the first row decide the column widths. |

## CSS Text

**PDF implementation:** Line layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`hanging-punctuation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/hanging-punctuation) | 4 | done |  |
| [`hyphenate-character`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/hyphenate-character) | 4 | done |  |
| [`hyphenate-limit-chars`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/hyphenate-limit-chars) | 4 | done |  |
| [`hyphens`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/hyphens) | 4 | done |  |
| [`letter-spacing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/letter-spacing) | 4 | done |  |
| [`line-break`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/line-break) | 4 | partial | `anywhere` breaks between any characters; `loose`, `normal` and `strict` follow the default rules. |
| [`overflow-wrap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-wrap) | 4 | done |  |
| [`tab-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/tab-size) | 4 | done |  |
| [`text-align`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-align) | 4 | done |  |
| [`text-align-last`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-align-last) | 4 | done |  |
| [`text-autospace`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-autospace) | 4 | done | Adds ⅛ em between ideographs and Latin letters or digits; the bundled fonts have no ideographs. |
| [`text-fit`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-fit) | 4 | parsed | Text is not scaled yet. |
| [`text-indent`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-indent) | 4 | done |  |
| [`text-justify`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-justify) | 4 | done |  |
| [`text-spacing-trim`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-spacing-trim) | 4 | parsed | Experimental. Needs CJK fonts with the `halt` and `chws` features; none are bundled. |
| [`text-transform`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-transform) | 4 | done |  |
| [`text-wrap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-wrap) | 4 | done | Shorthand |
| [`text-wrap-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-wrap-mode) | 4 | done |  |
| [`text-wrap-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-wrap-style) | 4 | done |  |
| [`white-space`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/white-space) | 4 | done |  |
| [`white-space-collapse`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/white-space-collapse) | 4 | done |  |
| [`word-break`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/word-break) | 4 | done |  |
| [`word-spacing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/word-spacing) | 4 | done |  |
| [`word-wrap`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/overflow-wrap) | 4 | done |  |

## CSS Text Decoration

**PDF implementation:** Paint.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`text-decoration`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration) | 4 | done | Shorthand |
| [`text-decoration-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-color) | 4 | done |  |
| [`text-decoration-inset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-inset) | 4 | parsed | Decorations are not inset yet. |
| [`text-decoration-line`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-line) | 4 | done |  |
| [`text-decoration-skip`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-skip) | 4 | partial | Experimental. `spaces` leaves out spaces; `edges` and `box-decoration` have no effect. |
| [`text-decoration-skip-ink`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-skip-ink) | 4 | done |  |
| [`text-decoration-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-style) | 4 | done |  |
| [`text-decoration-thickness`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-decoration-thickness) | 4 | done |  |
| [`text-emphasis`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-emphasis) | 4 | done | Shorthand |
| [`text-emphasis-color`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-emphasis-color) | 4 | done |  |
| [`text-emphasis-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-emphasis-position) | 4 | done | `left` and `right` place marks beside vertical text; `over` and `under` beside horizontal text. |
| [`text-emphasis-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-emphasis-style) | 4 | done |  |
| [`text-shadow`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-shadow) | 4 | done | Blurred shadows are rasterised. |
| [`text-underline-offset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-underline-offset) | 4 | done |  |
| [`text-underline-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-underline-position) | 4 | done | In vertical text, `left` and `right` put the underline on that side of the line. |

## CSS Transforms

**PDF implementation:** PDF transformation matrices. 3D transforms are flattened to their 2D projection.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`backface-visibility`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/backface-visibility) | 15 | done | Decided from the box's accumulated 3D transformation. |
| [`perspective`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/perspective) | 15 | done | A perspective projection is not affine, so a box is drawn with the affine map that best fits its projected corners. |
| [`perspective-origin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/perspective-origin) | 15 | done |  |
| [`rotate`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/rotate) | 15 | done |  |
| [`scale`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scale) | 15 | done |  |
| [`transform`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transform) | 15 | done | Not for inline boxes. Link areas inside a transformed box stay where the box is laid out. |
| [`transform-box`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transform-box) | 15 | done | Boxes have no fill or stroke, so `fill-box` is the content box and `stroke-box` and `view-box` the border box. |
| [`transform-origin`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transform-origin) | 15 | done |  |
| [`transform-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transform-style) | 15 | done | `preserve-3d` composes the 3D transformations of the children with their parent's; they paint in tree order, not by depth. |
| [`translate`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/translate) | 15 | done |  |

## CSS Transitions

**PDF implementation:** No transitions run; values render as specified.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`transition`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition) | 17 | done | Shorthand; Never runs: transitions start when styles change, which a snapshot never sees. |
| [`transition-behavior`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition-behavior) | 17 | done | Never runs: transitions start when styles change. |
| [`transition-delay`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition-delay) | 17 | done | Never runs: transitions start when styles change. |
| [`transition-duration`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition-duration) | 17 | done | Never runs: transitions start when styles change. |
| [`transition-property`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition-property) | 17 | done | Never runs: transitions start when styles change. |
| [`transition-timing-function`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/transition-timing-function) | 17 | done | Never runs: transitions start when styles change. |

## CSS Values and Units

**PDF implementation:** Cascade.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`interpolate-size`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interpolate-size) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`link-parameters`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/link-parameters) | 7 | parsed | Experimental. The parameters are for SVG images, and only raster images are supported. |

## CSS View Transitions

**PDF implementation:** No visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| `view-transition-class` | 17 | done | No visual effect (parse and compute only) |
| [`view-transition-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-transition-name) | 17 | done | No visual effect (parse and compute only) |
| [`view-transition-scope`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-transition-scope) | 17 | done | No visual effect (parse and compute only) |

## CSS Viewport

**PDF implementation:** Layout.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`zoom`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/zoom) | 3 | partial | Scales lengths, font sizes and line heights, and computed values report them zoomed. Other inherited lengths, such as `letter-spacing`, keep their parent's zoom. |

## CSS Will Change

**PDF implementation:** No visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`will-change`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/will-change) | 17 | done | No visual effect (parse and compute only) |

## CSS Writing Modes

**PDF implementation:** Layout, including vertical text.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`direction`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/direction) | 4 | done |  |
| [`text-combine-upright`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-combine-upright) | 14 | done | `all`, and `digits` for runs of up to that many ASCII digits, set the text upright in the space of one em, squeezed horizontally to fit. |
| [`text-orientation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-orientation) | 14 | done | `mixed` sets characters upright by their Unicode vertical orientation (UAX #50) and the rest sideways. Upright glyphs take the font's vertical forms and advances, or one em. Sideways writing modes set everything sideways. |
| [`unicode-bidi`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/unicode-bidi) | 4 | done |  |
| [`writing-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/writing-mode) | 14 | done | Every layout mode lays out vertically. The `<body>`'s writing mode is the document's, and pages progress in its block flow direction. A box in another writing mode than its parent establishes an independent formatting context, does not break across pages, and contains its absolutely positioned and fixed descendants. Tables keep the writing mode of the box around them. Background images of inline boxes turn with vertical text. |

## Compositing and Blending

**PDF implementation:** PDF transparency groups and blend modes.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`background-blend-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/background-blend-mode) | 15 | done | The background is an isolated group. |
| [`isolation`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/isolation) | 15 | done |  |
| [`mix-blend-mode`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/mix-blend-mode) | 15 | partial | PDF has no additive blend modes, so `plus-lighter` is drawn as `screen` and `plus-darker` as `multiply`. A stacking context that a descendant blends in is an isolated group. |

## Filter Effects

**PDF implementation:** Color filters change the colors of what is painted, so text and shapes stay vectors. Blurs are rasterised with `tiny-skia` and embedded as images.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`backdrop-filter`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/backdrop-filter) | 15 | done | The backdrop is what is painted below the element since the start of its innermost group, on the same page. |
| [`filter`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/filter) | 15 | done | References to SVG `<filter>` elements leave the element unfiltered, as for any missing reference. |

## Motion Path

**PDF implementation:** PDF transformation matrices.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`offset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset) | 15 | done | Shorthand |
| [`offset-anchor`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-anchor) | 15 | done | `auto` uses `transform-origin`. |
| [`offset-distance`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-distance) | 15 | done |  |
| [`offset-path`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-path) | 15 | done | The containing block is the parent's border box, and `ray()` ignores `contain`. References to SVG shapes are ignored. |
| [`offset-position`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-position) | 15 | done |  |
| [`offset-rotate`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-rotate) | 15 | done |  |

## Pointer Events

**PDF implementation:** No visual effect.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`touch-action`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/touch-action) | 17 | done | No visual effect (parse and compute only) |

## Scalable Vector Graphics

**PDF implementation:** Text painting (stroke, paint order, rendering hints).

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`paint-order`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/paint-order) | 4 | done |  |
| [`stroke`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/stroke) | 4 | done | Colors stroke text 1px wide (`stroke-width` is out of scope); paint servers use their fallback color. |
| `stroke-color` | 4 | done | Experimental |
| [`text-rendering`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/text-rendering) | 4 | done | `optimizeSpeed` turns off kerning and ligatures; the other values have no visible effect. |

## Scroll-driven Animations

**PDF implementation:** No visual effect: there is no scrolling.

| Property | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`animation-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-range) | 17 | done | Shorthand; Experimental; No visual effect (parse and compute only) |
| [`animation-range-end`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-range-end) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`animation-range-start`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-range-start) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`scroll-timeline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-timeline) | 17 | done | Shorthand; Experimental; No visual effect (parse and compute only) |
| [`scroll-timeline-axis`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-timeline-axis) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`scroll-timeline-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/scroll-timeline-name) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`timeline-scope`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/timeline-scope) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`view-timeline`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-timeline) | 17 | done | Shorthand; Experimental; No visual effect (parse and compute only) |
| [`view-timeline-axis`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-timeline-axis) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`view-timeline-inset`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-timeline-inset) | 17 | done | Experimental; No visual effect (parse and compute only) |
| [`view-timeline-name`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/view-timeline-name) | 17 | done | Experimental; No visual effect (parse and compute only) |
