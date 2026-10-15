//! Tables (CSS 2.1 § 17, CSS Tables 3): the table box tree with its anonymous boxes, the
//! grid of cells, collapsed borders, column widths, row heights and breaking rows across
//! pages with repeated headers and footers.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignmentBaseline, BaselineShift, BorderCollapse, BorderStyle, BoxDecorationBreak, BoxSizing,
    BreakBetween, CaptionSide, Direction, Display, DisplayInternal, EmptyCells, ImageValue,
    LengthPercentage, MaybeAuto, Size, TableLayout, Visibility,
};
use rustyprint_css::values::{AbsoluteColor, Color as CssColor};
use rustyprint_dom::NodeId;

use crate::boxes::{BlockBox, BlockContents};
use crate::display::Rect;
use crate::flow::{
    self, BlockKind, BreakToken, CollapsibleMargin, ContainingBlock, Fragmentainer, Laid, Resume,
};
use crate::fragment::{BoxFragment, Fragment, Sides};

/// The most columns a cell spans, and the most rows (HTML § 4.9.11).
const MAX_COLSPAN: u32 = 1000;
const MAX_ROWSPAN: u32 = 65534;

/// Slack for comparing positions with the end of the page.
const EPSILON: f32 = 0.01;

/// The grid and captions of a table box.
#[derive(Debug, Clone)]
pub struct Table {
    /// The style of the table grid box: the element's, but with collapsed borders its
    /// borders are half the outer collapsed borders and it has no padding.
    pub style: Arc<ComputedValues>,
    pub captions: Vec<BlockBox>,
    /// One per column of the grid.
    pub columns: Vec<Column>,
    pub column_groups: Vec<ColumnGroup>,
    /// Row groups in layout order: the header group first, the footer group last.
    pub groups: Vec<RowGroup>,
    /// The outer halves of collapsed borders, which the table draws per cell.
    pub outer: Option<OuterEdges>,
    /// The layout at the last width, which every page of the table shares.
    pub cache: LayoutCache,
}

