//! Value types of grid layout (CSS Grid Layout 2): track lists, line placement and areas.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{
    LengthPercentage, Result, SpecifiedLengthPercentage, fail, length_percentage_css,
    parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

/// A `<track-breadth>`: one bound of a track's size.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackBreadth<L> {
    Length(L),
    /// `<flex>`: a share of the space left, in `fr`.
    Flex(f32),
    MinContent,
    MaxContent,
    Auto,
}

/// A `<track-size>`: the size of a grid track.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackSize<L> {
    Breadth(TrackBreadth<L>),
    /// `minmax(min, max)`; the minimum is never `<flex>`.
    MinMax(TrackBreadth<L>, TrackBreadth<L>),
    /// `fit-content(limit)`.
    FitContent(L),
}

/// How many times `repeat()` repeats its tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatCount {
    Count(u32),
    /// As many as fit.
    AutoFill,
    /// As many as fit, with the empty ones collapsed.
    AutoFit,
}

/// `repeat()`: tracks, with the line names around them, repeated.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRepeat<L> {
    pub count: RepeatCount,
    /// The names of the lines around the tracks: one more than there are tracks.
    pub names: Vec<Vec<Arc<str>>>,
    pub tracks: Vec<TrackSize<L>>,
}

/// One entry of a track list: a track or a `repeat()`.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackEntry<L> {
    Track(TrackSize<L>),
    Repeat(TrackRepeat<L>),
}

/// A `<track-list>`: tracks with the names of the lines between them.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackList<L> {
    /// The names of the lines around the entries: one more than there are entries.
    pub names: Vec<Vec<Arc<str>>>,
    pub entries: Vec<TrackEntry<L>>,
}

/// One entry of a subgrid's `<line-name-list>`.
#[derive(Debug, Clone, PartialEq)]
pub enum SubgridNames {
    Names(Vec<Arc<str>>),
    /// `repeat()` of line names, with a count or `auto-fill`.
    Repeat(RepeatCount, Vec<Vec<Arc<str>>>),
}

/// `grid-template-columns` and `grid-template-rows`.
#[derive(Debug, Clone, PartialEq)]
pub enum GridTemplate<L> {
    None,
    Tracks(TrackList<L>),
    /// The tracks of the parent grid that the item spans, with line names of its own.
    Subgrid(Vec<SubgridNames>),
}

impl TrackBreadth<SpecifiedLengthPercentage> {
    fn compute(&self, ctx: &ComputeContext) -> TrackBreadth<LengthPercentage> {
        match self {
            TrackBreadth::Length(value) => TrackBreadth::Length(value.compute(ctx, true)),
            TrackBreadth::Flex(fr) => TrackBreadth::Flex(*fr),
            TrackBreadth::MinContent => TrackBreadth::MinContent,
            TrackBreadth::MaxContent => TrackBreadth::MaxContent,
            TrackBreadth::Auto => TrackBreadth::Auto,
        }
    }
}

impl TrackSize<SpecifiedLengthPercentage> {
    pub(crate) fn compute(&self, ctx: &ComputeContext) -> TrackSize<LengthPercentage> {
        match self {
            TrackSize::Breadth(breadth) => TrackSize::Breadth(breadth.compute(ctx)),
            TrackSize::MinMax(min, max) => TrackSize::MinMax(min.compute(ctx), max.compute(ctx)),
            TrackSize::FitContent(limit) => TrackSize::FitContent(limit.compute(ctx, true)),
        }
    }
}

impl TrackList<SpecifiedLengthPercentage> {
    pub(crate) fn compute(&self, ctx: &ComputeContext) -> TrackList<LengthPercentage> {
        TrackList {
            names: self.names.clone(),
            entries: self
                .entries
                .iter()
                .map(|entry| match entry {
                    TrackEntry::Track(size) => TrackEntry::Track(size.compute(ctx)),
                    TrackEntry::Repeat(repeat) => TrackEntry::Repeat(TrackRepeat {
                        count: repeat.count,
                        names: repeat.names.clone(),
                        tracks: repeat.tracks.iter().map(|t| t.compute(ctx)).collect(),
                    }),
                })
                .collect(),
        }
    }
}

impl<L> TrackBreadth<L> {
    fn is_fixed(&self) -> bool {
        matches!(self, TrackBreadth::Length(_))
    }

    fn is_flex(&self) -> bool {
        matches!(self, TrackBreadth::Flex(_))
    }
}

