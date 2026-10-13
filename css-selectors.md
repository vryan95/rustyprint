# CSS selector tracker

Implementation status of every CSS selector in scope for rustyprint. See [plan.md](plan.md) for the plan and milestones.

**Source:** [MDN CSS reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference), using the machine-readable [`mdn/data`](https://github.com/mdn/data) package, version 2.37.2. `:heading` and `:heading()` come from the MDN reference page, because that `mdn/data` version does not include them yet.

**Scope:** 121 standard and experimental selectors, combinators, pseudo-classes and pseudo-elements.

The PDF is a static snapshot with no user interaction. Selectors that depend on interaction, playback or browser UI are parsed but never match. They are `done` once they parse correctly.

## Status values

| Status | Meaning |
| --- | --- |
| `todo` | Not started |
| `partial` | Some forms supported (list the gaps in Notes) |
| `done` | Parsed and matched as described, with fixtures |

## Basic selectors

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [Type selectors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Type_selectors) | 2 | done |  |
| [Class selectors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Class_selectors) | 2 | done |  |
| [ID selectors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/ID_selectors) | 2 | done |  |
| [Universal selectors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Universal_selectors) | 2 | done |  |
| [Attribute selectors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Attribute_selectors) | 2 | done | All operators and the `i`/`s` flags. |
| [Selector list](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Selector_list) | 2 | done |  |
| [Namespace separator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Namespace_separator) | 2 | done | Used with `@namespace`. |
| [Nesting selector](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Nesting_selector) | 2 | done | `&` |

## Combinators

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [Descendant combinator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Descendant_combinator) | 2 | done |  |
| [Child combinator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Child_combinator) | 2 | done | `>` |
| [Next-sibling combinator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Next-sibling_combinator) | 2 | done | `+` |
| [Subsequent-sibling combinator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Subsequent-sibling_combinator) | 2 | done | `~` |
| [Column combinator](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/Column_combinator) | 8 | done | `||` — matches a table cell in the columns of a `col`, or of a `colgroup` without `col` children, by the HTML table model. |

## Pseudo-classes: tree-structural

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:root`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:root) | 2 | done |  |
| [`:empty`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:empty) | 2 | done |  |
| [`:first-child`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:first-child) | 2 | done |  |
| [`:last-child`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:last-child) | 2 | done |  |
| [`:only-child`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:only-child) | 2 | done |  |
| [`:first-of-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:first-of-type) | 2 | done |  |
| [`:last-of-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:last-of-type) | 2 | done |  |
| [`:only-of-type`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:only-of-type) | 2 | done |  |
| [`:nth-child()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:nth-child) | 2 | done | Including `of S`. |
| [`:nth-last-child()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:nth-last-child) | 2 | done | Including `of S`. |
| [`:nth-of-type()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:nth-of-type) | 2 | done |  |
| [`:nth-last-of-type()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:nth-last-of-type) | 2 | done |  |

## Pseudo-classes: logical and element

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:is()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:is) | 2 | done |  |
| [`:where()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:where) | 2 | done |  |
| [`:not()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:not) | 2 | done |  |
| [`:has()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:has) | 2 | done |  |
| [`:scope`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:scope) | 2 | done |  |
| [`:defined`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:defined) | 2 | done | Matches every element except autonomous custom elements (names with a hyphen), which are never defined without scripting. |
| [`:heading`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:heading) | 2 | done | Experimental. Matches `h1`–`h6`. |
| [`:heading()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:heading_function) | 2 | done | Experimental. Matches headings of the given levels. |
| [`:state()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:state) | 2 | done | Never matches: there are no custom elements. |

