//! Lengths and their units.

/// Units of `<length>`. Font-relative units starting with `R` refer to the root element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    // Absolute
    Px,
    Cm,
    Mm,
    Q,
    In,
    Pc,
    Pt,
    // Font-relative
    Em,
    Rem,
    Ex,
    Rex,
    Ch,
    Rch,
    Cap,
    Rcap,
    Ic,
    Ric,
    Lh,
    Rlh,
    // Viewport (the page area); small, large and dynamic variants are identical on paper.
    Vw,
    Vh,
    Vi,
    Vb,
    Vmin,
    Vmax,
    // Container query
    Cqw,
    Cqh,
    Cqi,
    Cqb,
    Cqmin,
    Cqmax,
}

impl LengthUnit {
    /// Parses a unit name, ignoring ASCII case.
    pub fn from_name(name: &str) -> Option<Self> {
        use LengthUnit::*;
        let lower = name.to_ascii_lowercase();
        // `sv*`, `lv*` and `dv*` equal `v*`: a page never resizes.
        let viewport = lower
            .strip_prefix('s')
            .or_else(|| lower.strip_prefix('l'))
            .or_else(|| lower.strip_prefix('d'));
        let lower = match viewport {
            Some(rest) if rest.starts_with('v') && !rest.is_empty() => rest.to_owned(),
            _ => lower,
        };
        Some(match &*lower {
            "px" => Px,
            "cm" => Cm,
            "mm" => Mm,
            "q" => Q,
            "in" => In,
            "pc" => Pc,
            "pt" => Pt,
            "em" => Em,
            "rem" => Rem,
            "ex" => Ex,
            "rex" => Rex,
            "ch" => Ch,
            "rch" => Rch,
            "cap" => Cap,
            "rcap" => Rcap,
            "ic" => Ic,
            "ric" => Ric,
            "lh" => Lh,
            "rlh" => Rlh,
            "vw" => Vw,
            "vh" => Vh,
            "vi" => Vi,
            "vb" => Vb,
            "vmin" => Vmin,
            "vmax" => Vmax,
            "cqw" => Cqw,
            "cqh" => Cqh,
            "cqi" => Cqi,
            "cqb" => Cqb,
            "cqmin" => Cqmin,
            "cqmax" => Cqmax,
            _ => return None,
        })
    }

    /// CSS pixels per unit for absolute units.
    pub fn absolute_px(self) -> Option<f32> {
        use LengthUnit::*;
        Some(match self {
            Px => 1.0,
            In => 96.0,
            Cm => 96.0 / 2.54,
            Mm => 96.0 / 25.4,
            Q => 96.0 / 101.6,
            Pt => 96.0 / 72.0,
            Pc => 16.0,
            _ => return None,
        })
    }
}

/// A length with its unit, as specified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: LengthUnit,
}

/// What relative lengths resolve against. All values are CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LengthContext {
    pub font: FontMetrics,
    pub root_font: FontMetrics,
    /// The page area: what viewport units refer to.
    pub viewport: (f32, f32),
    /// The query container, or the page area when there is none.
    pub container: (f32, f32),
    /// Whether the inline axis is vertical, which swaps `vi`/`vb` and `cqi`/`cqb`.
    pub vertical: bool,
    /// The effective `zoom`, which absolute lengths are multiplied by.
    pub zoom: f32,
    /// What `random()` picks its values from: one for each element styled, or `None` where
    /// there is no element.
    pub random_seed: Option<u64>,
}

/// Font metrics that font-relative units resolve against, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub size: f32,
    pub x_height: f32,
    /// Advance of `0`.
    pub zero_advance: f32,
    pub cap_height: f32,
    /// Advance of `水` (CJK water ideograph).
    pub ideograph_advance: f32,
    pub line_height: f32,
}

impl FontMetrics {
    /// Metrics without a font, using the fallbacks CSS Values defines.
    pub fn fallback(size: f32) -> Self {
        Self {
            size,
            x_height: size * 0.5,
            zero_advance: size * 0.5,
            cap_height: size,
            ideograph_advance: size,
            line_height: size * 1.2,
        }
    }
}

impl LengthContext {
    /// A context with the initial font size (16px) and no font, for media queries.
    pub fn initial(viewport: (f32, f32)) -> Self {
        let font = FontMetrics::fallback(16.0);
        Self {
            font,
            root_font: font,
            viewport,
            container: viewport,
            vertical: false,
            zoom: 1.0,
            random_seed: None,
        }
    }
}

impl Length {
    pub const fn px(value: f32) -> Self {
        Self {
            value,
            unit: LengthUnit::Px,
        }
    }

    /// The length in CSS pixels.
    pub fn to_px(self, ctx: &LengthContext) -> f32 {
        use LengthUnit::*;
        if let Some(px) = self.unit.absolute_px() {
            return self.value * px * ctx.zoom;
        }
        let (vw, vh) = ctx.viewport;
        let (cw, ch) = ctx.container;
        let (vi, vb, ci, cb) = if ctx.vertical {
            (vh, vw, ch, cw)
        } else {
            (vw, vh, cw, ch)
        };
        let base = match self.unit {
            Em => ctx.font.size,
            Rem => ctx.root_font.size,
            Ex => ctx.font.x_height,
            Rex => ctx.root_font.x_height,
            Ch => ctx.font.zero_advance,
            Rch => ctx.root_font.zero_advance,
            Cap => ctx.font.cap_height,
            Rcap => ctx.root_font.cap_height,
            Ic => ctx.font.ideograph_advance,
            Ric => ctx.root_font.ideograph_advance,
            Lh => ctx.font.line_height,
            Rlh => ctx.root_font.line_height,
            Vw => vw / 100.0,
            Vh => vh / 100.0,
            Vi => vi / 100.0,
            Vb => vb / 100.0,
            Vmin => vw.min(vh) / 100.0,
            Vmax => vw.max(vh) / 100.0,
            Cqw => cw / 100.0,
            Cqh => ch / 100.0,
            Cqi => ci / 100.0,
            Cqb => cb / 100.0,
            Cqmin => cw.min(ch) / 100.0,
            Cqmax => cw.max(ch) / 100.0,
            Px | Cm | Mm | Q | In | Pc | Pt => unreachable!("absolute units return early"),
        };
        self.value * base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(value: f32, unit: &str) -> f32 {
        let ctx = LengthContext::initial((800.0, 600.0));
        Length {
            value,
            unit: LengthUnit::from_name(unit).unwrap(),
        }
        .to_px(&ctx)
    }

    #[test]
    fn absolute_units() {
        assert_eq!(px(1.0, "in"), 96.0);
        assert_eq!(px(72.0, "pt"), 96.0);
        assert_eq!(px(6.0, "pc"), 96.0);
        assert!((px(2.54, "cm") - 96.0).abs() < 1e-3);
        assert!((px(25.4, "mm") - 96.0).abs() < 1e-3);
        assert!((px(101.6, "Q") - 96.0).abs() < 1e-3);
    }

    #[test]
    fn relative_units() {
        assert_eq!(px(2.0, "em"), 32.0);
        assert_eq!(px(1.0, "REM"), 16.0);
        assert_eq!(px(10.0, "vw"), 80.0);
        assert_eq!(px(10.0, "dvh"), 60.0);
        assert_eq!(px(10.0, "svmin"), 60.0);
        assert_eq!(px(1.0, "lh"), 19.2);
    }

    #[test]
    fn unknown_units() {
        assert!(LengthUnit::from_name("furlong").is_none());
        assert!(LengthUnit::from_name("s").is_none());
    }
}