impl<L> TrackSize<L> {
    /// Whether the size is a `<fixed-size>`, as tracks repeated to fill the space must be.
    fn is_fixed(&self) -> bool {
        match self {
            TrackSize::Breadth(breadth) => breadth.is_fixed(),
            TrackSize::MinMax(min, max) => min.is_fixed() || (max.is_fixed() && !min.is_flex()),
            TrackSize::FitContent(_) => false,
        }
    }
}

/// Whether a word can be a line or area name: not a CSS-wide keyword, `default`, `span` or
/// `auto`.
fn is_custom_ident(word: &str) -> bool {
    !matches!(
        &*word.to_ascii_lowercase(),
        "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default" | "span" | "auto"
    )
}

fn parse_custom_ident(input: &mut Parser<'_>) -> Result<Arc<str>> {
    let ident = input.expect_ident_cloned()?;
    if !is_custom_ident(&ident) {
        return fail();
    }
    Ok(Arc::from(&*ident))
}

/// `<line-names>`: `[ <custom-ident>* ]`.
fn parse_line_names(input: &mut Parser<'_>) -> Result<Vec<Arc<str>>> {
    input.expect_square_bracket_block()?;
    input.parse_nested_block(|input| {
        let mut names = Vec::new();
        while !input.is_exhausted() {
            names.push(parse_custom_ident(input)?);
        }
        Ok(names)
    })
}

/// `<track-breadth>`, or an `<inflexible-breadth>` without `flex`.
fn parse_breadth(
    input: &mut Parser<'_>,
    flex: bool,
) -> Result<TrackBreadth<SpecifiedLengthPercentage>> {
    if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
        return Ok(match &*keyword.to_ascii_lowercase() {
            "min-content" => TrackBreadth::MinContent,
            "max-content" => TrackBreadth::MaxContent,
            "auto" => TrackBreadth::Auto,
            _ => return fail(),
        });
    }
    if flex
        && let Ok(fr) = input.try_parse(|input| match input.next()? {
            Token::Dimension { value, unit, .. }
                if unit.eq_ignore_ascii_case("fr") && *value >= 0.0 =>
            {
                Ok(*value)
            }
            _ => fail(),
        })
    {
        return Ok(TrackBreadth::Flex(fr));
    }
    Ok(TrackBreadth::Length(parse_length_percentage(
        input, true, true,
    )?))
}

/// `<track-size>`.
pub(crate) fn parse_track_size(
    input: &mut Parser<'_>,
) -> Result<TrackSize<SpecifiedLengthPercentage>> {
    if input
        .try_parse(|input| input.expect_function_matching("minmax"))
        .is_ok()
    {
        return input.parse_nested_block(|input| {
            let min = parse_breadth(input, false)?;
            input.expect_comma()?;
            let max = parse_breadth(input, true)?;
            Ok(TrackSize::MinMax(min, max))
        });
    }
    if input
        .try_parse(|input| input.expect_function_matching("fit-content"))
        .is_ok()
    {
        return input.parse_nested_block(|input| {
            Ok(TrackSize::FitContent(parse_length_percentage(
                input, true, true,
            )?))
        });
    }
    Ok(TrackSize::Breadth(parse_breadth(input, true)?))
}

fn parse_repeat_count(input: &mut Parser<'_>, auto_fit: bool) -> Result<RepeatCount> {
    if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
        return match &*keyword.to_ascii_lowercase() {
            "auto-fill" => Ok(RepeatCount::AutoFill),
            "auto-fit" if auto_fit => Ok(RepeatCount::AutoFit),
            _ => fail(),
        };
    }
    let count = input.expect_integer()?;
    if count < 1 {
        return fail();
    }
    // Browsers cap repetitions; so does this, to keep the grid a sane size.
    Ok(RepeatCount::Count(count.min(10_000) as u32))
}

/// `repeat( [ <integer> | auto-fill | auto-fit ], [ <line-names>? <track-size> ]+
/// <line-names>? )`.
fn parse_repeat(input: &mut Parser<'_>) -> Result<TrackRepeat<SpecifiedLengthPercentage>> {
    input.expect_function_matching("repeat")?;
    input.parse_nested_block(|input| {
        let count = parse_repeat_count(input, true)?;
        input.expect_comma()?;
        let mut names = vec![Vec::new()];
        let mut tracks = Vec::new();
        loop {
            if let Ok(found) = input.try_parse(parse_line_names) {
                let last = names.last_mut().expect("names has a slot");
                if !last.is_empty() {
                    return fail();
                }
                *last = found;
            }
            match input.try_parse(parse_track_size) {
                Ok(size) => {
                    tracks.push(size);
                    names.push(Vec::new());
                }
                Err(_) => break,
            }
        }
        if tracks.is_empty() || !input.is_exhausted() {
            return fail();
        }
        // Repeating to fill the space needs tracks of a fixed size.
        if !matches!(count, RepeatCount::Count(_)) && !tracks.iter().all(TrackSize::is_fixed) {
            return fail();
        }
        Ok(TrackRepeat {
            count,
            names,
            tracks,
        })
    })
}

