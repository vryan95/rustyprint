//! `@page` rules (CSS Paged Media 3): page selectors, descriptors, margin boxes, and the
//! styles of pages.

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use crate::properties::ComputedValues;
use crate::properties::types::{PageSide, parse_length_percentage};
use crate::values::LengthContext;

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// A page selector, such as `chapter:first`. One without a name or pseudo-classes matches
/// every page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSelector {
    pub name: Option<String>,
    pub pseudo: Vec<PagePseudo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagePseudo {
    First,
    Left,
    Right,
    Blank,
}

impl PageSelector {
    /// Parses a comma-separated page selector list; an empty prelude matches every page.
    pub fn parse_list(input: &mut Parser<'_>) -> Result<Vec<PageSelector>> {
        if input.is_exhausted() {
            return Ok(vec![PageSelector {
                name: None,
                pseudo: Vec::new(),
            }]);
        }
        input.parse_comma_separated(|input| {
            let name = input
                .try_parse(|input| input.expect_ident_cloned())
                .ok()
                .map(|n| n.to_string());
            let mut pseudo = Vec::new();
            while input.try_parse(|input| input.expect_colon()).is_ok() {
                pseudo.push(match &*input.expect_ident()?.to_ascii_lowercase() {
                    "first" => PagePseudo::First,
                    "left" => PagePseudo::Left,
                    "right" => PagePseudo::Right,
                    "blank" => PagePseudo::Blank,
                    _ => return fail(),
                });
            }
            if name.is_none() && pseudo.is_empty() {
                return fail();
            }
            Ok(PageSelector { name, pseudo })
        })
    }

    /// Specificity (CSS Paged Media 3 § 3.4.2): a page name, then `:first` and `:blank`,
    /// then `:left` and `:right`.
    pub fn specificity(&self) -> (u32, u32, u32) {
        let count = |wanted: &[PagePseudo]| {
            self.pseudo.iter().filter(|p| wanted.contains(p)).count() as u32
        };
        (
            u32::from(self.name.is_some()),
            count(&[PagePseudo::First, PagePseudo::Blank]),
            count(&[PagePseudo::Left, PagePseudo::Right]),
        )
    }

    pub fn matches(&self, page: &PageKey) -> bool {
        self.name
            .as_deref()
            .is_none_or(|name| page.name.as_deref() == Some(name))
            && self.pseudo.iter().all(|pseudo| match pseudo {
                PagePseudo::First => page.first,
                PagePseudo::Left => page.side == PageSide::Left,
                PagePseudo::Right => page.side == PageSide::Right,
                PagePseudo::Blank => page.blank,
            })
    }
}

/// What page selectors match a page against.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageKey {
    /// The page name from the `page` property of the content that starts the page.
    pub name: Option<Arc<str>>,
    /// Whether this is the document's first page.
    pub first: bool,
    pub side: PageSide,
    /// Whether the page was left blank by a forced break to a left or right page.
    pub blank: bool,
}

macro_rules! margin_boxes {
    ($($variant:ident $name:literal,)+) => {
        /// A page-margin box (CSS Paged Media 3 § 4.2).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum MarginBox {
            $($variant,)+
        }

        impl MarginBox {
            pub const ALL: [MarginBox; 16] = [$(MarginBox::$variant,)+];

            pub fn name(self) -> &'static str {
                match self {
                    $(MarginBox::$variant => $name,)+
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(MarginBox::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

margin_boxes! {
    TopLeftCorner "top-left-corner",
    TopLeft "top-left",
    TopCenter "top-center",
    TopRight "top-right",
    TopRightCorner "top-right-corner",
    RightTop "right-top",
    RightMiddle "right-middle",
    RightBottom "right-bottom",
    BottomRightCorner "bottom-right-corner",
    BottomRight "bottom-right",
    BottomCenter "bottom-center",
    BottomLeft "bottom-left",
    BottomLeftCorner "bottom-left-corner",
    LeftBottom "left-bottom",
    LeftMiddle "left-middle",
    LeftTop "left-top",
}

/// The `size` descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// The size from the request, in its orientation.
    Auto,
    /// The size from the request, turned to an orientation if it is not in it already.
    Orientation { landscape: bool },
    /// A width and height in CSS pixels.
    Size(f32, f32),
}

/// The `page-orientation` descriptor: how the PDF viewer shows the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrientation {
    Upright,
    RotateLeft,
    RotateRight,
}

/// The `marks` descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marks {
    pub crop: bool,
    pub cross: bool,
}

/// The `page-margin-safety` descriptor. PDF pages have no unprintable area, so it has no
/// effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginSafety {
    None,
    Clamp,
    Add,
}