/// A column of the grid, with the `table-column` box that made it, if any.
#[derive(Debug, Clone)]
pub struct Column {
    pub node: Option<NodeId>,
    pub style: Option<Arc<ComputedValues>>,
    /// The column group it belongs to.
    pub group: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ColumnGroup {
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    pub columns: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// The table's first `table-header-group`, repeated at the top of every page.
    Header,
    Body,
    /// The table's first `table-footer-group`, repeated at the bottom of every page.
    Footer,
}

#[derive(Debug, Clone)]
pub struct RowGroup {
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    pub kind: GroupKind,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub block: BlockBox,
    /// The first column it covers.
    pub column: usize,
    pub colspan: usize,
    pub rowspan: usize,
}

/// The table-internal display type of a box, if it has one.
pub(crate) fn part(display: Display) -> Option<DisplayInternal> {
    match display {
        Display::Internal(internal)
            if !matches!(
                internal,
                DisplayInternal::RubyBase
                    | DisplayInternal::RubyText
                    | DisplayInternal::RubyBaseContainer
                    | DisplayInternal::RubyTextContainer
            ) =>
        {
            Some(internal)
        }
        _ => None,
    }
}

/// Reads the `span`, `colspan` or `rowspan` attribute of an element.
pub(crate) type Spans<'a> = &'a dyn Fn(NodeId, &str) -> Option<u32>;

/// A copy of a style without borders, for table parts whose borders do not apply.
fn without_borders(style: &Arc<ComputedValues>) -> Arc<ComputedValues> {
    let widths = [
        style.border_top_width,
        style.border_right_width,
        style.border_bottom_width,
        style.border_left_width,
    ];
    if widths == [0.0; 4] {
        return style.clone();
    }
    let mut style = ComputedValues::clone(style);
    style.border_top_width = 0.0;
    style.border_right_width = 0.0;
    style.border_bottom_width = 0.0;
    style.border_left_width = 0.0;
    Arc::new(style)
}

/// An anonymous box inheriting from `parent`.
fn anonymous(parent: &Arc<ComputedValues>, contents: Vec<BlockBox>) -> BlockBox {
    BlockBox {
        node: None,
        style: Arc::new(ComputedValues::inherit_from(parent)),
        contents: BlockContents::Blocks(contents),
        page: None,
        marker: None,
        first_line: Vec::new(),
        legend: None,
        center_contents: false,
        shape_image: None,
        replaced: None,
    }
}

/// Builds a table from the boxes of a table element's children, generating the anonymous
/// rows, cells and row groups that misparented boxes need (CSS 2.1 § 17.2.1). `children`
/// gives the block-level children of a box.
pub(crate) fn build(
    style: &Arc<ComputedValues>,
    children: Vec<BlockBox>,
    take: &dyn Fn(BlockBox) -> (BlockBox, Vec<BlockBox>),
    spans: Spans<'_>,
) -> Table {
    let mut captions = Vec::new();
    let mut columns = Vec::new();
    let mut column_groups = Vec::new();
    let mut header = None;
    let mut footer = None;
    let mut groups: Vec<RowGroup> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut loose: Vec<BlockBox> = Vec::new();

    let flush_loose = |loose: &mut Vec<BlockBox>, rows: &mut Vec<Row>| {
        if !loose.is_empty() {
            let row = anonymous(style, Vec::new());
            rows.push(row_of(row, std::mem::take(loose)));
        }
    };
    let flush_rows = |rows: &mut Vec<Row>, groups: &mut Vec<RowGroup>| {
        if !rows.is_empty() {
            groups.push(RowGroup {
                node: None,
                style: Arc::new(ComputedValues::inherit_from(style)),
                kind: GroupKind::Body,
                rows: std::mem::take(rows),
            });
        }
    };

    for child in children {
        match part(child.style.display) {
            Some(DisplayInternal::TableCaption) => captions.push(child),
            Some(DisplayInternal::TableColumn) => {
                let span = child.node.and_then(|n| spans(n, "span")).unwrap_or(1);
                for _ in 0..span.clamp(1, MAX_COLSPAN) {
                    columns.push(Column {
                        node: child.node,
                        style: Some(without_borders(&child.style)),
                        group: None,
                    });
                }
            }
            Some(DisplayInternal::TableColumnGroup) => {
                let (group, members) = take(child);
                let index = column_groups.len();
                let start = columns.len();
                for member in members {
                    if part(member.style.display) != Some(DisplayInternal::TableColumn) {
                        continue;
                    }
                    let span = member.node.and_then(|n| spans(n, "span")).unwrap_or(1);
                    for _ in 0..span.clamp(1, MAX_COLSPAN) {
                        columns.push(Column {
                            node: member.node,
                            style: Some(without_borders(&member.style)),
                            group: Some(index),
                        });
                    }
                }
                // A group without columns has `span` columns of its own.
                if columns.len() == start {
                    let span = group.node.and_then(|n| spans(n, "span")).unwrap_or(1);
                    for _ in 0..span.clamp(1, MAX_COLSPAN) {
                        columns.push(Column {
                            node: None,
                            style: None,
                            group: Some(index),
                        });
                    }
                }
                column_groups.push(ColumnGroup {
                    node: group.node,
                    style: without_borders(&group.style),
                    columns: start..columns.len(),
                });
            }
            Some(
                kind @ (DisplayInternal::TableRowGroup
                | DisplayInternal::TableHeaderGroup
                | DisplayInternal::TableFooterGroup),
            ) => {
                flush_loose(&mut loose, &mut rows);
                flush_rows(&mut rows, &mut groups);
                let (group, members) = take(child);
                let mut group_rows = Vec::new();
                let mut group_loose = Vec::new();
                for member in members {
                    if part(member.style.display) == Some(DisplayInternal::TableRow) {
                        if !group_loose.is_empty() {
                            let row = anonymous(&group.style, Vec::new());
                            group_rows.push(row_of(row, std::mem::take(&mut group_loose)));
                        }
                        let (row, cells) = take(member);
                        group_rows.push(row_of(row, cells));
                    } else {
                        group_loose.push(member);
                    }
                }
                if !group_loose.is_empty() {
                    let row = anonymous(&group.style, Vec::new());
                    group_rows.push(row_of(row, group_loose));
                }
                let kind = match kind {
                    DisplayInternal::TableHeaderGroup if header.is_none() => {
                        header = Some(groups.len());
                        GroupKind::Header
                    }
                    DisplayInternal::TableFooterGroup if footer.is_none() => {
                        footer = Some(groups.len());
                        GroupKind::Footer
                    }
                    _ => GroupKind::Body,
                };
                groups.push(RowGroup {
                    node: group.node,
                    style: without_borders(&group.style),
                    kind,
                    rows: group_rows,
                });
            }
            Some(DisplayInternal::TableRow) => {
                flush_loose(&mut loose, &mut rows);
                let (row, cells) = take(child);
                rows.push(row_of(row, cells));
            }
            _ => loose.push(child),
        }
    }
    flush_loose(&mut loose, &mut rows);
    flush_rows(&mut rows, &mut groups);

    // The header group goes first and the footer group last.
    let mut ordered = Vec::with_capacity(groups.len());
    let mut middle = Vec::new();
    let mut last = None;
    for group in groups {
        match group.kind {
            GroupKind::Header => ordered.insert(0, group),
            GroupKind::Footer => last = Some(group),
            GroupKind::Body => middle.push(group),
        }
    }
    let header_first = ordered.len();
    ordered.extend(middle);
    ordered.extend(last);
    debug_assert!(header_first <= 1);

    let mut table = Table {
        style: style.clone(),
        captions,
        columns,
        column_groups,
        groups: ordered,
        outer: None,
        cache: LayoutCache::default(),
    };
    place_cells(&mut table, spans);
    if style.border_collapse == BorderCollapse::Collapse {
        collapse_borders(&mut table);
    }
    table
}

/// A row from its box and children: cells, with other boxes wrapped in anonymous cells.
fn row_of(row: BlockBox, children: Vec<BlockBox>) -> Row {
    let mut cells = Vec::new();
    let mut loose = Vec::new();
    let cell = |block: BlockBox| Cell {
        block,
        column: 0,
        colspan: 1,
        rowspan: 1,
    };
    for child in children {
        if part(child.style.display) == Some(DisplayInternal::TableCell) {
            if !loose.is_empty() {
                cells.push(cell(anonymous(&row.style, std::mem::take(&mut loose))));
            }
            cells.push(cell(child));
        } else {
            loose.push(child);
        }
    }
    if !loose.is_empty() {
        cells.push(cell(anonymous(&row.style, loose)));
    }
    Row {
        node: row.node,
        style: without_borders(&row.style),
        cells,
    }
}

/// Gives each cell its columns and spans, skipping slots that cells from rows above
/// cover (HTML § 4.9.12). Row spans stay inside their row group.
fn place_cells(table: &mut Table, spans: Spans<'_>) {
    let mut count = table.columns.len();
    for group in &mut table.groups {
        let rows = group.rows.len();
        // How many more rows, including the current one, each column is covered for.
        let mut covered: Vec<usize> = Vec::new();
        for (index, row) in group.rows.iter_mut().enumerate() {
            let left = rows - index;
            let mut column = 0;
            for cell in &mut row.cells {
                while covered.get(column).is_some_and(|&c| c > 0) {
                    column += 1;
                }
                let span = |name: &str| cell.block.node.and_then(|n| spans(n, name));
                let colspan = span("colspan").unwrap_or(1).clamp(1, MAX_COLSPAN) as usize;
                let rowspan = match span("rowspan").map(|r| r.min(MAX_ROWSPAN)) {
                    Some(0) => left,
                    Some(r) => (r as usize).min(left),
                    None => 1,
                };
                cell.column = column;
                cell.colspan = colspan;
                cell.rowspan = rowspan;
                if covered.len() < column + colspan {
                    covered.resize(column + colspan, 0);
                }
                for c in &mut covered[column..column + colspan] {
                    *c = rowspan;
                }
                column += colspan;
            }
            count = count.max(covered.len());
            for c in &mut covered {
                *c = c.saturating_sub(1);
            }
        }
    }
    while table.columns.len() < count {
        table.columns.push(Column {
            node: None,
            style: None,
            group: None,
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Collapsed borders

/// A border competing for an edge between cells.
#[derive(Debug, Clone, Copy)]
struct Edge {
    width: f32,
    style: BorderStyle,
    color: AbsoluteColor,
    /// Where it comes from: cells win over rows, row groups, columns, column groups and the
    /// table, in that order.
    rank: u8,
}

impl Edge {
    const NONE: Edge = Edge {
        width: 0.0,
        style: BorderStyle::None,
        color: AbsoluteColor::TRANSPARENT,
        rank: 0,
    };

    /// Side 0 is the top, then clockwise.
    fn of(style: &ComputedValues, side: usize, rank: u8) -> Edge {
        let (width, border, color) = match side {
            0 => (
                style.border_top_width,
                style.border_top_style,
                &style.border_top_color,
            ),
            1 => (
                style.border_right_width,
                style.border_right_style,
                &style.border_right_color,
            ),
            2 => (
                style.border_bottom_width,
                style.border_bottom_style,
                &style.border_bottom_color,
            ),
            _ => (
                style.border_left_width,
                style.border_left_style,
                &style.border_left_color,
            ),
        };
        let visible = !matches!(border, BorderStyle::None | BorderStyle::Hidden);
        Edge {
            width: if visible { width } else { 0.0 },
            style: border,
            color: color.resolve(&style.color),
            rank,
        }
    }

    /// Whether this border wins over `other` (CSS 2.1 § 17.6.2.1). Ties go to `other`, the
    /// one seen first, which is further up or left.
    fn beats(&self, other: &Edge) -> bool {
        if other.style == BorderStyle::Hidden {
            return false;
        }
        if self.style == BorderStyle::Hidden {
            return true;
        }
        let priority = |s: BorderStyle| match s {
            BorderStyle::Double => 8,
            BorderStyle::Solid => 7,
            BorderStyle::Dashed => 6,
            BorderStyle::Dotted => 5,
            BorderStyle::Ridge => 4,
            BorderStyle::Outset => 3,
            BorderStyle::Groove => 2,
            BorderStyle::Inset => 1,
            BorderStyle::None | BorderStyle::Hidden => 0,
        };
        if self.width != other.width {
            return self.width > other.width;
        }
        if priority(self.style) != priority(other.style) {
            return priority(self.style) > priority(other.style);
        }
        self.rank > other.rank
    }

    fn winner(edges: impl IntoIterator<Item = Edge>) -> Edge {
        let mut best: Option<Edge> = None;
        for edge in edges {
            best = Some(match best {
                Some(best) if !edge.beats(&best) => best,
                _ => edge,
            });
        }
        match best {
            Some(edge) if edge.style != BorderStyle::Hidden && edge.width > 0.0 => edge,
            _ => Edge::NONE,
        }
    }
}

const CELL: u8 = 5;
const ROW: u8 = 4;
const ROW_GROUP: u8 = 3;
const COLUMN: u8 = 2;
const COLUMN_GROUP: u8 = 1;
const TABLE: u8 = 0;

/// The cell covering each slot of the grid, as (group, row, cell), by grid row.
fn slots(table: &Table) -> Vec<Vec<Option<(usize, usize, usize)>>> {
    let columns = table.columns.len();
    let total: usize = table.groups.iter().map(|g| g.rows.len()).sum();
    let mut grid = vec![vec![None; columns]; total];
    let mut first = 0;
    for (g, group) in table.groups.iter().enumerate() {
        for (r, row) in group.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                for slot_row in &mut grid[first + r..first + r + cell.rowspan] {
                    for slot in &mut slot_row[cell.column..cell.column + cell.colspan] {
                        *slot = Some((g, r, c));
                    }
                }
            }
        }
        first += group.rows.len();
    }
    grid
}

/// Resolves the collapsed borders (CSS 2.1 § 17.6.2) into the styles of the cells, each
/// with half of every border around it, and of the table, with half of the outer borders.
fn collapse_borders(table: &mut Table) {
    let grid = slots(table);
    let rows = grid.len();
    let columns = table.columns.len();
    if columns == 0 {
        return;
    }
    // The group and row of each grid row, and whether it starts or ends its group.
    let mut row_of = Vec::with_capacity(rows);
    for (g, group) in table.groups.iter().enumerate() {
        for r in 0..group.rows.len() {
            row_of.push((g, r, r == 0, r + 1 == group.rows.len()));
        }
    }
    let cell_style = |slot: (usize, usize, usize)| -> &Arc<ComputedValues> {
        &table.groups[slot.0].rows[slot.1].cells[slot.2].block.style
    };
    let column_edges = |c: usize, side: usize, out: &mut Vec<Edge>| {
        let column = &table.columns[c];
        if let Some(style) = &column.style {
            out.push(Edge::of(style, side, COLUMN));
        }
        if let Some(group) = column.group {
            let group = &table.column_groups[group];
            let at_edge = match side {
                1 => c + 1 == group.columns.end,
                3 => c == group.columns.start,
                _ => true,
            };
            if at_edge {
                out.push(Edge::of(&group.style, side, COLUMN_GROUP));
            }
        }
    };

    // Horizontal edges above grid row `r`, and vertical edges left of column `c`.
    let mut horizontal = vec![vec![Edge::NONE; columns]; rows + 1];
    let mut vertical = vec![vec![Edge::NONE; columns + 1]; rows];
    for (r, line) in horizontal.iter_mut().enumerate() {
        for (c, edge) in line.iter_mut().enumerate() {
            let above = r.checked_sub(1).and_then(|r| grid[r][c]);
            let below = grid.get(r).and_then(|row| row[c]);
            if above.is_some() && above == below {
                continue;
            }
            let mut edges = Vec::new();
            if let Some(slot) = above {
                edges.push(Edge::of(cell_style(slot), 2, CELL));
            }
            if let Some(slot) = below {
                edges.push(Edge::of(cell_style(slot), 0, CELL));
            }
            if r > 0 {
                let (g, gr, _, last) = row_of[r - 1];
                edges.push(Edge::of(&table.groups[g].rows[gr].style, 2, ROW));
                if last {
                    edges.push(Edge::of(&table.groups[g].style, 2, ROW_GROUP));
                }
            }
            if r < rows {
                let (g, gr, first, _) = row_of[r];
                edges.push(Edge::of(&table.groups[g].rows[gr].style, 0, ROW));
                if first {
                    edges.push(Edge::of(&table.groups[g].style, 0, ROW_GROUP));
                }
            }
            if r == 0 {
                column_edges(c, 0, &mut edges);
                edges.push(Edge::of(&table.style, 0, TABLE));
            }
            if r == rows {
                column_edges(c, 2, &mut edges);
                edges.push(Edge::of(&table.style, 2, TABLE));
            }
            *edge = Edge::winner(edges);
        }
    }
    for (r, line) in vertical.iter_mut().enumerate() {
        let (g, gr, _, _) = row_of[r];
        for (c, edge) in line.iter_mut().enumerate() {
            let left = c.checked_sub(1).and_then(|c| grid[r][c]);
            let right = grid[r].get(c).copied().flatten();
            if left.is_some() && left == right {
                continue;
            }
            let mut edges = Vec::new();
            if let Some(slot) = left {
                edges.push(Edge::of(cell_style(slot), 1, CELL));
            }
            if let Some(slot) = right {
                edges.push(Edge::of(cell_style(slot), 3, CELL));
            }
            if c == 0 || c == columns {
                let side = if c == 0 { 3 } else { 1 };
                edges.push(Edge::of(&table.groups[g].rows[gr].style, side, ROW));
                edges.push(Edge::of(&table.groups[g].style, side, ROW_GROUP));
            }
            if c > 0 {
                column_edges(c - 1, 1, &mut edges);
            }
            if c < columns {
                column_edges(c, 3, &mut edges);
            }
            if c == 0 {
                edges.push(Edge::of(&table.style, 3, TABLE));
            }
            if c == columns {
                edges.push(Edge::of(&table.style, 1, TABLE));
            }
            *edge = Edge::winner(edges);
        }
    }

    let widest = |edges: &mut dyn Iterator<Item = Edge>| {
        edges.fold(Edge::NONE, |a, b| if b.width > a.width { b } else { a })
    };
    let mut first = 0;
    for group in &mut table.groups {
        for (r, row) in group.rows.iter_mut().enumerate() {
            let top = first + r;
            for cell in &mut row.cells {
                let columns = cell.column..cell.column + cell.colspan;
                let rows = top..top + cell.rowspan;
                let sides = [
                    widest(&mut columns.clone().map(|c| horizontal[rows.start][c])),
                    widest(&mut rows.clone().map(|r| vertical[r][columns.end])),
                    widest(&mut columns.clone().map(|c| horizontal[rows.end][c])),
                    widest(&mut rows.clone().map(|r| vertical[r][columns.start])),
                ];
                cell.block.style = Arc::new(with_half_borders(&cell.block.style, sides));
            }
            row.style = without_borders(&row.style);
        }
        group.style = without_borders(&group.style);
        first += group.rows.len();
    }
    if rows > 0 {
        let sides = [
            widest(&mut horizontal[0].iter().copied()),
            widest(&mut vertical.iter().map(|line| line[columns])),
            widest(&mut horizontal[rows].iter().copied()),
            widest(&mut vertical.iter().map(|line| line[0])),
        ];
        let mut style = with_half_borders(&table.style, sides);
        style.padding_top = LengthPercentage::ZERO;
        style.padding_right = LengthPercentage::ZERO;
        style.padding_bottom = LengthPercentage::ZERO;
        style.padding_left = LengthPercentage::ZERO;
        // The table's half of the outer borders differs along each side, so the edges
        // draw it segment by segment; the table's own borders only take up the space.
        style.border_top_style = BorderStyle::None;
        style.border_right_style = BorderStyle::None;
        style.border_bottom_style = BorderStyle::None;
        style.border_left_style = BorderStyle::None;
        table.style = Arc::new(style);
        let segment = |edge: &Edge| OuterSegment {
            width: edge.width / 2.0,
            style: edge.style,
            color: edge.color,
        };
        table.outer = Some(OuterEdges {
            top: horizontal[0].iter().map(segment).collect(),
            bottom: horizontal[rows].iter().map(segment).collect(),
            left: vertical.iter().map(|line| segment(&line[0])).collect(),
            right: vertical
                .iter()
                .map(|line| segment(&line[columns]))
                .collect(),
        });
    }
}

/// Half of a collapsed border on the outside of the table.
#[derive(Debug, Clone, Copy)]
pub struct OuterSegment {
    width: f32,
    style: BorderStyle,
    color: AbsoluteColor,
}

/// The outer halves of the collapsed borders: along the top and bottom by column, along
/// the sides by grid row.
#[derive(Debug, Clone)]
pub struct OuterEdges {
    top: Vec<OuterSegment>,
    bottom: Vec<OuterSegment>,
    left: Vec<OuterSegment>,
    right: Vec<OuterSegment>,
}

/// A fragment that draws one side of a border: `side` 0 is the top, then clockwise.
fn border_strip(
    table_style: &Arc<ComputedValues>,
    segment: &OuterSegment,
    side: usize,
    rect: Rect,
) -> Option<Fragment> {
    if segment.width <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let mut style = ComputedValues::inherit_from(table_style);
    let color = CssColor::Absolute(segment.color);
    let mut border = Sides::default();
    match side {
        0 => {
            style.border_top_style = segment.style;
            style.border_top_color = color;
            border.top = rect.height;
        }
        1 => {
            style.border_right_style = segment.style;
            style.border_right_color = color;
            border.right = rect.width;
        }
        2 => {
            style.border_bottom_style = segment.style;
            style.border_bottom_color = color;
            border.bottom = rect.height;
        }
        _ => {
            style.border_left_style = segment.style;
            style.border_left_color = color;
            border.left = rect.width;
        }
    }
    Some(Fragment::Box(BoxFragment {
        node: None,
        style: Arc::new(style),
        rect,
        border,
        padding: Sides::default(),
        children: Vec::new(),
        replaced: None,
        legend: None,
        out_of_flow: None,
    }))
}

/// A style with half of each collapsed border.
fn with_half_borders(style: &ComputedValues, sides: [Edge; 4]) -> ComputedValues {
    let mut style = style.clone();
    let [top, right, bottom, left] = sides;
    style.border_top_width = top.width / 2.0;
    style.border_top_style = top.style;
    style.border_top_color = CssColor::Absolute(top.color);
    style.border_right_width = right.width / 2.0;
    style.border_right_style = right.style;
    style.border_right_color = CssColor::Absolute(right.color);
    style.border_bottom_width = bottom.width / 2.0;
    style.border_bottom_style = bottom.style;
    style.border_bottom_color = CssColor::Absolute(bottom.color);
    style.border_left_width = left.width / 2.0;
    style.border_left_style = left.style;
    style.border_left_color = CssColor::Absolute(left.color);
    style
}

// ---------------------------------------------------------------------------------------------
// Column widths

/// A cell's border-box width from its `width`, or its percentage.
enum CellWidth {
    Auto,
    Fixed(f32),
    Percent(f32),
}

fn horizontal_extra(style: &ComputedValues) -> f32 {
    let fixed = |v: &LengthPercentage| v.as_length().unwrap_or(0.0);
    fixed(&style.padding_left)
        + fixed(&style.padding_right)
        + style.border_left_width
        + style.border_right_width
}

fn cell_width(style: &ComputedValues) -> CellWidth {
    match &style.width {
        Size::Value(LengthPercentage::Length(px)) => {
            CellWidth::Fixed(if style.box_sizing == BoxSizing::BorderBox {
                *px
            } else {
                px + horizontal_extra(style)
            })
        }
        Size::Value(LengthPercentage::Percentage(p)) => CellWidth::Percent(*p),
        _ => CellWidth::Auto,
    }
}

/// The minimum and maximum widths of each column, which have a percentage width if any.
struct Constraints {
    min: Vec<f32>,
    max: Vec<f32>,
    percent: Vec<Option<f32>>,
    /// Whether the column has a fixed width, so it only grows when every column does.
    fixed: Vec<bool>,
}

/// The cells of the table with their grid row, in grid order.
fn cells(table: &Table) -> Vec<(usize, &Row, &Cell)> {
    let mut out = Vec::new();
    let mut first = 0;
    for group in &table.groups {
        for (r, row) in group.rows.iter().enumerate() {
            for cell in &row.cells {
                out.push((first + r, row, cell));
            }
        }
        first += group.rows.len();
    }
    out
}

fn constraints(table: &Table, spacing: f32) -> Constraints {
    let n = table.columns.len();
    let mut c = Constraints {
        min: vec![0.0; n],
        max: vec![0.0; n],
        percent: vec![None; n],
        fixed: vec![false; n],
    };
    for (i, column) in table.columns.iter().enumerate() {
        let styles = column
            .style
            .iter()
            .chain(column.group.map(|g| &table.column_groups[g].style));
        for style in styles {
            match cell_width(style) {
                CellWidth::Fixed(w) if !c.fixed[i] => {
                    c.min[i] = c.min[i].max(w);
                    c.max[i] = c.max[i].max(w);
                    c.fixed[i] = true;
                }
                CellWidth::Percent(p) if c.percent[i].is_none() => c.percent[i] = Some(p),
                _ => {}
            }
        }
    }
    let mut cells = cells(table);
    cells.sort_by_key(|(_, _, cell)| cell.colspan);
    for (_, _, cell) in cells {
        let style = &cell.block.style;
        let (content_min, content_max) = flow::intrinsic_content(&cell.block);
        let extra = horizontal_extra(style);
        let (mut min, mut max) = (content_min + extra, content_max + extra);
        let mut fixed = false;
        match cell_width(style) {
            CellWidth::Fixed(w) => {
                min = min.max(w);
                max = min.max(w);
                fixed = true;
            }
            CellWidth::Percent(p) if cell.colspan == 1 => {
                let percent = &mut c.percent[cell.column];
                *percent = Some(percent.map_or(p, |q| q.max(p)));
            }
            _ => {}
        }
        let span = cell.column..cell.column + cell.colspan;
        if cell.colspan == 1 {
            let i = cell.column;
            c.min[i] = c.min[i].max(min);
            c.max[i] = c.max[i].max(max);
            c.fixed[i] |= fixed;
            continue;
        }
        // A spanning cell spreads what the columns lack, in proportion to their widths.
        let gaps = spacing * (cell.colspan - 1) as f32;
        let spread = |values: &mut [f32], weights: &[f32], wanted: f32| {
            let have: f32 = values.iter().sum::<f32>() + gaps;
            if wanted <= have {
                return;
            }
            let total: f32 = weights.iter().sum();
            let count = values.len() as f32;
            for (value, weight) in values.iter_mut().zip(weights) {
                let share = if total > 0.0 {
                    weight / total
                } else {
                    1.0 / count
                };
                *value += (wanted - have) * share;
            }
        };
        let weights: Vec<f32> = c.max[span.clone()].to_vec();
        spread(&mut c.min[span.clone()], &weights, min);
        spread(&mut c.max[span.clone()], &weights, max);
    }
    for i in 0..n {
        c.max[i] = c.max[i].max(c.min[i]);
    }
    c
}

/// The minimum and maximum widths of the grid, with the spacing around the columns.
fn grid_widths(table: &Table, c: &Constraints, spacing: f32) -> (f32, f32) {
    let gaps = spacing * (table.columns.len() + 1) as f32;
    let min: f32 = c.min.iter().sum();
    let mut max: f32 = c.max.iter().sum();
    // Percentage columns need the grid wide enough for their share.
    let percent: f32 = c.percent.iter().flatten().sum::<f32>().min(1.0);
    let others: f32 = (0..c.max.len())
        .filter(|&i| c.percent[i].is_none())
        .map(|i| c.max[i])
        .sum();
    for (i, p) in c.percent.iter().enumerate() {
        if let Some(p) = p.filter(|p| *p > 0.0) {
            max = max.max(c.max[i] / p);
        }
    }
    if percent < 1.0 && percent > 0.0 {
        max = max.max(others / (1.0 - percent));
    }
    (min + gaps, max.max(min) + gaps)
}

/// Minimum and maximum widths of a table's content box: its grid and captions.
pub(crate) fn intrinsic(table: &Table) -> (f32, f32) {
    let spacing = spacing(table).0;
    let c = constraints(table, spacing);
    let (mut min, mut max) = grid_widths(table, &c, spacing);
    for caption in &table.captions {
        let (caption_min, _) = flow::intrinsic_outer(caption);
        min = min.max(caption_min);
        max = max.max(caption_min);
    }
    (min, max)
}

/// The horizontal and vertical space between cells.
fn spacing(table: &Table) -> (f32, f32) {
    if table.style.border_collapse == BorderCollapse::Collapse {
        (0.0, 0.0)
    } else {
        table.style.border_spacing
    }
}

/// Distributes the grid's width, without spacing, over the columns (CSS 2.1 § 17.5.2.2).
fn distribute(c: &Constraints, width: f32) -> Vec<f32> {
    let n = c.min.len();
    let mut widths = c.min.clone();
    let min_total: f32 = c.min.iter().sum();
    if n == 0 || width <= min_total {
        return widths;
    }
    // Percentage columns take their share first, as far as the others' minimums allow.
    let mut left = width - min_total;
    for ((width_i, percent), min) in widths.iter_mut().zip(&c.percent).zip(&c.min) {
        if let Some(p) = percent {
            let want = (p * width - min).max(0.0).min(left);
            *width_i += want;
            left -= want;
        }
    }
    let flexible: Vec<usize> = (0..n).filter(|&i| c.percent[i].is_none()).collect();
    if flexible.is_empty() {
        // Only percentage columns: they share what is left in proportion.
        let total: f32 = widths.iter().sum();
        for w in &mut widths {
            *w += if total > 0.0 {
                left * *w / total
            } else {
                left / n as f32
            };
        }
        return widths;
    }
    let grow: f32 = flexible.iter().map(|&i| c.max[i] - c.min[i]).sum();
    if left <= grow {
        // Between the minimum and maximum: every column moves the same fraction of the way.
        let t = if grow > 0.0 { left / grow } else { 0.0 };
        for &i in &flexible {
            widths[i] += (c.max[i] - c.min[i]) * t;
        }
        return widths;
    }
    for &i in &flexible {
        widths[i] = c.max[i];
    }
    left -= grow;
    // Wider than the maximum: auto columns grow in proportion to their widths, and fixed
    // ones only when there are no others.
    let auto: Vec<usize> = flexible.iter().copied().filter(|&i| !c.fixed[i]).collect();
    let growing = if auto.is_empty() { &flexible } else { &auto };
    let total: f32 = growing.iter().map(|&i| c.max[i]).sum();
    for &i in growing {
        widths[i] += if total > 0.0 {
            left * c.max[i] / total
        } else {
            left / growing.len() as f32
        };
    }
    widths
}

/// Column widths of the fixed table layout, from the columns and the first row's cells
/// (CSS 2.1 § 17.5.2.1), and the grid width they need.
fn fixed_widths(table: &Table, width: f32) -> Vec<f32> {
    let n = table.columns.len();
    let mut widths: Vec<Option<f32>> = vec![None; n];
    for (i, column) in table.columns.iter().enumerate() {
        if let Some(style) = &column.style {
            widths[i] = match cell_width(style) {
                CellWidth::Fixed(w) => Some(w),
                CellWidth::Percent(p) => Some(p * width),
                CellWidth::Auto => None,
            };
        }
    }
    if let Some(row) = table.groups.iter().flat_map(|g| g.rows.first()).next() {
        for cell in &row.cells {
            let span = cell.column..cell.column + cell.colspan;
            if widths[span.clone()].iter().any(Option::is_some) {
                continue;
            }
            let total = match cell_width(&cell.block.style) {
                CellWidth::Fixed(w) => w,
                CellWidth::Percent(p) => p * width,
                CellWidth::Auto => continue,
            };
            for w in &mut widths[span] {
                *w = Some(total / cell.colspan as f32);
            }
        }
    }
    let used: f32 = widths.iter().flatten().sum();
    let auto = widths.iter().filter(|w| w.is_none()).count();
    let share = if auto > 0 {
        ((width - used) / auto as f32).max(0.0)
    } else {
        0.0
    };
    let mut widths: Vec<f32> = widths.into_iter().map(|w| w.unwrap_or(share)).collect();
    // With no auto columns, the fixed ones share any width left.
    let total: f32 = widths.iter().sum();
    if auto == 0 && total < width && total > 0.0 {
        for w in &mut widths {
            *w *= width / total;
        }
    }
    widths
}

// ---------------------------------------------------------------------------------------------
// Layout

/// How a cell's content sits in its rows (CSS 2.1 § 17.5.4).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Align {
    Baseline,
    Top,
    Middle,
    Bottom,
}

fn align(style: &ComputedValues) -> Align {
    match style.baseline_shift {
        BaselineShift::Top => Align::Top,
        BaselineShift::Bottom => Align::Bottom,
        BaselineShift::Center => Align::Middle,
        _ => match style.alignment_baseline {
            AlignmentBaseline::Middle | AlignmentBaseline::Central => Align::Middle,
            _ => Align::Baseline,
        },
    }
}

/// A cell laid out at its width, before its rows are known.
#[derive(Debug)]
struct LaidCell {
    row: usize,
    column: usize,
    rowspan: usize,
    fragment: BoxFragment,
    /// Its content's first baseline from the top of its border box.
    baseline: f32,
    align: Align,
}

/// A grid row: its group, and whether `visibility: collapse` removes it.
#[derive(Debug)]
struct GridRow {
    group: usize,
    kind: GroupKind,
    collapsed: bool,
}

/// What a table's layout at one width needs on every page: column widths, the cells laid
/// out, row heights and captions. Pages only choose which rows they show.
#[derive(Debug)]
pub(crate) struct Prepared {
    /// The table's border-box width, and its margins with `auto` resolved.
    width: f32,
    margin_left: Option<f32>,
    margin_right: Option<f32>,
    widths: Vec<f32>,
    /// Where each column starts in the grid box, and where the last one ends.
    column_x: Vec<f32>,
    grid: Vec<GridRow>,
    /// Cells in grid order, and the range of them starting in each row.
    cells: Vec<LaidCell>,
    row_cells: Vec<Range<usize>>,
    heights: Vec<f32>,
    baselines: Vec<f32>,
    top_captions: Vec<(BoxFragment, Sides)>,
    bottom_captions: Vec<(BoxFragment, Sides)>,
}

/// The preparation of a table for the last width it was laid out at.
#[derive(Default)]
pub struct LayoutCache(Mutex<Option<(u32, usize, Arc<Prepared>)>>);

impl Clone for LayoutCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for LayoutCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LayoutCache")
    }
}

