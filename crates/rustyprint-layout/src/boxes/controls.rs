//! Form controls (HTML § 15.5, CSS Forms 1), drawn in the state the parser leaves them in.
//!
//! Text fields, buttons and drop-downs become inline-blocks with an anonymous inner box that
//! shows their value, placeholder or label. Checkboxes, radio buttons, sliders, color wells,
//! progress bars and meters become replaced boxes that paint a [`Widget`]. There are no
//! native widgets, so `appearance: auto` draws what current browsers draw by default, and
//! `appearance: none` leaves only what CSS draws.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignmentBaseline, Appearance, BoxSizing, Content, FieldSizing, LengthPercentage, Overflow,
    Size, TextWrapMode, WhiteSpaceCollapse,
};
use rustyprint_css::selector::PseudoElement;
use rustyprint_dom::forms::{self, InputType};
use rustyprint_dom::{Element, NodeId, local_name};

use super::{BlockBox, BlockContents, Builder, InlineItem, MeterLevel, Replaced, Widget};

/// What a form control renders as.
pub(super) enum Control {
    Replaced(Replaced),
    Block(BlockBox),
}

/// A zero-width space, which gives an empty field a line and so a height and a baseline.
const EMPTY_LINE: &str = "\u{200B}";

/// What a `<keygen>` shows: its key size.
const KEYGEN_LABEL: &str = "2048 (High Grade)";

/// The space between a drop-down's label and its arrow, and around them.
const DROP_DOWN_GAP: f32 = 4.0;

/// The size of checkboxes and radio buttons, and of range sliders, in CSS pixels.
const CHECKBOX_SIZE: (f32, f32) = (13.0, 13.0);
const RANGE_SIZE: (f32, f32) = (129.0, 16.0);
/// The content box of a color well, inside its 1px border and 1px 2px padding.
const COLOR_SIZE: (f32, f32) = (44.0, 23.0);

fn block(node: Option<NodeId>, style: Arc<ComputedValues>, contents: BlockContents) -> BlockBox {
    BlockBox {
        node,
        style,
        contents,
        page: None,
        marker: None,
        first_line: Vec::new(),
        legend: None,
        center_contents: false,
        shape_image: None,
        replaced: None,
    }
}

/// A replaced box that paints `widget`, with a natural size.
fn widget(widget: Widget, size: (f32, f32)) -> Control {
    Control::Replaced(Replaced {
        picture: None,
        size: Some(size),
        widget: Some(widget),
    })
}

/// An atomic inline that paints `widget` at `size`, styled by `style`.
fn widget_item(widget: Widget, size: (f32, f32), style: Arc<ComputedValues>) -> InlineItem {
    let mut item = block(None, style, BlockContents::Blocks(Vec::new()));
    item.replaced = Some(Replaced {
        picture: None,
        size: Some(size),
        widget: Some(widget),
    });
    InlineItem::Atomic(Box::new(item))
}

/// The style of a control's inner box: it inherits from the control, keeps white space,
/// does not wrap unless `wrap`, and clips what overflows.
fn inner_style(style: &ComputedValues, wrap: bool) -> ComputedValues {
    let mut inner = ComputedValues::inherit_from(style);
    if !wrap {
        inner.white_space_collapse = WhiteSpaceCollapse::Preserve;
        inner.text_wrap_mode = TextWrapMode::Nowrap;
    }
    inner.overflow_x = Overflow::Clip;
    inner.overflow_y = Overflow::Clip;
    inner
}

/// A control's style with a default content-box size for a `width` or `height` that is
/// `auto`, unless `field-sizing: content` sizes it to its contents instead.
fn sized(
    style: &Arc<ComputedValues>,
    width: Option<f32>,
    height: Option<f32>,
) -> Arc<ComputedValues> {
    let fixed = style.field_sizing == FieldSizing::Fixed;
    let width = width.filter(|_| fixed && matches!(style.width, Size::Auto));
    let height = height.filter(|_| fixed && matches!(style.height, Size::Auto));
    if width.is_none() && height.is_none() {
        return style.clone();
    }
    let (padding, border) = crate::flow::padding_and_border(style, 0.0);
    let border_box = style.box_sizing == BoxSizing::BorderBox;
    let mut sized = (**style).clone();
    if let Some(width) = width {
        let extra = padding.horizontal() + border.horizontal();
        let width = if border_box { width + extra } else { width };
        sized.width = Size::Value(LengthPercentage::Length(width));
    }
    if let Some(height) = height {
        let extra = padding.vertical() + border.vertical();
        let height = if border_box { height + extra } else { height };
        sized.height = Size::Value(LengthPercentage::Length(height));
    }
    Arc::new(sized)
}