/// A parsed `@page` descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum Descriptor {
    Size(PageSize),
    Orientation(PageOrientation),
    /// `None` for `auto`.
    Bleed(Option<f32>),
    Marks(Marks),
    MarginSafety(MarginSafety),
}

/// Named page sizes in millimetres (CSS Paged Media 3 § 7.1.1).
fn named_size(name: &str) -> Option<(f32, f32)> {
    const INCH: f32 = 25.4;
    Some(match name {
        "a5" => (148.0, 210.0),
        "a4" => (210.0, 297.0),
        "a3" => (297.0, 420.0),
        "b5" => (176.0, 250.0),
        "b4" => (250.0, 353.0),
        "jis-b5" => (182.0, 257.0),
        "jis-b4" => (257.0, 364.0),
        "letter" => (8.5 * INCH, 11.0 * INCH),
        "legal" => (8.5 * INCH, 14.0 * INCH),
        "ledger" => (11.0 * INCH, 17.0 * INCH),
        _ => return None,
    })
}

/// A non-negative `<length>` in CSS pixels, with font-relative units against the initial font.
fn parse_length(input: &mut Parser<'_>, non_negative: bool) -> Result<f32> {
    let value = parse_length_percentage(input, false, non_negative)?;
    let ctx = LengthContext::initial((0.0, 0.0));
    Ok(match value {
        crate::properties::types::SpecifiedLengthPercentage::Length(length) => length.to_px(&ctx),
        crate::properties::types::SpecifiedLengthPercentage::Calc(node) => node
            .absolutize(&ctx)
            .evaluate(&crate::values::CalcContext::default())
            .map_or(0.0, |v| v as f32),
        crate::properties::types::SpecifiedLengthPercentage::Percentage(_) => return fail(),
    })
}

impl Descriptor {
    /// Whether `name` is an `@page` descriptor rather than a property.
    pub fn is_descriptor(name: &str) -> bool {
        matches!(
            name,
            "size" | "page-orientation" | "bleed" | "marks" | "page-margin-safety"
        )
    }

    /// Parses the value of the descriptor `name`.
    pub fn parse(name: &str, value: &str) -> Option<Descriptor> {
        let mut input = Parser::new(value);
        input
            .parse_entirely(|input| Self::parse_value(name, input))
            .ok()
    }

    fn parse_value(name: &str, input: &mut Parser<'_>) -> Result<Descriptor> {
        let ident = |input: &mut Parser<'_>| -> Result<String> {
            Ok(input.expect_ident()?.to_ascii_lowercase())
        };
        Ok(match name {
            "size" => {
                if let Ok(width) = input.try_parse(|input| parse_length(input, true)) {
                    let height = input
                        .try_parse(|input| parse_length(input, true))
                        .unwrap_or(width);
                    return Ok(Descriptor::Size(PageSize::Size(width, height)));
                }
                let mut size = None;
                let mut landscape = None;
                for _ in 0..2 {
                    let Ok(word) = input.try_parse(ident) else {
                        break;
                    };
                    match word.as_str() {
                        "auto" if size.is_none() && landscape.is_none() => {
                            return Ok(Descriptor::Size(PageSize::Auto));
                        }
                        "portrait" if landscape.is_none() => landscape = Some(false),
                        "landscape" if landscape.is_none() => landscape = Some(true),
                        other if size.is_none() => {
                            size = Some(named_size(other).ok_or_else(|| ParseError::custom(()))?)
                        }
                        _ => return fail(),
                    }
                }
                let mm = 96.0 / 25.4;
                match (size, landscape) {
                    (None, None) => return fail(),
                    (None, Some(landscape)) => {
                        Descriptor::Size(PageSize::Orientation { landscape })
                    }
                    (Some((w, h)), landscape) => {
                        let (w, h) = (w * mm, h * mm);
                        Descriptor::Size(if landscape == Some(true) {
                            PageSize::Size(h, w)
                        } else {
                            PageSize::Size(w, h)
                        })
                    }
                }
            }
            "page-orientation" => Descriptor::Orientation(match ident(input)?.as_str() {
                "upright" => PageOrientation::Upright,
                "rotate-left" => PageOrientation::RotateLeft,
                "rotate-right" => PageOrientation::RotateRight,
                _ => return fail(),
            }),
            "bleed" => {
                if input
                    .try_parse(|input| input.expect_ident_matching("auto"))
                    .is_ok()
                {
                    Descriptor::Bleed(None)
                } else {
                    Descriptor::Bleed(Some(parse_length(input, false)?))
                }
            }
            "marks" => {
                let mut marks = Marks::default();
                let first = ident(input)?;
                if first == "none" {
                    return Ok(Descriptor::Marks(marks));
                }
                let mut word = Some(first);
                while let Some(current) = word.take() {
                    let slot = match current.as_str() {
                        "crop" => &mut marks.crop,
                        "cross" => &mut marks.cross,
                        _ => return fail(),
                    };
                    if *slot {
                        return fail();
                    }
                    *slot = true;
                    word = input.try_parse(ident).ok();
                }
                Descriptor::Marks(marks)
            }
            "page-margin-safety" => Descriptor::MarginSafety(match ident(input)?.as_str() {
                "none" => MarginSafety::None,
                "clamp" => MarginSafety::Clamp,
                "add" => MarginSafety::Add,
                _ => return fail(),
            }),
            _ => return fail(),
        })
    }
}