## Pseudo-classes: linguistic

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:lang()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:lang) | 2 | done | From `lang` attributes. |
| [`:dir()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:dir) | 2 | done | From `dir` attributes and directionality. |

## Pseudo-classes: location

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:any-link`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:any-link) | 2 | done | Matches `a`, `area` and `link` with `href`. |
| [`:link`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:link) | 2 | done | Matches every link. |
| [`:local-link`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:local-link) | 2 | done | Experimental. Matches links to a fragment of the document. |
| [`:visited`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:visited) | 2 | done | Never matches: no browsing history. |
| [`:target`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:target) | 2 | done | Never matches: no fragment is targeted. |
| [`:target-within`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:target-within) | 2 | done | Experimental. Never matches: no fragment is targeted. |
| `:target-current` | 2 | done | Experimental. Never matches: no scroll markers. |

## Pseudo-classes: user action

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:hover`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:hover) | 2 | done | Never matches. |
| [`:active`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:active) | 2 | done | Never matches. |
| [`:focus`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:focus) | 2 | done | Never matches. |
| [`:focus-visible`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:focus-visible) | 2 | done | Never matches. |
| [`:focus-within`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:focus-within) | 2 | done | Never matches. |

## Pseudo-classes: input

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:enabled`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:enabled) | 9 | done | From the `disabled` attribute and `fieldset` ancestry. |
| [`:disabled`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:disabled) | 9 | done | As `:enabled`. |
| [`:read-only`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:read-only) | 9 | done | From `readonly`, `disabled` and the element type. |
| [`:read-write`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:read-write) | 9 | done | As `:read-only`. |
| [`:placeholder-shown`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:placeholder-shown) | 9 | done | When `placeholder` is set and the value is empty. |
| [`:default`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:default) | 9 | done | Checkboxes and radio buttons with `checked`, options with `selected`, and the first submit button of each form. |
| [`:checked`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:checked) | 9 | done | Checked checkboxes and radio buttons (the last `checked` one of a radio group) and selected options, including the option a drop-down selects by default. |
| [`:indeterminate`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:indeterminate) | 9 | done | Radio buttons whose group has no checked button, and `progress` without a valid `value`. Checkboxes are only indeterminate from script. |
| [`:valid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:valid) | 9 | partial | From constraint validation of the initial value: required values, email and URL syntax, `min`, `max` and `step`; forms and fieldsets match when all their controls do. `pattern` is not checked, and length limits only apply to edited values. |
| [`:invalid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:invalid) | 9 | partial | As `:valid`. |
| [`:in-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:in-range) | 9 | done | Number, range, date and time inputs with `min` or `max`, whose value is within them. |
| [`:out-of-range`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:out-of-range) | 9 | done | As `:in-range`, for values outside the limits. |
| [`:required`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:required) | 9 | done | From `required`. |
| [`:optional`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:optional) | 9 | done | From `required`. |
| [`:user-valid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:user-valid) | 2 | done | Never matches: no user interaction. |
| [`:user-invalid`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:user-invalid) | 2 | done | Never matches: no user interaction. |
| [`:autofill`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:autofill) | 2 | done | Never matches. |

## Pseudo-classes: element display state

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:open`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:open) | 9 | done | Matches `details[open]`. |
| [`:fullscreen`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:fullscreen) | 2 | done | Never matches. |
| [`:modal`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:modal) | 2 | done | Never matches. |
| [`:picture-in-picture`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:picture-in-picture) | 2 | done | Never matches. |
| [`:popover-open`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:popover-open) | 2 | done | Never matches. |

## Pseudo-classes: resource state

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:paused`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:paused) | 7 | done | Matches `audio` and `video`; media never plays. |
| [`:muted`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:muted) | 7 | done | From the `muted` attribute. |
| [`:playing`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:playing) | 2 | done | Never matches. |
| [`:seeking`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:seeking) | 2 | done | Never matches. |
| [`:buffering`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:buffering) | 2 | done | Never matches. |
| [`:stalled`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:stalled) | 2 | done | Never matches. |
| [`:volume-locked`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:volume-locked) | 2 | done | Never matches. |

## Pseudo-classes: time-dimensional

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:current`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:current) | 2 | done | Experimental. Never matches: no timed text is playing. |
| [`:past`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:past) | 2 | done | Never matches. |
| [`:future`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:future) | 2 | done | Never matches. |

## Pseudo-classes: paged media

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:first`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:first) | 5 | done | In `@page`: the first page. |
| [`:left`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:left) | 5 | done | In `@page`: left pages. |
| [`:right`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:right) | 5 | done | In `@page`: right pages. |
| [`:blank`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:blank) | 5 | done | Experimental. In `@page`: pages left blank by forced breaks. |