/// The part of a broken row on a page: its cells, its height, and where cells broke again.
type RowPart = (Vec<BoxFragment>, f32, Option<Vec<Option<BreakToken>>>);

/// A row placed on this page.
struct PlacedRow {
    row: usize,
    y: f32,
}

/// Lays out the columns, cells and captions of a table at the width of `cb`.
fn prepare(
    block: &BlockBox,
    table: &Table,
    cb: ContainingBlock,
    kind: BlockKind,
    decorators: &[Arc<ComputedValues>],
) -> Prepared {
    let outer = &block.style;
    let style = &table.style;
    let (padding, border) = flow::padding_and_border(style, cb.width);
    let extra = padding.horizontal() + border.horizontal();
    let (hs, vs) = spacing(table);
    let n = table.columns.len();

    // Margins and the table's width.
    let margin = |m: &MaybeAuto<LengthPercentage>| match m {
        MaybeAuto::Auto => None,
        MaybeAuto::Value(v) => Some(v.resolve(cb.width)),
    };
    let mut margin_left = margin(&outer.margin_left);
    let mut margin_right = margin(&outer.margin_right);
    let border_box = |w: f32| {
        if outer.box_sizing == BoxSizing::BorderBox {
            w
        } else {
            w + extra
        }
    };
    let specified = match (&outer.width, kind) {
        // A flex item is as wide as its container makes it.
        (_, BlockKind::Item { width, .. }) => Some(width + extra),
        (Size::Value(v), _) => Some(border_box(v.resolve(cb.width))),
        _ => None,
    };
    let c = constraints(table, hs);
    let (grid_min, grid_max) = grid_widths(table, &c, hs);
    let fixed = outer.table_layout == TableLayout::Fixed && specified.is_some();
    let (min_width, max_width) = if fixed {
        (extra, extra)
    } else {
        (grid_min + extra, grid_max + extra)
    };
    let captions_min = table
        .captions
        .iter()
        .map(|caption| flow::intrinsic_outer(caption).0)
        .fold(0.0, f32::max);
    let available = (cb.width - margin_left.unwrap_or(0.0) - margin_right.unwrap_or(0.0)).max(0.0);
    let mut width = match specified {
        Some(w) => w.max(min_width),
        None => max_width.min(available).max(min_width),
    };
    if let Size::Value(v) = &outer.max_width {
        width = width.min(border_box(v.resolve(cb.width))).max(min_width);
    }
    if let Size::Value(v) = &outer.min_width {
        width = width.max(border_box(v.resolve(cb.width)));
    }
    width = width.max(captions_min);
    let grid_space = (width - extra - hs * (n + 1) as f32).max(0.0);
    let mut widths = if fixed {
        fixed_widths(table, grid_space)
    } else {
        distribute(&c, grid_space)
    };
    // Collapsed columns take no space.
    for (i, column) in table.columns.iter().enumerate() {
        if column
            .style
            .as_ref()
            .is_some_and(|s| s.visibility == Visibility::Collapse)
        {
            widths[i] = 0.0;
        }
    }
    let used: f32 = widths.iter().sum::<f32>() + hs * (n + 1) as f32 + extra;
    width = width.max(used);

    if !matches!(kind, BlockKind::Atomic | BlockKind::Item { .. }) {
        let remaining = cb.width - width;
        match (margin_left, margin_right) {
            (None, None) => {
                margin_left = Some((remaining / 2.0).max(0.0));
                margin_right = Some((remaining / 2.0).max(0.0));
            }
            (None, Some(right)) => margin_left = Some(remaining - right),
            (Some(left), None) => margin_right = Some(remaining - left),
            _ => {}
        }
    }

    // Column positions in the grid box.
    let content_x = border.left + padding.left;
    let mut column_x = Vec::with_capacity(n + 1);
    let mut x = content_x + hs;
    for w in &widths {
        column_x.push(x);
        x += w + hs;
    }
    column_x.push(if n == 0 { content_x } else { x - hs });
    let span_width = |cell: &Cell| {
        widths[cell.column..cell.column + cell.colspan]
            .iter()
            .sum::<f32>()
            + hs * (cell.colspan - 1) as f32
    };

    // The grid rows.
    let mut grid: Vec<GridRow> = Vec::new();
    for (g, group) in table.groups.iter().enumerate() {
        for row in &group.rows {
            grid.push(GridRow {
                group: g,
                kind: group.kind,
                collapsed: row.style.visibility == Visibility::Collapse
                    || group.style.visibility == Visibility::Collapse,
            });
        }
    }
    let rows = grid.len();

    // Cells, laid out at their widths.
    let mut own_decorators = decorators.to_vec();
    if !outer.text_decoration_line.is_none() {
        own_decorators.push(outer.clone());
    }
    let mut cells: Vec<LaidCell> = Vec::new();
    let mut row_cells = Vec::with_capacity(rows);
    let mut first = 0;
    for group in &table.groups {
        for (r, row) in group.rows.iter().enumerate() {
            let start = cells.len();
            if !grid[first + r].collapsed {
                for cell in &row.cells {
                    cells.push(lay_out_cell(
                        cell,
                        first + r,
                        span_width(cell),
                        style,
                        &own_decorators,
                    ));
                }
            }
            row_cells.push(start..cells.len());
        }
        first += group.rows.len();
    }

    // Row heights: CSS 2.1 § 17.5.3.
    let mut heights = vec![0.0f32; rows];
    let mut baselines = vec![0.0f32; rows];
    let mut index = 0;
    for group in &table.groups {
        for row in &group.rows {
            if let Size::Value(LengthPercentage::Length(h)) = &row.style.height {
                heights[index] = *h;
            }
            index += 1;
        }
    }
    for cell in &cells {
        if cell.align == Align::Baseline {
            baselines[cell.row] = baselines[cell.row].max(cell.baseline);
        }
    }
    let offset = |cell: &LaidCell| {
        if cell.align == Align::Baseline {
            baselines[cell.row] - cell.baseline
        } else {
            0.0
        }
    };
    for cell in cells.iter().filter(|c| c.rowspan == 1) {
        heights[cell.row] = heights[cell.row].max(offset(cell) + cell.fragment.rect.height);
    }
    for cell in cells.iter().filter(|c| c.rowspan > 1) {
        let span = cell.row..cell.row + cell.rowspan;
        let have = heights[span.clone()].iter().sum::<f32>() + vs * (cell.rowspan - 1) as f32;
        let need = offset(cell) + cell.fragment.rect.height;
        if need > have {
            heights[span.end - 1] += need - have;
        }
    }
    for (r, grid_row) in grid.iter().enumerate() {
        if grid_row.collapsed {
            heights[r] = 0.0;
        }
    }
    // A taller table gives its body rows the extra height, in proportion.
    if let Size::Value(LengthPercentage::Length(h)) = &outer.height {
        let content = if outer.box_sizing == BoxSizing::BorderBox {
            h - padding.vertical() - border.vertical()
        } else {
            *h
        };
        let have = heights.iter().sum::<f32>() + vs * (rows + 1) as f32;
        let body: Vec<usize> = (0..rows)
            .filter(|&r| grid[r].kind == GroupKind::Body && !grid[r].collapsed)
            .collect();
        if content > have && !body.is_empty() {
            let total: f32 = body.iter().map(|&r| heights[r]).sum();
            for &r in &body {
                heights[r] += if total > 0.0 {
                    (content - have) * heights[r] / total
                } else {
                    (content - have) / body.len() as f32
                };
            }
        }
    }

    // Captions, at the table's width.
    let caption_cb = ContainingBlock {
        width,
        height: None,
        rtl: cb.rtl,
    };
    let mut top_captions = Vec::new();
    let mut bottom_captions = Vec::new();
    for caption in &table.captions {
        let laid = flow::layout_block(
            caption,
            caption_cb,
            BlockKind::InFlow,
            &own_decorators,
            None,
            None,
        );
        let laid = (laid.fragment, laid.margin);
        if caption.style.caption_side == CaptionSide::Bottom {
            bottom_captions.push(laid);
        } else {
            top_captions.push(laid);
        }
    }

    Prepared {
        width,
        margin_left,
        margin_right,
        widths,
        column_x,
        grid,
        cells,
        row_cells,
        heights,
        baselines,
        top_captions,
        bottom_captions,
    }
}