/// The style of one page: its page box, descriptors and margin boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct PageStyle {
    /// The page context: margins, padding, border, background and inherited properties
    /// for the margin boxes.
    pub style: Arc<ComputedValues>,
    /// Which margins (top, right, bottom, left) an `@page` rule sets; the others use the
    /// request's default margin.
    pub margins_set: [bool; 4],
    pub size: PageSize,
    pub orientation: PageOrientation,
    /// `None` for `auto`: 6pt with crop marks, none otherwise.
    pub bleed: Option<f32>,
    pub marks: Marks,
    pub margin_safety: MarginSafety,
    /// The margin boxes that `@page` rules style, in [`MarginBox::ALL`] order.
    pub margin_boxes: Vec<(MarginBox, Arc<ComputedValues>)>,
}

impl PageStyle {
    /// The bleed around the page in CSS pixels.
    pub fn used_bleed(&self) -> f32 {
        self.bleed.unwrap_or(if self.marks.crop {
            6.0 * 96.0 / 72.0
        } else {
            0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selectors(css: &str) -> Option<Vec<PageSelector>> {
        let mut input = Parser::new(css);
        input.parse_entirely(PageSelector::parse_list).ok()
    }

    #[test]
    fn page_selectors() {
        let list = selectors("chapter:first, :left:blank").unwrap();
        assert_eq!(list[0].name.as_deref(), Some("chapter"));
        assert_eq!(list[0].specificity(), (1, 1, 0));
        assert_eq!(list[1].specificity(), (0, 1, 1));
        assert!(selectors(":nth(2)").is_none());
        assert_eq!(selectors("").unwrap()[0].specificity(), (0, 0, 0));
    }

    #[test]
    fn descriptors() {
        let a4 = Descriptor::parse("size", "A4 landscape").unwrap();
        let Descriptor::Size(PageSize::Size(w, h)) = a4 else {
            panic!("{a4:?}")
        };
        assert!(w > h);
        assert_eq!(
            Descriptor::parse("size", "10cm"),
            Some(Descriptor::Size(PageSize::Size(
                10.0 * 96.0 / 2.54,
                10.0 * 96.0 / 2.54
            )))
        );
        assert_eq!(
            Descriptor::parse("size", "portrait"),
            Some(Descriptor::Size(PageSize::Orientation { landscape: false }))
        );
        assert!(Descriptor::parse("size", "A4 A5").is_none());
        assert_eq!(
            Descriptor::parse("marks", "cross crop"),
            Some(Descriptor::Marks(Marks {
                crop: true,
                cross: true
            }))
        );
        assert!(Descriptor::parse("bleed", "1em").is_some());
        assert!(Descriptor::parse("page-orientation", "sideways").is_none());
    }
}