## Pseudo-classes: shadow DOM and other

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`:host`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:host) | 2 | done | Never matches: there are no shadow trees. |
| [`:host()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:host_function) | 2 | done | Never matches: there are no shadow trees. |
| [`:host-context()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:host-context) | 2 | done | Never matches: there are no shadow trees. |
| [`:has-slotted`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:has-slotted) | 2 | done | Never matches: there are no shadow trees. |
| `:active-view-transition` | 2 | done | Never matches. |
| `:active-view-transition-type()` | 2 | done | Never matches. |
| `:xr-overlay` | 2 | done | Experimental. Never matches. |

## Pseudo-elements: generated content and text

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`::before`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::before) | 6 | done | Inline, block and inline-block boxes from `content`. |
| [`::after`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::after) | 6 | done | Inline, block and inline-block boxes from `content`. |
| [`::marker`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::marker) | 6 | done | Outside markers sit beside the first line box; inside markers start it. |
| [`::first-letter`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::first-letter) | 6 | done | The first letter and the punctuation around it, from text or `::before`; an inside marker is skipped. `float` on it comes with milestone 10. |
| [`::first-line`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::first-line) | 6 | done | The first line breaks in its styles, and the rest continue in their own. Inherited properties apply to the content of the line; backgrounds and decorations paint behind it. |

## Pseudo-elements: forms and interactive

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`::placeholder`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::placeholder) | 9 | done | The placeholder text of empty `input` and `textarea` fields. |
| [`::file-selector-button`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::file-selector-button) | 9 | done | The button of `input type=file`. |
| [`::checkmark`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::checkmark) | 9 | done | Shown in checked checkboxes and radio buttons with `appearance: base`: its `content`, or else a drawn check mark or dot. The options of a `base-select` picker are never shown, since the picker is never open. |
| [`::picker-icon`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::picker-icon) | 9 | done | The drop-down arrow of `select`: its `content`, or else a drawn chevron. Not shown with `appearance: none`. |
| [`::details-content`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::details-content) | 9 | done | The box around the content of an open `details`. |
| [`::picker()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::picker) | 2 | done | Never matches: pickers are never open. |

## Pseudo-elements: never generated

| Selector | Milestone | Status | Notes |
| --- | --- | --- | --- |
| [`::selection`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::selection) | 2 | done | No selection. |
| [`::target-text`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::target-text) | 2 | done | No text fragment. |
| [`::highlight()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::highlight) | 2 | done | No Highlight API. |
| [`::spelling-error`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::spelling-error) | 2 | done | No spell checking. |
| [`::grammar-error`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::grammar-error) | 2 | done | No grammar checking. |
| [`::backdrop`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::backdrop) | 2 | done | No top layer. |
| [`::cue`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::cue) | 2 | done | No captions are shown. |
| [`::cue()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::cue) | 2 | done | No captions are shown. |
| `::cue-region` | 2 | done | Experimental. No captions are shown. |
| `::cue-region()` | 2 | done | Experimental. No captions are shown. |
| [`::part()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::part) | 2 | done | No shadow trees. |
| [`::slotted()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::slotted) | 2 | done | No shadow trees. |
| `::scroll-marker` | 2 | done | Experimental. No scroll containers. |
| `::scroll-marker-group` | 2 | done | Experimental. No scroll containers. |
| [`::view-transition`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition) | 2 | done | No view transitions. |
| [`::view-transition-group()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition-group) | 2 | done | No view transitions. |
| [`::view-transition-image-pair()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition-image-pair) | 2 | done | No view transitions. |
| [`::view-transition-new()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition-new) | 2 | done | No view transitions. |
| [`::view-transition-old()`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition-old) | 2 | done | No view transitions. |