/// Lays out a cell at the width of its columns.
fn lay_out_cell(
    cell: &Cell,
    row: usize,
    width: f32,
    table_style: &ComputedValues,
    decorators: &[Arc<ComputedValues>],
) -> LaidCell {
    let cell_style = &cell.block.style;
    let cell_cb = ContainingBlock {
        width,
        height: None,
        rtl: cell_style.direction == Direction::Rtl,
    };
    let laid = flow::layout_block(
        &cell.block,
        cell_cb,
        BlockKind::Cell,
        decorators,
        None,
        None,
    );
    let mut fragment = laid.fragment;
    // A definite height is a minimum.
    if let Size::Value(LengthPercentage::Length(h)) = &cell_style.height {
        let h = if cell_style.box_sizing == BoxSizing::BorderBox {
            *h
        } else {
            h + fragment.padding.vertical() + fragment.border.vertical()
        };
        fragment.rect.height = fragment.rect.height.max(h);
    }
    let baseline = fragment
        .first_baseline()
        .unwrap_or(fragment.rect.height - fragment.padding.bottom - fragment.border.bottom);
    // `empty-cells: hide` draws nothing around cells without content.
    if cell_style.empty_cells == EmptyCells::Hide
        && table_style.border_collapse == BorderCollapse::Separate
        && fragment.children.is_empty()
    {
        let mut hidden = ComputedValues::clone(&fragment.style);
        hidden.background_color = CssColor::Absolute(AbsoluteColor::TRANSPARENT);
        hidden.background_image = Arc::from([ImageValue::None]);
        hidden.border_top_style = BorderStyle::None;
        hidden.border_right_style = BorderStyle::None;
        hidden.border_bottom_style = BorderStyle::None;
        hidden.border_left_style = BorderStyle::None;
        fragment.style = Arc::new(hidden);
    }
    LaidCell {
        row,
        column: cell.column,
        rowspan: cell.rowspan,
        align: align(cell_style),
        baseline,
        fragment,
    }
}

