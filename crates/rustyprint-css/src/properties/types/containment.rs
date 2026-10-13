//! Value types of containment and container queries (CSS Containment 2 and 3).

use cssparser::Parser;

use super::{IdentList, Result, fail};
use crate::properties::{ComputeContext, Property};

/// `contain`: the kinds of containment the element asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Contain {
    pub size: bool,
    pub inline_size: bool,
    pub layout: bool,
    pub style: bool,
    pub paint: bool,
}

impl Contain {
    pub const NONE: Self = Contain {
        size: false,
        inline_size: false,
        layout: false,
        style: false,
        paint: false,
    };
    pub const STRICT: Self = Contain {
        size: true,
        inline_size: false,
        layout: true,
        style: true,
        paint: true,
    };
    pub const CONTENT: Self = Contain {
        size: false,
        inline_size: false,
        layout: true,
        style: true,
        paint: true,
    };

    /// Containment in both, or none.
    pub fn union(self, other: Contain) -> Contain {
        Contain {
            size: self.size || other.size,
            inline_size: (self.inline_size || other.inline_size) && !(self.size || other.size),
            layout: self.layout || other.layout,
            style: self.style || other.style,
            paint: self.paint || other.paint,
        }
    }
}

impl Property for Contain {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        if *computed == Contain::NONE {
            return "none".into();
        }
        if *computed == Contain::STRICT {
            return "strict".into();
        }
        if *computed == Contain::CONTENT {
            return "content".into();
        }
        let mut words = Vec::new();
        if computed.size {
            words.push("size");
        }
        if computed.inline_size {
            words.push("inline-size");
        }
        if computed.layout {
            words.push("layout");
        }
        if computed.style {
            words.push("style");
        }
        if computed.paint {
            words.push("paint");
        }
        words.join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let first = input.expect_ident_cloned()?.to_ascii_lowercase();
        match &*first {
            "none" => return Ok(Contain::NONE),
            "strict" => return Ok(Contain::STRICT),
            "content" => return Ok(Contain::CONTENT),
            _ => {}
        }
        let mut value = Contain::NONE;
        let mut word = Some(first);
        while let Some(w) = word.take() {
            match &*w {
                "size" if !value.size && !value.inline_size => value.size = true,
                "inline-size" if !value.size && !value.inline_size => value.inline_size = true,
                "layout" if !value.layout => value.layout = true,
                "style" if !value.style => value.style = true,
                "paint" if !value.paint => value.paint = true,
                _ => return fail(),
            }
            word = input
                .try_parse(|input| input.expect_ident_cloned())
                .ok()
                .map(|w| w.to_ascii_lowercase());
        }
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(
    /// `content-visibility`: whether the element's contents are rendered. Everything in a
    /// PDF is relevant to the user, so `auto` renders them.
    ContentVisibility { Visible = "visible", Auto = "auto", Hidden = "hidden" }
);

/// The axes a size container's size is known in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerSize {
    /// `size`: both axes.
    Both,
    /// `inline-size`: the inline axis.
    Inline,
}

/// `container-type`: what kinds of queries the element answers. Every element answers style
/// queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContainerType {
    pub size: Option<ContainerSize>,
    pub scroll_state: bool,
}

impl ContainerType {
    pub const NORMAL: Self = ContainerType {
        size: None,
        scroll_state: false,
    };
}

impl Property for ContainerType {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let mut words = Vec::new();
        match computed.size {
            Some(ContainerSize::Both) => words.push("size"),
            Some(ContainerSize::Inline) => words.push("inline-size"),
            None => {}
        }
        if computed.scroll_state {
            words.push("scroll-state");
        }
        if words.is_empty() {
            "normal".into()
        } else {
            words.join(" ")
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = ContainerType::NORMAL;
        let first = input.expect_ident_cloned()?.to_ascii_lowercase();
        if first == "normal" {
            return Ok(value);
        }
        let mut word = Some(first);
        while let Some(w) = word.take() {
            match &*w {
                "size" if value.size.is_none() => value.size = Some(ContainerSize::Both),
                "inline-size" if value.size.is_none() => value.size = Some(ContainerSize::Inline),
                "scroll-state" if !value.scroll_state => value.scroll_state = true,
                _ => return fail(),
            }
            word = input
                .try_parse(|input| input.expect_ident_cloned())
                .ok()
                .map(|w| w.to_ascii_lowercase());
        }
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `container-name`: `none` or the names the element answers queries by.
pub type ContainerName = IdentList<1>;