/// `<track-list> | <auto-track-list>`, or `<explicit-track-list>` when `repeats` is false.
pub(crate) fn parse_track_list(
    input: &mut Parser<'_>,
    repeats: bool,
) -> Result<TrackList<SpecifiedLengthPercentage>> {
    let mut names = vec![Vec::new()];
    let mut entries = Vec::new();
    loop {
        if let Ok(found) = input.try_parse(parse_line_names) {
            let last = names.last_mut().expect("names has a slot");
            if !last.is_empty() {
                return fail();
            }
            *last = found;
        }
        if let Ok(size) = input.try_parse(parse_track_size) {
            entries.push(TrackEntry::Track(size));
        } else if repeats && let Ok(repeat) = input.try_parse(parse_repeat) {
            entries.push(TrackEntry::Repeat(repeat));
        } else {
            break;
        }
        names.push(Vec::new());
    }
    if entries.is_empty() {
        return fail();
    }
    // With a repeat that fills the space, every other track must have a fixed size too.
    let auto_repeats = entries
        .iter()
        .filter(|e| matches!(e, TrackEntry::Repeat(r) if !matches!(r.count, RepeatCount::Count(_))))
        .count();
    if auto_repeats > 1 {
        return fail();
    }
    if auto_repeats == 1
        && !entries.iter().all(|entry| match entry {
            TrackEntry::Track(size) => size.is_fixed(),
            TrackEntry::Repeat(repeat) => repeat.tracks.iter().all(TrackSize::is_fixed),
        })
    {
        return fail();
    }
    Ok(TrackList { names, entries })
}

/// `subgrid <line-name-list>?`.
fn parse_subgrid(input: &mut Parser<'_>) -> Result<Vec<SubgridNames>> {
    input.expect_ident_matching("subgrid")?;
    let mut list = Vec::new();
    loop {
        if let Ok(names) = input.try_parse(parse_line_names) {
            list.push(SubgridNames::Names(names));
            continue;
        }
        let repeat = input.try_parse(|input| {
            input.expect_function_matching("repeat")?;
            input.parse_nested_block(|input| {
                let count = parse_repeat_count(input, false)?;
                input.expect_comma()?;
                let mut names = vec![parse_line_names(input)?];
                while let Ok(more) = input.try_parse(parse_line_names) {
                    names.push(more);
                }
                Ok(SubgridNames::Repeat(count, names))
            })
        });
        match repeat {
            Ok(repeat) => list.push(repeat),
            Err(_) => break,
        }
    }
    Ok(list)
}

fn names_css(names: &[Arc<str>]) -> String {
    let words: Vec<String> = names
        .iter()
        .map(|name| {
            let mut out = String::new();
            let _ = cssparser::serialize_identifier(name, &mut out);
            out
        })
        .collect();
    format!("[{}]", words.join(" "))
}

fn breadth_css(breadth: &TrackBreadth<LengthPercentage>) -> String {
    match breadth {
        TrackBreadth::Length(value) => length_percentage_css(value),
        TrackBreadth::Flex(fr) => format!("{}fr", serialize::number(f64::from(*fr))),
        TrackBreadth::MinContent => "min-content".into(),
        TrackBreadth::MaxContent => "max-content".into(),
        TrackBreadth::Auto => "auto".into(),
    }
}

pub(crate) fn track_size_css(size: &TrackSize<LengthPercentage>) -> String {
    match size {
        TrackSize::Breadth(breadth) => breadth_css(breadth),
        TrackSize::MinMax(min, max) => {
            format!("minmax({}, {})", breadth_css(min), breadth_css(max))
        }
        TrackSize::FitContent(limit) => format!("fit-content({})", length_percentage_css(limit)),
    }
}

fn count_css(count: RepeatCount) -> String {
    match count {
        RepeatCount::Count(n) => n.to_string(),
        RepeatCount::AutoFill => "auto-fill".into(),
        RepeatCount::AutoFit => "auto-fit".into(),
    }
}

/// Tracks with the names of the lines around them, as CSS text.
fn tracks_css<T>(names: &[Vec<Arc<str>>], items: &[T], item_css: impl Fn(&T) -> String) -> String {
    let mut words = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if !names[i].is_empty() {
            words.push(names_css(&names[i]));
        }
        words.push(item_css(item));
    }
    if let Some(last) = names.get(items.len())
        && !last.is_empty()
    {
        words.push(names_css(last));
    }
    words.join(" ")
}