/// The average character width that sizes text fields: the advance of `0`.
fn char_width(style: &ComputedValues) -> f32 {
    crate::font_metrics(style).zero_advance
}

/// A positive integer attribute such as `size`, `cols` or `rows`, or `default`.
fn positive(element: &Element, name: &str, default: u32) -> u32 {
    element
        .attr(name)
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(default)
}

/// A number attribute, parsed leniently as `<progress>` and `<meter>` do.
fn number(element: &Element, name: &str) -> Option<f64> {
    element
        .attr(name)
        .and_then(|v| forms::parse_float(v.trim()))
}

/// What a text field shows for a date or time input without a value: the format to type.
fn date_format(ty: InputType) -> Option<&'static str> {
    Some(match ty {
        InputType::Date => "yyyy-mm-dd",
        InputType::Month => "yyyy-mm",
        InputType::Week => "yyyy-W--",
        InputType::Time => "--:--",
        InputType::DatetimeLocal => "yyyy-mm-dd --:--",
        _ => return None,
    })
}

/// The `<progress>` bar: filled to `value` out of `max`, or indeterminate without a value
/// (HTML § 4.10.13).
fn progress(element: &Element, style: &ComputedValues) -> Replaced {
    let max = number(element, "max").filter(|m| *m > 0.0).unwrap_or(1.0);
    let position = number(element, "value").map(|v| (v.clamp(0.0, max) / max) as f32);
    let em = style.font_size;
    Replaced {
        picture: None,
        size: Some((10.0 * em, em)),
        widget: Some(Widget::Progress { position }),
    }
}

/// The `<meter>` gauge: its value within `min` and `max`, and which region of the range
/// `low`, `high` and `optimum` put it in (HTML § 4.10.14).
fn meter(element: &Element, style: &ComputedValues) -> Replaced {
    let min = number(element, "min").unwrap_or(0.0);
    let max = number(element, "max").unwrap_or(1.0).max(min);
    let value = number(element, "value").unwrap_or(0.0).clamp(min, max);
    let low = number(element, "low").unwrap_or(min).clamp(min, max);
    let high = number(element, "high").unwrap_or(max).clamp(low, max);
    let optimum = number(element, "optimum")
        .unwrap_or((min + max) / 2.0)
        .clamp(min, max);
    let level = if optimum < low {
        if value < low {
            MeterLevel::Optimum
        } else if value <= high {
            MeterLevel::Suboptimum
        } else {
            MeterLevel::EvenLessGood
        }
    } else if optimum > high {
        if value > high {
            MeterLevel::Optimum
        } else if value >= low {
            MeterLevel::Suboptimum
        } else {
            MeterLevel::EvenLessGood
        }
    } else if value >= low && value <= high {
        MeterLevel::Optimum
    } else {
        MeterLevel::Suboptimum
    };
    let fraction = if max > min {
        ((value - min) / (max - min)) as f32
    } else {
        0.0
    };
    let em = style.font_size;
    Replaced {
        picture: None,
        size: Some((5.0 * em, em)),
        widget: Some(Widget::Meter { fraction, level }),
    }
}

/// The color of an `<input type=color>`'s value, which is always `#rrggbb`.
fn hex_color(value: &str) -> [u8; 3] {
    let channel = |i: usize| {
        value
            .get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(0)
    };
    [channel(1), channel(3), channel(5)]
}