/// The preparation of a table at the width of `cb`, from the cache when the width is the
/// one it was last laid out at.
fn prepared(
    block: &BlockBox,
    table: &Table,
    cb: ContainingBlock,
    kind: BlockKind,
    decorators: &[Arc<ComputedValues>],
) -> Arc<Prepared> {
    let key = (cb.width.to_bits(), decorators.len());
    let mut cache = table.cache.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((width, count, prepared)) = &*cache
        && (*width, *count) == key
    {
        return prepared.clone();
    }
    let prepared = Arc::new(prepare(block, table, cb, kind, decorators));
    *cache = Some((key.0, key.1, prepared.clone()));
    prepared
}

/// Lays out a table box (CSS 2.1 § 17.5). The table is shrink-to-fit unless its width is
/// set; captions go above or below the grid. With a fragmentainer, rows that do not fit
/// move to the next page, which repeats the header and footer groups.
pub(crate) fn layout(
    block: &BlockBox,
    table: &Table,
    cb: ContainingBlock,
    kind: BlockKind,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> Laid {
    let outer = &block.style;
    let style = &table.style;
    let (padding, border) = flow::padding_and_border(style, cb.width);
    let extra = padding.horizontal() + border.horizontal();
    let (hs, vs) = spacing(table);
    let resumed = resume.is_some();
    let p = prepared(block, table, cb, kind, decorators);
    let width = p.width;
    let heights = &p.heights;
    let grid = &p.grid;
    let rows = grid.len();

    let margin = |m: &MaybeAuto<LengthPercentage>| match m {
        MaybeAuto::Auto => 0.0,
        MaybeAuto::Value(v) => v.resolve(cb.width),
    };
    let margin_top = if resumed {
        0.0
    } else {
        margin(&outer.margin_top)
    };
    let margin_bottom = margin(&outer.margin_bottom);

    let captions_height = |laid: &[(BoxFragment, Sides)]| -> f32 {
        laid.iter()
            .map(|(fragment, margin)| fragment.rect.height + margin.vertical())
            .sum()
    };
    let top_height = if resumed {
        0.0
    } else {
        captions_height(&p.top_captions)
    };
    let bottom_height = captions_height(&p.bottom_captions);

    // Which rows go on this page.
    let header: Vec<usize> = (0..rows)
        .filter(|&r| grid[r].kind == GroupKind::Header)
        .collect();
    let footer: Vec<usize> = (0..rows)
        .filter(|&r| grid[r].kind == GroupKind::Footer)
        .collect();
    let body: Vec<usize> = (0..rows)
        .filter(|&r| grid[r].kind == GroupKind::Body)
        .collect();
    let rows_height = |rows: &[usize]| rows.iter().map(|&r| heights[r] + vs).sum::<f32>();
    let (start, partial) = match resume.map(|t| &t.resume) {
        Some(Resume::Row { index }) => ((*index).min(body.len()), None),
        Some(Resume::RowPart { index, cells }) => ((*index).min(body.len()), Some(&cells[..])),
        _ => (0, None),
    };
    let mut own_decorators = decorators.to_vec();
    if !outer.text_decoration_line.is_none() {
        own_decorators.push(outer.clone());
    }
    let columns = fragmentainer.is_some_and(|f| f.columns);
    // A row broken across pages, laid out in `space` from where its cells broke before.
    let split_row = |r: usize, tokens: Option<&[Option<BreakToken>]>, space: f32| {
        let row = row_of_grid(table, r);
        let laid = &p.cells[p.row_cells[r].clone()];
        let mut fragments = Vec::new();
        let mut next = Vec::new();
        for (k, (cell, prepared)) in row.cells.iter().zip(laid).enumerate() {
            let token = tokens.map(|t| t.get(k).and_then(Option::as_ref));
            if token == Some(None) {
                // Finished on an earlier page: the cell continues without content.
                let mut fragment = prepared.fragment.clone();
                fragment.children.clear();
                fragment.rect.height = fragment.padding.vertical() + fragment.border.vertical();
                fragments.push(fragment);
                next.push(None);
                continue;
            }
            let width = p.widths[cell.column..cell.column + cell.colspan]
                .iter()
                .sum::<f32>()
                + hs * (cell.colspan - 1) as f32;
            let cell_cb = ContainingBlock {
                width,
                height: None,
                rtl: cell.block.style.direction == Direction::Rtl,
            };
            let laid = flow::layout_block(
                &cell.block,
                cell_cb,
                BlockKind::Cell,
                &own_decorators,
                Some(Fragmentainer {
                    available: space,
                    at_top: true,
                    columns,
                }),
                token.flatten(),
            );
            fragments.push(laid.fragment);
            next.push(laid.break_token);
        }
        let broke = next.iter().any(Option::is_some);
        let tallest = fragments.iter().map(|f| f.rect.height).fold(0.0, f32::max);
        let height = if broke { tallest.max(space) } else { tallest };
        (fragments, height, broke.then_some(next))
    };
    // Rows that a cell from above spans into, or with cells spanning rows, do not split.
    let splittable = |r: usize| {
        p.cells[p.row_cells[r].clone()]
            .iter()
            .all(|c| c.rowspan == 1)
            && !p.cells.iter().any(|c| c.row < r && c.row + c.rowspan > r)
    };
    // The part of a broken row on this page: its cells, height, and where they broke.
    let mut part: Option<RowPart> = None;
    let row_style = |r: usize| {
        let mut index = r;
        for group in &table.groups {
            if index < group.rows.len() {
                return &group.rows[index].style;
            }
            index -= group.rows.len();
        }
        &table.style
    };
    let avoid = if columns {
        outer.break_inside.avoids_column()
    } else {
        outer.break_inside.avoids_page()
    };
    let fragmentainer = fragmentainer.filter(|f| f.at_top || !avoid);
    let grid_content_top = border.top + padding.top;
    let fixed_bottom = vs + padding.bottom + border.bottom;
    let mut end = body.len();
    let mut forced = false;
    if let Some(f) = fragmentainer {
        let mut y = top_height + grid_content_top + vs + rows_height(&header);
        let tail = rows_height(&footer) + fixed_bottom;
        let mut placed = 0;
        for (i, &r) in body.iter().enumerate().skip(start) {
            if i > start {
                let before = row_style(body[i - 1]).break_after;
                let after = row_style(r).break_before;
                let forces = |b: BreakBetween| {
                    if columns {
                        b.forces_column()
                    } else {
                        b.forces_page()
                    }
                };
                if forces(before) || forces(after) {
                    end = i;
                    forced = true;
                    break;
                }
            }
            let last = i + 1 == body.len();
            let bottom = if last { bottom_height } else { 0.0 };
            let fits = y + heights[r] + vs + tail + bottom <= f.available + EPSILON;
            // The rest of a broken row, or a row too tall for a page of its own, breaks
            // inside its cells.
            if (i == start && partial.is_some())
                || (!fits && placed == 0 && f.at_top && splittable(r))
            {
                let space = (f.available - y - tail - vs).max(0.0);
                let (fragments, height, tokens) = split_row(r, partial, space);
                let broke = tokens.is_some();
                part = Some((fragments, height, tokens));
                y += height + vs;
                placed += 1;
                if broke {
                    end = i + 1;
                    break;
                }
                continue;
            }
            if !fits && (placed > 0 || !f.at_top) {
                if placed == 0 {
                    return flow::deferred(block, border, padding, width - extra, extra);
                }
                // A cell spanning the break takes its rows to the next page, where that
                // leaves a row on this one.
                let mut at = i;
                loop {
                    let moved = p
                        .cells
                        .iter()
                        .filter(|cell| cell.rowspan > 1)
                        .filter(|cell| cell.row < body[at] && cell.row + cell.rowspan > body[at])
                        .filter_map(|cell| body.iter().position(|&b| b == cell.row))
                        .filter(|&k| k > start)
                        .min();
                    match moved {
                        Some(k) if k < at => at = k,
                        _ => break,
                    }
                }
                end = at;
                break;
            }
            y += heights[r] + vs;
            placed += 1;
        }
    }
    if fragmentainer.is_none()
        && let Some(tokens) = partial
        && start < body.len()
    {
        part = Some(split_row(body[start], Some(tokens), f32::INFINITY));
    }
    if end == start && !forced && end < body.len() {
        end = start + 1;
    }
    let part_tokens = part.as_mut().and_then(|(_, _, tokens)| tokens.take());
    let broke = end < body.len() || part_tokens.is_some();
    // The broken row's height on this page replaces its own.
    let mut page_heights;
    let heights = if let Some((_, height, _)) = &part {
        page_heights = heights.clone();
        page_heights[body[start]] = *height;
        &page_heights
    } else {
        heights
    };

    // Positions of the rows on this page.
    let mut placed: Vec<PlacedRow> = Vec::new();
    let mut row_y: Vec<Option<f32>> = vec![None; rows];
    let mut y = grid_content_top + vs;
    for &r in header.iter().chain(&body[start..end]).chain(footer.iter()) {
        placed.push(PlacedRow { row: r, y });
        row_y[r] = Some(y);
        y += heights[r] + vs;
    }
    let grid_height = y + padding.bottom + border.bottom;

    // Cell fragments, aligned in their rows.
    let column_x = &p.column_x;
    let grid_left = column_x[0];
    let grid_right = column_x[column_x.len() - 1];
    let mut row_cells: Vec<Vec<Fragment>> = (0..rows).map(|_| Vec::new()).collect();
    let mut spanning: Vec<(usize, Fragment)> = Vec::new();
    for placed_row in &placed {
        let r = placed_row.row;
        let y = placed_row.y;
        if let Some((fragments, _, _)) = &part
            && r == body[start]
        {
            // A broken row's cells sit at its top.
            for (fragment, cell) in fragments.iter().zip(&p.cells[p.row_cells[r].clone()]) {
                let mut fragment = fragment.clone();
                fragment.rect.height = heights[r];
                fragment.rect.x = column_x[cell.column] - grid_left;
                fragment.rect.y = 0.0;
                row_cells[r].push(Fragment::Box(fragment));
            }
            continue;
        }
        for cell in &p.cells[p.row_cells[r].clone()] {
            // The rows of the span that follow it on this page.
            let mut height = heights[r];
            let mut count = 1;
            while count < cell.rowspan {
                match row_y.get(r + count).copied().flatten() {
                    Some(next) if next > y => {
                        height = next + heights[r + count] - y;
                        count += 1;
                    }
                    _ => break,
                }
            }
            let mut fragment = cell.fragment.clone();
            let content = fragment.rect.height;
            let offset = match cell.align {
                Align::Top => 0.0,
                Align::Middle => ((height - content) / 2.0).max(0.0),
                Align::Bottom => (height - content).max(0.0),
                Align::Baseline => p.baselines[r] - cell.baseline,
            };
            if offset != 0.0 {
                for child in &mut fragment.children {
                    match child {
                        Fragment::Box(b) => b.rect.y += offset,
                        Fragment::Line(l) => l.rect.y += offset,
                    }
                }
            }
            fragment.rect.height = height;
            fragment.rect.x = column_x[cell.column] - grid_left;
            if count > 1 {
                fragment.rect.y = y;
                spanning.push((grid[r].group, Fragment::Box(fragment)));
            } else {
                fragment.rect.y = 0.0;
                row_cells[r].push(Fragment::Box(fragment));
            }
        }
    }

    // Columns, row groups and rows.
    let rows_top = placed.first().map_or(grid_content_top + vs, |p| p.y);
    let rows_bottom = placed.last().map_or(rows_top, |p| p.y + heights[p.row]);
    let mut children = Vec::new();
    for group in &table.column_groups {
        if group.columns.is_empty() {
            continue;
        }
        let left = column_x[group.columns.start];
        let right = column_x[group.columns.end - 1] + p.widths[group.columns.end - 1];
        children.push(Fragment::Box(part_fragment(
            group.node,
            &group.style,
            Rect::new(left, rows_top, right - left, rows_bottom - rows_top),
            Vec::new(),
        )));
    }
    for (i, column) in table.columns.iter().enumerate() {
        if let (Some(node), Some(style)) = (column.node, &column.style) {
            children.push(Fragment::Box(part_fragment(
                Some(node),
                style,
                Rect::new(column_x[i], rows_top, p.widths[i], rows_bottom - rows_top),
                Vec::new(),
            )));
        }
    }
    let mut index = 0;
    while index < placed.len() {
        let g = grid[placed[index].row].group;
        let group_top = placed[index].y;
        let mut group_rows = Vec::new();
        let mut bottom = group_top;
        while index < placed.len() && grid[placed[index].row].group == g {
            let placed_row = &placed[index];
            let r = placed_row.row;
            bottom = placed_row.y + heights[r];
            if !grid[r].collapsed {
                let row = row_of_grid(table, r);
                group_rows.push(Fragment::Box(part_fragment(
                    row.node,
                    &row.style,
                    Rect::new(
                        0.0,
                        placed_row.y - group_top,
                        grid_right - grid_left,
                        heights[r],
                    ),
                    std::mem::take(&mut row_cells[r]),
                )));
            }
            index += 1;
        }
        for (group, mut cell) in std::mem::take(&mut spanning) {
            if group == g {
                if let Fragment::Box(b) = &mut cell {
                    b.rect.y -= group_top;
                }
                group_rows.push(cell);
            } else {
                spanning.push((group, cell));
            }
        }
        let group = &table.groups[g];
        children.push(Fragment::Box(part_fragment(
            group.node,
            &group.style,
            Rect::new(
                grid_left,
                group_top,
                grid_right - grid_left,
                bottom - group_top,
            ),
            group_rows,
        )));
    }

    // The outer halves of collapsed borders, segment by segment, against the cells.
    if let Some(edges) = &table.outer {
        let slice = outer.box_decoration_break != BoxDecorationBreak::Clone;
        let inner_left = border.left;
        let inner_right = width - border.right;
        let inner_top = border.top;
        let inner_bottom = grid_height - border.bottom;
        let side_width = |segments: &[OuterSegment], r: Option<usize>| {
            r.and_then(|r| segments.get(r)).map_or(0.0, |s| s.width)
        };
        // The first and last segments along the top and bottom reach the corners.
        let mut along = |segments: &[OuterSegment], side: usize, y: f32, row: Option<usize>| {
            let count = segments.len();
            for (c, segment) in segments.iter().enumerate() {
                let left = if c == 0 {
                    inner_left - side_width(&edges.left, row)
                } else {
                    column_x[c]
                };
                let right = if c + 1 == count {
                    inner_right + side_width(&edges.right, row)
                } else {
                    column_x[c + 1]
                };
                let y = if side == 0 { y - segment.width } else { y };
                children.extend(border_strip(
                    style,
                    segment,
                    side,
                    Rect::new(left, y, right - left, segment.width),
                ));
            }
        };
        if !(resumed && slice) {
            along(&edges.top, 0, inner_top, placed.first().map(|p| p.row));
        }
        if !(broke && slice) {
            along(&edges.bottom, 2, inner_bottom, placed.last().map(|p| p.row));
        }
        for placed_row in &placed {
            let r = placed_row.row;
            let (top, bottom) = (placed_row.y, placed_row.y + heights[r]);
            if let Some(segment) = edges.left.get(r) {
                children.extend(border_strip(
                    style,
                    segment,
                    3,
                    Rect::new(inner_left - segment.width, top, segment.width, bottom - top),
                ));
            }
            if let Some(segment) = edges.right.get(r) {
                children.extend(border_strip(
                    style,
                    segment,
                    1,
                    Rect::new(inner_right, top, segment.width, bottom - top),
                ));
            }
        }
    }

    let clone = outer.box_decoration_break == BoxDecorationBreak::Clone;
    let mut grid_border = border;
    let mut grid_padding = padding;
    if resumed && !clone {
        grid_border.top = 0.0;
        grid_padding.top = 0.0;
    }
    if broke && !clone {
        grid_border.bottom = 0.0;
        grid_padding.bottom = 0.0;
    }
    let grid_fragment = |node: Option<NodeId>, y: f32| BoxFragment {
        node,
        style: style.clone(),
        rect: Rect::new(0.0, y, width, grid_height),
        border: grid_border,
        padding: grid_padding,
        children,
        replaced: None,
        legend: None,
        out_of_flow: None,
    };
    let top_captions: &[(BoxFragment, Sides)] = if resumed { &[] } else { &p.top_captions };
    let bottom_captions: &[(BoxFragment, Sides)] = if broke { &[] } else { &p.bottom_captions };
    let fragment = if top_captions.is_empty() && bottom_captions.is_empty() {
        grid_fragment(block.node, 0.0)
    } else {
        // The table wrapper box holds the captions and the grid.
        let mut wrapper_children = Vec::new();
        let mut y = 0.0;
        let mut place = |(fragment, margin): &(BoxFragment, Sides), y: &mut f32| {
            let mut fragment = fragment.clone();
            fragment.rect.x = margin.left;
            fragment.rect.y = *y + margin.top;
            *y += fragment.rect.height + margin.vertical();
            wrapper_children.push(Fragment::Box(fragment));
        };
        for caption in top_captions {
            place(caption, &mut y);
        }
        let grid_y = y;
        y += grid_height;
        for caption in bottom_captions {
            place(caption, &mut y);
        }
        let at = top_captions.len();
        wrapper_children.insert(at, Fragment::Box(grid_fragment(None, grid_y)));
        BoxFragment {
            node: block.node,
            style: Arc::new(ComputedValues::inherit_from(outer)),
            rect: Rect::new(0.0, 0.0, width, y),
            border: Sides::default(),
            padding: Sides::default(),
            children: wrapper_children,
            replaced: None,
            legend: None,
            out_of_flow: None,
        }
    };

    let break_token = broke.then_some(BreakToken {
        consumed: 0.0,
        resume: match part_tokens {
            Some(cells) => Resume::RowPart {
                index: start,
                cells,
            },
            None => Resume::Row { index: end },
        },
        forced,
        side: None,
    });
    Laid {
        fragment,
        margin: Sides {
            top: margin_top,
            right: p.margin_right.unwrap_or(0.0),
            bottom: if broke { 0.0 } else { margin_bottom },
            left: p.margin_left.unwrap_or(0.0),
        },
        collapsed_top: CollapsibleMargin::of(margin_top),
        collapsed_bottom: if broke {
            CollapsibleMargin::default()
        } else {
            CollapsibleMargin::of(margin_bottom)
        },
        through: false,
        break_token,
        deferred: false,
    }
}

/// The row at grid row `r`.
fn row_of_grid(table: &Table, mut r: usize) -> &Row {
    for group in &table.groups {
        if r < group.rows.len() {
            return &group.rows[r];
        }
        r -= group.rows.len();
    }
    unreachable!("a grid row is in a group")
}

/// The fragment of a column, column group, row group or row: it paints its background.
fn part_fragment(
    node: Option<NodeId>,
    style: &Arc<ComputedValues>,
    rect: Rect,
    children: Vec<Fragment>,
) -> BoxFragment {
    BoxFragment {
        node,
        style: style.clone(),
        rect,
        border: Sides::default(),
        padding: Sides::default(),
        children,
        replaced: None,
        legend: None,
        out_of_flow: None,
    }
}