pub(crate) fn track_list_css(list: &TrackList<LengthPercentage>) -> String {
    tracks_css(&list.names, &list.entries, |entry| match entry {
        TrackEntry::Track(size) => track_size_css(size),
        TrackEntry::Repeat(repeat) => format!(
            "repeat({}, {})",
            count_css(repeat.count),
            tracks_css(&repeat.names, &repeat.tracks, track_size_css)
        ),
    })
}

impl Property for GridTemplate<LengthPercentage> {
    type Specified = GridTemplate<SpecifiedLengthPercentage>;
    type Computed = GridTemplate<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            GridTemplate::None => "none".into(),
            GridTemplate::Tracks(list) => track_list_css(list),
            GridTemplate::Subgrid(list) => {
                let mut words = vec!["subgrid".to_owned()];
                for entry in list {
                    words.push(match entry {
                        SubgridNames::Names(names) => names_css(names),
                        SubgridNames::Repeat(count, names) => format!(
                            "repeat({}, {})",
                            count_css(*count),
                            names
                                .iter()
                                .map(|n| names_css(n))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    });
                }
                words.join(" ")
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(GridTemplate::None);
        }
        if let Ok(list) = input.try_parse(parse_subgrid) {
            return Ok(GridTemplate::Subgrid(list));
        }
        Ok(GridTemplate::Tracks(parse_track_list(input, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            GridTemplate::None => GridTemplate::None,
            GridTemplate::Tracks(list) => GridTemplate::Tracks(list.compute(ctx)),
            GridTemplate::Subgrid(list) => GridTemplate::Subgrid(list.clone()),
        }
    }
}

/// `grid-auto-columns` and `grid-auto-rows`: the sizes of implicit tracks, repeated.
pub struct GridAutoTracks;

impl Property for GridAutoTracks {
    type Specified = Arc<[TrackSize<SpecifiedLengthPercentage>]>;
    type Computed = Arc<[TrackSize<LengthPercentage>]>;

    fn to_css(computed: &Self::Computed) -> String {
        computed
            .iter()
            .map(track_size_css)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let mut sizes = vec![parse_track_size(input)?];
        while let Ok(size) = input.try_parse(parse_track_size) {
            sizes.push(size);
        }
        Ok(sizes.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|size| size.compute(ctx)).collect()
    }
}

/// `grid-auto-flow`: `[ row | column ] || dense`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridAutoFlow {
    /// Items fill columns rather than rows.
    pub column: bool,
    /// Items fill holes left earlier in the grid.
    pub dense: bool,
}

impl GridAutoFlow {
    /// Parses one keyword into `self`, as `grid` interleaves them with `auto-flow`.
    pub(crate) fn parse_keywords(input: &mut Parser<'_>) -> Result<Self> {
        let (mut axis, mut dense) = (None, false);
        loop {
            let parsed = input.try_parse(|input| {
                let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
                match ident.as_str() {
                    "row" if axis.is_none() => axis = Some(false),
                    "column" if axis.is_none() => axis = Some(true),
                    "dense" if !dense => dense = true,
                    _ => return fail(),
                }
                Ok::<_, ParseError<()>>(())
            });
            if parsed.is_err() {
                break;
            }
        }
        if axis.is_none() && !dense {
            return fail();
        }
        Ok(GridAutoFlow {
            column: axis.unwrap_or(false),
            dense,
        })
    }
}

impl Property for GridAutoFlow {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let axis = if computed.column { "column" } else { "row" };
        if computed.dense {
            format!("{axis} dense")
        } else {
            axis.into()
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        GridAutoFlow::parse_keywords(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `<grid-line>`: where an item starts or ends along an axis.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GridLine {
    #[default]
    Auto,
    Line {
        /// The line number, or with `span` the number of lines to span; `None` when
        /// only a name is given.
        integer: Option<i32>,
        name: Option<Arc<str>>,
        span: bool,
    },
}

impl GridLine {
    /// Whether the value is just a name, which `grid-row` and similar copy to the end.
    pub fn is_ident(&self) -> bool {
        matches!(
            self,
            GridLine::Line {
                integer: None,
                name: Some(_),
                span: false
            }
        )
    }
}

impl Property for GridLine {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            GridLine::Auto => "auto".into(),
            GridLine::Line {
                integer,
                name,
                span,
            } => {
                let mut words = Vec::new();
                if *span {
                    words.push("span".to_owned());
                }
                if let Some(integer) = integer {
                    words.push(integer.to_string());
                }
                if let Some(name) = name {
                    let mut out = String::new();
                    let _ = cssparser::serialize_identifier(name, &mut out);
                    words.push(out);
                }
                words.join(" ")
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            return Ok(GridLine::Auto);
        }
        let (mut span, mut integer, mut name) = (false, None, None);
        loop {
            if !span
                && input
                    .try_parse(|input| input.expect_ident_matching("span"))
                    .is_ok()
            {
                span = true;
            } else if integer.is_none()
                && let Ok(value) = input.try_parse(|input| input.expect_integer())
            {
                integer = Some(value);
            } else if name.is_none()
                && let Ok(value) = input.try_parse(parse_custom_ident)
            {
                name = Some(value);
            } else {
                break;
            }
        }
        match (span, integer, &name) {
            (_, Some(0), _) => fail(),
            (true, Some(n), _) if n < 0 => fail(),
            (_, None, None) => fail(),
            _ => Ok(GridLine::Line {
                integer,
                name,
                span,
            }),
        }
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// A named area of `grid-template-areas`: the rows and columns it covers, from zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedArea {
    pub name: Arc<str>,
    pub rows: std::ops::Range<usize>,
    pub columns: std::ops::Range<usize>,
}

/// `grid-template-areas`: `none | <string>+`, with each named area a rectangle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GridAreas {
    /// The rows, each a list of cell tokens; `.` for a cell without an area.
    pub rows: Arc<[Vec<Arc<str>>]>,
    pub areas: Arc<[NamedArea]>,
}

impl GridAreas {
    pub fn columns(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    /// The cell tokens of one row string: names, and runs of `.` as null cells.
    fn tokens(row: &str) -> Option<Vec<Arc<str>>> {
        let mut tokens = Vec::new();
        let mut chars = row.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_whitespace() {
                chars.next();
            } else if c == '.' {
                while chars.peek() == Some(&'.') {
                    chars.next();
                }
                tokens.push(Arc::from("."));
            } else if c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii() {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii() {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Arc::from(name));
            } else {
                // A trash token makes the declaration invalid.
                return None;
            }
        }
        Some(tokens)
    }

    /// The areas of rows of cell tokens, or `None` if a row has a different number of
    /// cells or an area is not a rectangle.
    pub(crate) fn from_rows(rows: Vec<Vec<Arc<str>>>) -> Option<Self> {
        let columns = rows.first()?.len();
        if columns == 0 || rows.iter().any(|row| row.len() != columns) {
            return None;
        }
        let mut areas: Vec<NamedArea> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for (c, name) in row.iter().enumerate() {
                if &**name == "." || areas.iter().any(|a| a.name == *name) {
                    continue;
                }
                let mut end_c = c;
                while end_c < columns && row[end_c] == *name {
                    end_c += 1;
                }
                let mut end_r = r;
                while end_r < rows.len() && rows[end_r][c] == *name {
                    end_r += 1;
                }
                // Every cell inside is the area's, and none outside.
                for (rr, other) in rows.iter().enumerate() {
                    for (cc, cell) in other.iter().enumerate() {
                        let inside = (r..end_r).contains(&rr) && (c..end_c).contains(&cc);
                        if inside != (cell == name) {
                            return None;
                        }
                    }
                }
                areas.push(NamedArea {
                    name: name.clone(),
                    rows: r..end_r,
                    columns: c..end_c,
                });
            }
        }
        Some(GridAreas {
            rows: rows.into(),
            areas: areas.into(),
        })
    }

    /// Parses one `<string>` of the value into its cell tokens.
    pub(crate) fn parse_row(input: &mut Parser<'_>) -> Result<Vec<Arc<str>>> {
        let row = input.expect_string_cloned()?;
        GridAreas::tokens(&row)
            .filter(|tokens| !tokens.is_empty())
            .ok_or_else(|| ParseError::custom(()))
    }
}

impl Property for GridAreas {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        if computed.rows.is_empty() {
            return "none".into();
        }
        computed
            .rows
            .iter()
            .map(|row| {
                let text: Vec<&str> = row.iter().map(|t| &**t).collect();
                let mut out = String::new();
                let _ = cssparser::serialize_string(&text.join(" "), &mut out);
                out
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(GridAreas::default());
        }
        let mut rows = vec![GridAreas::parse_row(input)?];
        while let Ok(row) = input.try_parse(GridAreas::parse_row) {
            rows.push(row);
        }
        GridAreas::from_rows(rows).ok_or_else(|| ParseError::custom(()))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}