impl Builder<'_> {
    /// The box of a form control, or `None` if the element is not one or renders like
    /// other elements, as `<button>` does.
    pub(super) fn control(
        &mut self,
        node: NodeId,
        element: &Element,
        style: &Arc<ComputedValues>,
    ) -> Option<Control> {
        Some(match element.name.local {
            local_name!("input") => return self.input(node, element, style),
            local_name!("textarea") => Control::Block(self.textarea(node, element, style)),
            local_name!("select") if forms::is_list_box(element) => {
                Control::Block(self.list_box(node, element, style))
            }
            local_name!("select") => {
                let document = self.document;
                let labels: Vec<String> = forms::options(document, node)
                    .into_iter()
                    .map(|option| forms::option_label(document, option))
                    .collect();
                let label = forms::selected_options(document, node)
                    .first()
                    .map(|&option| forms::option_label(document, option))
                    .unwrap_or_default();
                Control::Block(self.drop_down(node, style, label, &labels))
            }
            local_name!("keygen") => {
                let label = KEYGEN_LABEL.to_owned();
                Control::Block(self.drop_down(node, style, label.clone(), &[label]))
            }
            local_name!("progress") => Control::Replaced(progress(element, style)),
            local_name!("meter") => Control::Replaced(meter(element, style)),
            _ => return None,
        })
    }

    fn input(
        &mut self,
        node: NodeId,
        element: &Element,
        style: &Arc<ComputedValues>,
    ) -> Option<Control> {
        let document = self.document;
        let ty = InputType::of(element);
        let native = style.appearance.is_native();
        let disabled = forms::is_disabled(document, node);
        Some(match ty {
            // Hidden inputs have no box; image buttons are embedded images.
            InputType::Hidden | InputType::Image => return None,
            InputType::Checkbox | InputType::Radio => {
                let checked = forms::is_checked(document, node);
                let radio = ty == InputType::Radio;
                if style.appearance == Appearance::Base {
                    Control::Block(self.checkmark_box(node, style, checked, radio))
                } else if !native {
                    Control::Block(self.empty(node, style))
                } else if radio {
                    widget(Widget::Radio { checked, disabled }, CHECKBOX_SIZE)
                } else {
                    widget(Widget::Checkbox { checked, disabled }, CHECKBOX_SIZE)
                }
            }
            InputType::Range if native => {
                let (value, min, max) = forms::range_value_of(document, node)?;
                let fraction = if max > min {
                    ((value - min) / (max - min)) as f32
                } else {
                    0.0
                };
                widget(Widget::Range { fraction, disabled }, RANGE_SIZE)
            }
            InputType::Range => Control::Block(self.empty(node, style)),
            InputType::Color => {
                let value = forms::input_value(document, node);
                widget(Widget::Color(hex_color(&value)), COLOR_SIZE)
            }
            InputType::File => Control::Block(self.file_input(node, element, style)),
            InputType::Submit | InputType::Reset | InputType::Button => {
                let label = match element.attr("value") {
                    Some(value) => value.to_owned(),
                    None => match ty {
                        InputType::Submit => "Submit".to_owned(),
                        InputType::Reset => "Reset".to_owned(),
                        _ => String::new(),
                    },
                };
                let inner = Arc::new(inner_style(style, false));
                let items = self.text(node, &label, &inner);
                Control::Block(self.field(node, style.clone(), inner, items))
            }
            _ => {
                let value = forms::input_value(document, node);
                let shown = match ty {
                    InputType::Password => "\u{2022}".repeat(value.chars().count()),
                    InputType::DatetimeLocal => value.replacen('T', " ", 1),
                    _ => value,
                };
                let inner = Arc::new(inner_style(style, false));
                let items = if !shown.is_empty() {
                    self.text(node, &shown, &inner)
                } else if let Some(format) = date_format(ty) {
                    self.text(node, format, &inner)
                } else {
                    let placeholder: String = element
                        .attr("placeholder")
                        .unwrap_or("")
                        .chars()
                        .filter(|c| !matches!(c, '\n' | '\r'))
                        .collect();
                    self.placeholder(node, &placeholder, &inner)
                };
                // Fields that take text are `size` characters wide; date and time fields fit
                // what they show.
                let width = match ty {
                    InputType::Text
                    | InputType::Search
                    | InputType::Tel
                    | InputType::Url
                    | InputType::Email
                    | InputType::Password => {
                        Some(positive(element, "size", 20) as f32 * char_width(style))
                    }
                    InputType::Number => Some(20.0 * char_width(style)),
                    _ => None,
                };
                let style = sized(style, width, None);
                Control::Block(self.field(node, style, inner, items))
            }
        })
    }

    /// A control with a box but no content, for `appearance: none` on controls that only
    /// draw a widget.
    fn empty(&self, node: NodeId, style: &Arc<ComputedValues>) -> BlockBox {
        block(Some(node), style.clone(), BlockContents::Inline(Vec::new()))
    }

    /// Text in a control's inner box.
    fn text(&self, node: NodeId, text: &str, inner: &Arc<ComputedValues>) -> Vec<InlineItem> {
        if text.is_empty() {
            return Vec::new();
        }
        vec![InlineItem::Text {
            text: text.to_owned(),
            style: inner.clone(),
            lang: self.language(node),
        }]
    }

    /// The placeholder text of an empty field, in a `::placeholder` box that keeps the
    /// inner box's white space handling.
    fn placeholder(
        &self,
        node: NodeId,
        text: &str,
        inner: &Arc<ComputedValues>,
    ) -> Vec<InlineItem> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut style = match self.styles.pseudo(node, &PseudoElement::Placeholder) {
            Some(style) => (**style).clone(),
            None => ComputedValues::inherit_from(inner),
        };
        style.white_space_collapse = inner.white_space_collapse;
        style.text_wrap_mode = inner.text_wrap_mode;
        let style = Arc::new(style);
        vec![
            InlineItem::Start {
                style: style.clone(),
                node: None,
            },
            InlineItem::Text {
                text: text.to_owned(),
                style,
                lang: self.language(node),
            },
            InlineItem::End,
        ]
    }

    /// A control whose inner box holds `items`. Without items, the inner box still has an
    /// empty line, which gives the control its height and baseline.
    fn field(
        &self,
        node: NodeId,
        style: Arc<ComputedValues>,
        inner: Arc<ComputedValues>,
        mut items: Vec<InlineItem>,
    ) -> BlockBox {
        if items.is_empty() {
            items = self.text(node, EMPTY_LINE, &inner);
        }
        let contents = block(None, inner, BlockContents::Inline(items));
        let mut field = block(Some(node), style, BlockContents::Blocks(vec![contents]));
        field.center_contents = true;
        field
    }

    /// A `<textarea>`: its text, wrapped as its `white-space` says, in a box `cols`
    /// characters wide and `rows` lines high.
    fn textarea(
        &mut self,
        node: NodeId,
        element: &Element,
        style: &Arc<ComputedValues>,
    ) -> BlockBox {
        let inner = Arc::new(inner_style(style, true));
        let value = forms::textarea_value(self.document, node);
        let items = if value.is_empty() {
            let placeholder = element.attr("placeholder").unwrap_or("");
            self.placeholder(node, placeholder, &inner)
        } else {
            self.text(node, &value, &inner)
        };
        let width = positive(element, "cols", 20) as f32 * char_width(style);
        let height = positive(element, "rows", 2) as f32 * crate::inline::line_height(style);
        let mut area = self.field(node, sized(style, Some(width), Some(height)), inner, items);
        // Text areas start at the top.
        area.center_contents = false;
        area
    }

    /// A drop-down `<select>` or a `<keygen>`: the selected label and an arrow, wide enough
    /// for the longest label.
    fn drop_down(
        &mut self,
        node: NodeId,
        style: &Arc<ComputedValues>,
        label: String,
        labels: &[String],
    ) -> BlockBox {
        let mut inner = inner_style(style, false);
        inner.padding_left = LengthPercentage::Length(DROP_DOWN_GAP);
        inner.padding_right = LengthPercentage::Length(DROP_DOWN_GAP);
        let inner = Arc::new(inner);
        let mut items = self.text(node, &label, &inner);
        if items.is_empty() {
            items = self.text(node, EMPTY_LINE, &inner);
        }
        let widest = labels
            .iter()
            .chain([&label])
            .map(|label| crate::inline::intrinsic(&self.text(node, label, &inner), &inner).1)
            .fold(0.0, f32::max);
        let mut width = 2.0 * DROP_DOWN_GAP + widest;
        if let Some((icon, icon_width)) = self.picker_icon(node, style) {
            // An empty leader takes the space between the label and the arrow, which keeps
            // the arrow at the end.
            items.push(InlineItem::Leader {
                pattern: Arc::from(""),
                style: inner.clone(),
            });
            items.extend(icon);
            width += DROP_DOWN_GAP + icon_width;
        }
        let contents = block(None, inner, BlockContents::Inline(items));
        let mut drop_down = block(
            Some(node),
            sized(style, Some(width), None),
            BlockContents::Blocks(vec![contents]),
        );
        drop_down.center_contents = true;
        drop_down
    }

    /// The arrow of a drop-down: `::picker-icon` with its content, or else a chevron.
    /// Returns its items and its width, or `None` with `appearance: none`.
    fn picker_icon(
        &mut self,
        node: NodeId,
        style: &Arc<ComputedValues>,
    ) -> Option<(Vec<InlineItem>, f32)> {
        if style.appearance == Appearance::None {
            return None;
        }
        let icon = self
            .styles
            .pseudo(node, &PseudoElement::PickerIcon)
            .cloned()
            .unwrap_or_else(|| Arc::new(ComputedValues::inherit_from(style)));
        if icon.display == rustyprint_css::properties::types::Display::None {
            return None;
        }
        if let Content::Items { items, .. } = &icon.content {
            let lang = self.language(node);
            let mut out = vec![InlineItem::Start {
                style: icon.clone(),
                node: None,
            }];
            out.extend(self.content_items(items, &icon, lang));
            out.push(InlineItem::End);
            let width = crate::inline::intrinsic(&out, &icon).1;
            return Some((out, width));
        }
        let size = (0.75 * icon.font_size).round();
        let mut icon_style = ComputedValues::inherit_from(&icon);
        icon_style.alignment_baseline = AlignmentBaseline::Middle;
        let item = widget_item(Widget::PickerIcon, (size, size), Arc::new(icon_style));
        Some((vec![item], size))
    }

    /// A `<select>` list box: its options as rows, `size` rows high.
    fn list_box(
        &mut self,
        node: NodeId,
        element: &Element,
        style: &Arc<ComputedValues>,
    ) -> BlockBox {
        // Options have 1px of padding below.
        let row = crate::inline::line_height(style) + 1.0;
        let rows = forms::display_size(element) as f32;
        let mut style = (*sized(style, None, Some(rows * row))).clone();
        style.overflow_x = Overflow::Clip;
        style.overflow_y = Overflow::Clip;
        self.block(node, Arc::new(style))
    }

    /// An `<input type=file>`: a `::file-selector-button` and the text that no file is
    /// chosen.
    fn file_input(
        &mut self,
        node: NodeId,
        element: &Element,
        style: &Arc<ComputedValues>,
    ) -> BlockBox {
        let multiple = element.has_attr("multiple");
        let inner = Arc::new(inner_style(style, false));
        let mut items = Vec::new();
        let button = self
            .styles
            .pseudo(node, &PseudoElement::FileSelectorButton)
            .cloned()
            .unwrap_or_else(|| Arc::new(ComputedValues::inherit_from(style)));
        if button.display != rustyprint_css::properties::types::Display::None {
            let label = if multiple {
                "Choose Files"
            } else {
                "Choose File"
            };
            let button_inner = Arc::new(inner_style(&button, false));
            let text = self.text(node, label, &button_inner);
            let contents = block(None, button_inner, BlockContents::Inline(text));
            let button = block(None, button, BlockContents::Blocks(vec![contents]));
            items.push(InlineItem::Atomic(Box::new(button)));
        }
        let status = if multiple {
            "No files chosen"
        } else {
            "No file chosen"
        };
        items.extend(self.text(node, status, &inner));
        self.field(node, style.clone(), inner, items)
    }

    /// A checkbox or radio button with `appearance: base`: a box styled by CSS, holding its
    /// `::checkmark` when checked.
    fn checkmark_box(
        &mut self,
        node: NodeId,
        style: &Arc<ComputedValues>,
        checked: bool,
        radio: bool,
    ) -> BlockBox {
        let items = if checked {
            self.checkmark(node, style, radio).unwrap_or_default()
        } else {
            Vec::new()
        };
        block(Some(node), style.clone(), BlockContents::Inline(items))
    }

    /// The `::checkmark` of a checked control or a selected option: its content, or else a
    /// check mark, or a dot for radio buttons, 1em in size.
    pub(super) fn checkmark(
        &mut self,
        node: NodeId,
        style: &Arc<ComputedValues>,
        radio: bool,
    ) -> Option<Vec<InlineItem>> {
        let mark = self
            .styles
            .pseudo(node, &PseudoElement::Checkmark)
            .cloned()
            .unwrap_or_else(|| Arc::new(ComputedValues::inherit_from(style)));
        if mark.display == rustyprint_css::properties::types::Display::None {
            return None;
        }
        match &mark.content {
            Content::None => None,
            Content::Items { items, .. } => {
                let lang = self.language(node);
                let mut out = vec![InlineItem::Start {
                    style: mark.clone(),
                    node: None,
                }];
                out.extend(self.content_items(items, &mark, lang));
                out.push(InlineItem::End);
                Some(out)
            }
            Content::Normal => {
                let size = mark.font_size;
                let kind = if radio {
                    Widget::Dot
                } else {
                    Widget::Checkmark
                };
                let mut item_style = (*mark).clone();
                item_style.alignment_baseline = AlignmentBaseline::Middle;
                Some(vec![widget_item(kind, (size, size), Arc::new(item_style))])
            }
        }
    }
}
