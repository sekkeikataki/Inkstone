pub mod address;
pub mod format;
pub mod formula;

use crate::document::{Color, DocumentError, Point, Rect};
use crate::spreadsheet::address::{CellAddr, CellRange, MAX_COLS, MAX_ROWS, col_name, parse_col};
use crate::spreadsheet::formula::{
    EvalContext, Value, adjust_formula, parse_formula, parse_literal, shift_formula,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

pub const DEFAULT_COL_WIDTH: f32 = 64.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 21.0;
pub const HEADER_COL_WIDTH: f32 = 36.0;
pub const HEADER_ROW_HEIGHT: f32 = 22.0;
pub const TITLE_HEIGHT: f32 = 26.0;
pub const TAB_HEIGHT: f32 = 22.0;
pub const MIN_DISPLAY_COLS: u32 = 10;
pub const MIN_DISPLAY_ROWS: u32 = 10;
pub const GROW_HANDLE: f32 = 14.0;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Notes,
    Excel,
}

impl LayerKind {
    pub fn is_notes(&self) -> bool {
        matches!(self, Self::Notes)
    }

    pub fn is_spreadsheet(&self) -> bool {
        matches!(self, Self::Excel)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Notes => "Notes",
            Self::Excel => "Spreadsheet",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellStyle {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, skip_serializing_if = "is_general_align")]
    pub h_align: HAlign,
    #[serde(default, skip_serializing_if = "is_center_valign")]
    pub v_align: VAlign,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number_format: String,
    #[serde(default, skip_serializing_if = "is_default_font_size")]
    pub font_size: f32,
}

impl CellStyle {
    pub fn font_size_or_default(&self) -> f32 {
        if self.font_size > 0.0 {
            self.font_size
        } else {
            11.0
        }
    }

    pub fn text_color(&self) -> Color {
        self.color.unwrap_or(Color::INK)
    }
}

fn is_general_align(value: &HAlign) -> bool {
    matches!(value, HAlign::General)
}

fn is_center_valign(value: &VAlign) -> bool {
    matches!(value, VAlign::Center)
}

fn is_default_font_size(value: &f32) -> bool {
    *value == 0.0
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Cell {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub input: String,
    #[serde(default, skip_serializing_if = "is_default_style")]
    pub style: CellStyle,
}

fn is_default_style(style: &CellStyle) -> bool {
    *style == CellStyle::default()
}

impl Cell {
    pub fn from_input(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            style: CellStyle::default(),
        }
    }

    pub fn is_formula(&self) -> bool {
        self.input.trim_start().starts_with('=')
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Sheet {
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        serialize_with = "serialize_cells",
        deserialize_with = "deserialize_cells"
    )]
    pub cells: BTreeMap<CellAddr, Cell>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        serialize_with = "serialize_widths",
        deserialize_with = "deserialize_widths"
    )]
    pub column_widths: BTreeMap<u32, f32>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        serialize_with = "serialize_heights",
        deserialize_with = "deserialize_heights"
    )]
    pub row_heights: BTreeMap<u32, f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<CellRange>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub frozen_rows: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub frozen_cols: u32,
    #[serde(
        default = "default_visible_cols",
        skip_serializing_if = "is_default_visible_cols"
    )]
    pub visible_cols: u32,
    #[serde(
        default = "default_visible_rows",
        skip_serializing_if = "is_default_visible_rows"
    )]
    pub visible_rows: u32,
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn default_visible_cols() -> u32 {
    MIN_DISPLAY_COLS
}

fn default_visible_rows() -> u32 {
    MIN_DISPLAY_ROWS
}

fn is_default_visible_cols(value: &u32) -> bool {
    *value == MIN_DISPLAY_COLS
}

fn is_default_visible_rows(value: &u32) -> bool {
    *value == MIN_DISPLAY_ROWS
}

impl Sheet {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: BTreeMap::new(),
            column_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            merged: Vec::new(),
            frozen_rows: 0,
            frozen_cols: 0,
            visible_cols: MIN_DISPLAY_COLS,
            visible_rows: MIN_DISPLAY_ROWS,
        }
    }

    pub fn col_width(&self, col: u32) -> f32 {
        self.column_widths
            .get(&col)
            .copied()
            .filter(|width| width.is_finite() && *width > 0.0)
            .unwrap_or(DEFAULT_COL_WIDTH)
    }

    pub fn row_height(&self, row: u32) -> f32 {
        self.row_heights
            .get(&row)
            .copied()
            .filter(|height| height.is_finite() && *height > 0.0)
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    pub fn used_range(&self) -> CellRange {
        if self.cells.is_empty() {
            return CellRange::single(CellAddr { col: 0, row: 0 });
        }
        let mut min_col = u32::MAX;
        let mut min_row = u32::MAX;
        let mut max_col = 0;
        let mut max_row = 0;
        for addr in self.cells.keys() {
            min_col = min_col.min(addr.col);
            min_row = min_row.min(addr.row);
            max_col = max_col.max(addr.col);
            max_row = max_row.max(addr.row);
        }
        CellRange::new(
            CellAddr {
                col: min_col,
                row: min_row,
            },
            CellAddr {
                col: max_col,
                row: max_row,
            },
        )
    }

    pub fn used_cols(&self) -> u32 {
        self.cells
            .keys()
            .map(|addr| addr.col + 1)
            .max()
            .unwrap_or(0)
    }

    pub fn used_rows(&self) -> u32 {
        self.cells
            .keys()
            .map(|addr| addr.row + 1)
            .max()
            .unwrap_or(0)
    }

    pub fn display_cols(&self) -> u32 {
        self.visible_cols
            .max(self.used_cols())
            .clamp(MIN_DISPLAY_COLS, MAX_COLS)
    }

    pub fn display_rows(&self) -> u32 {
        self.visible_rows
            .max(self.used_rows())
            .clamp(MIN_DISPLAY_ROWS, MAX_ROWS)
    }

    pub fn grow_to_include(&mut self, addr: CellAddr) {
        self.visible_cols = self.display_cols().max(addr.col + 1).min(MAX_COLS);
        self.visible_rows = self.display_rows().max(addr.row + 1).min(MAX_ROWS);
    }

    pub fn set_visible_size(&mut self, cols: u32, rows: u32) {
        self.visible_cols = cols.max(self.used_cols()).clamp(MIN_DISPLAY_COLS, MAX_COLS);
        self.visible_rows = rows.max(self.used_rows()).clamp(MIN_DISPLAY_ROWS, MAX_ROWS);
    }

    pub fn add_visible_cols(&mut self, count: u32) {
        self.visible_cols = self.display_cols().saturating_add(count).min(MAX_COLS);
    }

    pub fn add_visible_rows(&mut self, count: u32) {
        self.visible_rows = self.display_rows().saturating_add(count).min(MAX_ROWS);
    }

    pub fn set_input(&mut self, addr: CellAddr, input: String) {
        if input.is_empty() {
            if let Some(cell) = self.cells.get_mut(&addr) {
                if is_default_style(&cell.style) {
                    self.cells.remove(&addr);
                } else {
                    cell.input.clear();
                }
            }
            return;
        }
        self.cells.entry(addr).or_default().input = input;
    }

    pub fn clear_range(&mut self, range: CellRange) {
        for addr in range.cells() {
            self.set_input(addr, String::new());
        }
    }

    pub fn merge_range(&mut self, range: CellRange) {
        if range.start == range.end {
            self.merged
                .retain(|existing| !existing.contains(range.start));
            return;
        }
        let exact = self.merged.contains(&range);
        self.merged.retain(|existing| !existing.intersects(range));
        if !exact {
            self.merged.push(range);
        }
    }

    pub fn merge_anchor(&self, addr: CellAddr) -> CellAddr {
        self.merge_containing(addr)
            .map(|range| range.start)
            .unwrap_or(addr)
    }

    pub fn merge_containing(&self, addr: CellAddr) -> Option<CellRange> {
        self.merged
            .iter()
            .copied()
            .find(|range| range.contains(addr))
    }

    pub fn insert_rows(&mut self, before: u32, count: u32) {
        self.shift(0, before, 0, count as i32);
    }

    pub fn insert_cols(&mut self, before: u32, count: u32) {
        self.shift(before, 0, count as i32, 0);
    }

    pub fn delete_rows(&mut self, before: u32, count: u32) {
        self.shift(0, before, 0, -(count as i32));
    }

    pub fn delete_cols(&mut self, before: u32, count: u32) {
        self.shift(before, 0, -(count as i32), 0);
    }

    fn shift(&mut self, col0: u32, row0: u32, dcol: i32, drow: i32) {
        let mut next = BTreeMap::new();
        for (addr, cell) in std::mem::take(&mut self.cells) {
            if let Some(new_addr) = shift_addr(addr, col0, row0, dcol, drow) {
                next.insert(new_addr, cell);
            }
        }
        self.cells = next;
        for cell in self.cells.values_mut() {
            if cell.is_formula() {
                cell.input = shift_formula(&cell.input, col0, row0, dcol, drow);
            }
        }
        self.column_widths = shift_index_map(std::mem::take(&mut self.column_widths), col0, dcol);
        self.row_heights = shift_index_map(std::mem::take(&mut self.row_heights), row0, drow);
        self.merged = self
            .merged
            .drain(..)
            .filter_map(|range| shift_range(range, col0, row0, dcol, drow))
            .collect();
        if drow != 0 && row0 < self.frozen_rows {
            self.frozen_rows = i64::from(self.frozen_rows)
                .saturating_add(i64::from(drow))
                .max(0) as u32;
        }
        if dcol != 0 && col0 < self.frozen_cols {
            self.frozen_cols = i64::from(self.frozen_cols)
                .saturating_add(i64::from(dcol))
                .max(0) as u32;
        }
        if dcol > 0 {
            self.visible_cols = self.visible_cols.saturating_add(dcol as u32).min(MAX_COLS);
        } else if dcol < 0 {
            self.visible_cols = self.visible_cols.saturating_sub(dcol.unsigned_abs()).max(1);
        }
        if drow > 0 {
            self.visible_rows = self.visible_rows.saturating_add(drow as u32).min(MAX_ROWS);
        } else if drow < 0 {
            self.visible_rows = self.visible_rows.saturating_sub(drow.unsigned_abs()).max(1);
        }
    }

    pub fn copy_range(&self, range: CellRange, dcol: i32, drow: i32) -> Vec<(CellAddr, Cell)> {
        let mut copies = Vec::new();
        for addr in range.cells() {
            let Some(cell) = self.cells.get(&addr) else {
                continue;
            };
            let Some(target) = addr.offset(dcol, drow) else {
                continue;
            };
            let mut copy = cell.clone();
            if copy.is_formula() {
                copy.input = adjust_formula(&copy.input, dcol, drow);
            }
            copies.push((target, copy));
        }
        copies
    }

    pub fn paste_from(
        &mut self,
        source_origin: CellAddr,
        cells: &[(CellAddr, Cell)],
        dest: CellAddr,
    ) {
        let dcol = dest.col as i32 - source_origin.col as i32;
        let drow = dest.row as i32 - source_origin.row as i32;
        for (addr, cell) in cells {
            let Some(target) = addr.offset(dcol, drow) else {
                continue;
            };
            let mut copy = cell.clone();
            if copy.is_formula() {
                copy.input = adjust_formula(&copy.input, dcol, drow);
            }
            if copy.input.is_empty() && is_default_style(&copy.style) {
                self.cells.remove(&target);
            } else {
                self.cells.insert(target, copy);
            }
            self.grow_to_include(target);
        }
    }

    pub fn set_col_width(&mut self, col: u32, width: f32) {
        self.column_widths.insert(col, width.clamp(24.0, 480.0));
    }

    pub fn set_row_height(&mut self, row: u32, height: f32) {
        self.row_heights.insert(row, height.clamp(12.0, 240.0));
    }

    pub fn fill(&mut self, source: CellRange, target: CellRange) {
        let height = source.rows();
        let width = source.cols();
        for addr in target.cells() {
            if source.contains(addr) {
                continue;
            }
            let src_col = source.start.col + (addr.col - target.start.col) % width;
            let src_row = source.start.row + (addr.row - target.start.row) % height;
            let src = CellAddr {
                col: src_col,
                row: src_row,
            };
            let dcol = addr.col as i32 - src.col as i32;
            let drow = addr.row as i32 - src.row as i32;
            if let Some(cell) = self.cells.get(&src).cloned() {
                let mut copy = cell;
                if copy.is_formula() {
                    copy.input = adjust_formula(&copy.input, dcol, drow);
                } else if let Ok(number) = parse_literal(&copy.input).as_number()
                    && !copy.input.chars().any(|ch| ch.is_ascii_alphabetic())
                {
                    let series = number + f64::from(drow + dcol);
                    copy.input = crate::spreadsheet::formula::format_general(series);
                }
                self.cells.insert(addr, copy);
            }
        }
    }

    pub fn sort_range(&mut self, range: CellRange, key_col: u32, ascending: bool) {
        let mut rows: Vec<Vec<(CellAddr, Cell)>> = Vec::new();
        for row in range.start.row..=range.end.row {
            let mut cells = Vec::new();
            for col in range.start.col..=range.end.col {
                let addr = CellAddr { col, row };
                if let Some(cell) = self.cells.remove(&addr) {
                    cells.push((addr, cell));
                } else {
                    cells.push((addr, Cell::default()));
                }
            }
            rows.push(cells);
        }
        rows.sort_by(|a, b| {
            let offset = key_col.saturating_sub(range.start.col) as usize;
            let left = a.get(offset).map(|(_, cell)| &cell.input);
            let right = b.get(offset).map(|(_, cell)| &cell.input);
            match (left, right) {
                (Some(l), Some(r)) => {
                    let order = parse_literal(l)
                        .comparable()
                        .ok()
                        .zip(parse_literal(r).comparable().ok())
                        .and_then(|(a, b)| a.cmp_excel(&b).ok())
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if ascending { order } else { order.reverse() }
                }
                _ => std::cmp::Ordering::Equal,
            }
        });
        for (index, row_cells) in rows.into_iter().enumerate() {
            let row = range.start.row + index as u32;
            for (source, cell) in row_cells {
                if !cell.input.is_empty() || !is_default_style(&cell.style) {
                    self.cells.insert(
                        CellAddr {
                            col: source.col,
                            row,
                        },
                        cell,
                    );
                }
            }
        }
    }

    pub fn sort_evaluated(&mut self, range: CellRange, keys: &[Value], ascending: bool) {
        let mut rows: Vec<(Value, Vec<(CellAddr, Cell)>)> = Vec::new();
        for (index, row) in (range.start.row..=range.end.row).enumerate() {
            let mut cells = Vec::new();
            for col in range.start.col..=range.end.col {
                let addr = CellAddr { col, row };
                if let Some(cell) = self.cells.remove(&addr) {
                    cells.push((addr, cell));
                } else {
                    cells.push((addr, Cell::default()));
                }
            }
            rows.push((keys.get(index).cloned().unwrap_or(Value::Empty), cells));
        }
        rows.sort_by(|a, b| {
            let order =
                a.0.comparable()
                    .ok()
                    .zip(b.0.comparable().ok())
                    .and_then(|(l, r)| l.cmp_excel(&r).ok())
                    .unwrap_or(std::cmp::Ordering::Equal);
            if ascending { order } else { order.reverse() }
        });
        for (index, (_, row_cells)) in rows.into_iter().enumerate() {
            let row = range.start.row + index as u32;
            for (source, cell) in row_cells {
                if !cell.input.is_empty() || !is_default_style(&cell.style) {
                    self.cells.insert(
                        CellAddr {
                            col: source.col,
                            row,
                        },
                        cell,
                    );
                }
            }
        }
    }

    pub fn searchable_text(&self) -> impl Iterator<Item = (CellAddr, &str)> {
        self.cells
            .iter()
            .map(|(addr, cell)| (*addr, cell.input.as_str()))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Spreadsheet {
    #[serde(default)]
    pub origin: Point,
    #[serde(default)]
    pub sheets: Vec<Sheet>,
    #[serde(default)]
    pub active_sheet: usize,
}

impl Default for Spreadsheet {
    fn default() -> Self {
        Self::new()
    }
}

impl Spreadsheet {
    pub fn new() -> Self {
        Self {
            origin: Point::new(48.0, 36.0),
            sheets: vec![Sheet::named("Sheet1")],
            active_sheet: 0,
        }
    }

    pub fn active(&self) -> &Sheet {
        self.sheets
            .get(self.active_sheet.min(self.sheets.len().saturating_sub(1)))
            .unwrap_or(&self.sheets[0])
    }

    pub fn active_mut(&mut self) -> &mut Sheet {
        let index = self.active_sheet.min(self.sheets.len().saturating_sub(1));
        &mut self.sheets[index]
    }

    pub fn add_sheet(&mut self) -> usize {
        let mut index = self.sheets.len() + 1;
        let mut name = format!("Sheet{index}");
        while self
            .sheets
            .iter()
            .any(|sheet| sheet.name.eq_ignore_ascii_case(&name))
        {
            index += 1;
            name = format!("Sheet{index}");
        }
        self.sheets.push(Sheet::named(name));
        self.active_sheet = self.sheets.len() - 1;
        self.active_sheet
    }

    pub fn evaluate(&self, sheet_index: usize, addr: CellAddr) -> Value {
        let mut ctx = EvalContext::new(&self.sheets, sheet_index, addr);
        ctx.evaluate_cell(sheet_index, addr)
    }

    pub fn display_cell(&self, sheet_index: usize, addr: CellAddr) -> String {
        let mut ctx = EvalContext::new(&self.sheets, sheet_index, addr);
        self.display_with(&mut ctx, sheet_index, addr)
    }

    pub fn display_with(
        &self,
        ctx: &mut EvalContext<'_>,
        sheet_index: usize,
        addr: CellAddr,
    ) -> String {
        let value = ctx.evaluate_cell(sheet_index, addr);
        self.format_value(sheet_index, addr, &value)
    }

    pub fn format_value(&self, sheet_index: usize, addr: CellAddr, value: &Value) -> String {
        let format = self
            .sheets
            .get(sheet_index)
            .and_then(|sheet| sheet.cells.get(&addr))
            .map(|cell| cell.style.number_format.as_str())
            .unwrap_or("");
        format::format_cell(value, format)
    }

    pub fn sort_range(&mut self, range: CellRange, key_col: u32, ascending: bool) {
        let sheet_index = self.active_sheet;
        let keys: Vec<Value> = (range.start.row..=range.end.row)
            .map(|row| self.evaluate(sheet_index, CellAddr { col: key_col, row }))
            .collect();
        self.active_mut().sort_evaluated(range, &keys, ascending);
    }

    pub fn col_stops(&self) -> Vec<f32> {
        let sheet = self.active();
        let cols = sheet.display_cols();
        let mut xs = Vec::with_capacity(cols as usize + 1);
        let mut x = self.origin.x + HEADER_COL_WIDTH;
        xs.push(x);
        for col in 0..cols {
            x += sheet.col_width(col);
            xs.push(x);
        }
        xs
    }

    pub fn row_stops(&self) -> Vec<f32> {
        let sheet = self.active();
        let rows = sheet.display_rows();
        let mut ys = Vec::with_capacity(rows as usize + 1);
        let mut y = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        ys.push(y);
        for row in 0..rows {
            y += sheet.row_height(row);
            ys.push(y);
        }
        ys
    }

    pub fn set_active_input(&mut self, addr: CellAddr, input: String) -> Result<(), DocumentError> {
        if self.active().name.trim().is_empty() {
            return Err(DocumentError::Invalid(
                "spreadsheet sheet is missing a name".to_owned(),
            ));
        }
        if input.trim_start().starts_with('=') && parse_formula(&input).is_err() {
            // Excel still stores the text; evaluation yields #NAME?/#VALUE!. Keep the input.
        }
        self.active_mut().set_input(addr, input);
        Ok(())
    }

    pub fn bounds(&self) -> Rect {
        let sheet = self.active();
        let cols = sheet.display_cols();
        let rows = sheet.display_rows();
        let mut width = HEADER_COL_WIDTH;
        for col in 0..cols {
            width += sheet.col_width(col);
        }
        let mut height = TITLE_HEIGHT + HEADER_ROW_HEIGHT + TAB_HEIGHT;
        for row in 0..rows {
            height += sheet.row_height(row);
        }
        Rect {
            x: self.origin.x,
            y: self.origin.y,
            width,
            height,
        }
    }

    pub fn cell_origin(&self, addr: CellAddr) -> Point {
        let sheet = self.active();
        let mut x = self.origin.x + HEADER_COL_WIDTH;
        for col in 0..addr.col {
            x += sheet.col_width(col);
        }
        let mut y = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        for row in 0..addr.row {
            y += sheet.row_height(row);
        }
        Point::new(x, y)
    }

    pub fn cell_rect(&self, addr: CellAddr) -> Rect {
        let sheet = self.active();
        let origin = self.cell_origin(addr);
        Rect {
            x: origin.x,
            y: origin.y,
            width: sheet.col_width(addr.col),
            height: sheet.row_height(addr.row),
        }
    }

    pub fn hit_cell(&self, point: Point) -> Option<CellAddr> {
        let bounds = self.bounds();
        let sheet = self.active();
        let grid_top = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        let grid_left = self.origin.x + HEADER_COL_WIDTH;
        if point.x < grid_left
            || point.y < grid_top
            || point.x > bounds.x + bounds.width
            || point.y > bounds.y + bounds.height - TAB_HEIGHT
        {
            return None;
        }
        let mut x = grid_left;
        let mut col = None;
        for index in 0..sheet.display_cols() {
            let width = sheet.col_width(index);
            if point.x >= x && point.x < x + width {
                col = Some(index);
                break;
            }
            x += width;
        }
        let mut y = grid_top;
        let mut row = None;
        for index in 0..sheet.display_rows() {
            let height = sheet.row_height(index);
            if point.y >= y && point.y < y + height {
                row = Some(index);
                break;
            }
            y += height;
        }
        CellAddr::new(col?, row?)
    }

    pub fn hit_title(&self, point: Point) -> bool {
        let bounds = self.bounds();
        point.x >= bounds.x
            && point.x <= bounds.x + bounds.width
            && point.y >= bounds.y
            && point.y <= bounds.y + TITLE_HEIGHT
    }

    pub fn hit_select_all(&self, point: Point) -> bool {
        let header_top = self.origin.y + TITLE_HEIGHT;
        point.x >= self.origin.x
            && point.x < self.origin.x + HEADER_COL_WIDTH
            && point.y >= header_top
            && point.y < header_top + HEADER_ROW_HEIGHT
    }

    pub fn hit_col_header(&self, point: Point) -> Option<u32> {
        let header_top = self.origin.y + TITLE_HEIGHT;
        if point.y < header_top || point.y >= header_top + HEADER_ROW_HEIGHT {
            return None;
        }
        let sheet = self.active();
        let mut x = self.origin.x + HEADER_COL_WIDTH;
        for col in 0..sheet.display_cols() {
            let width = sheet.col_width(col);
            if point.x >= x && point.x < x + width {
                return Some(col);
            }
            x += width;
        }
        None
    }

    pub fn hit_row_header(&self, point: Point) -> Option<u32> {
        if point.x < self.origin.x || point.x >= self.origin.x + HEADER_COL_WIDTH {
            return None;
        }
        let sheet = self.active();
        let mut y = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        for row in 0..sheet.display_rows() {
            let height = sheet.row_height(row);
            if point.y >= y && point.y < y + height {
                return Some(row);
            }
            y += height;
        }
        None
    }

    pub fn hit_col_resize(&self, point: Point) -> Option<u32> {
        let header_top = self.origin.y + TITLE_HEIGHT;
        if point.y < header_top || point.y >= header_top + HEADER_ROW_HEIGHT {
            return None;
        }
        let sheet = self.active();
        let mut x = self.origin.x + HEADER_COL_WIDTH;
        for col in 0..sheet.display_cols() {
            x += sheet.col_width(col);
            if (point.x - x).abs() <= 4.0 {
                return Some(col);
            }
        }
        None
    }

    pub fn hit_row_resize(&self, point: Point) -> Option<u32> {
        if point.x < self.origin.x || point.x >= self.origin.x + HEADER_COL_WIDTH {
            return None;
        }
        let sheet = self.active();
        let mut y = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        for row in 0..sheet.display_rows() {
            y += sheet.row_height(row);
            if (point.y - y).abs() <= 4.0 {
                return Some(row);
            }
        }
        None
    }

    pub fn hit_grow_handle(&self, point: Point) -> bool {
        let handle = self.grow_handle_rect();
        point.x >= handle.x
            && point.x <= handle.x + handle.width
            && point.y >= handle.y
            && point.y <= handle.y + handle.height
    }

    pub fn grow_handle_rect(&self) -> Rect {
        let bounds = self.bounds();
        Rect {
            x: bounds.x + bounds.width - GROW_HANDLE,
            y: bounds.y + bounds.height - GROW_HANDLE,
            width: GROW_HANDLE,
            height: GROW_HANDLE,
        }
    }

    pub fn visible_size_at(&self, point: Point) -> (u32, u32) {
        let sheet = self.active();
        let mut cols = MIN_DISPLAY_COLS;
        let mut x = self.origin.x + HEADER_COL_WIDTH;
        for col in 0..MAX_COLS.min(256) {
            x += sheet.col_width(col);
            if x >= point.x {
                cols = (col + 1).max(MIN_DISPLAY_COLS);
                break;
            }
            cols = (col + 1).max(MIN_DISPLAY_COLS);
        }
        let mut rows = MIN_DISPLAY_ROWS;
        let mut y = self.origin.y + TITLE_HEIGHT + HEADER_ROW_HEIGHT;
        for row in 0..MAX_ROWS.min(512) {
            y += sheet.row_height(row);
            if y >= point.y {
                rows = (row + 1).max(MIN_DISPLAY_ROWS);
                break;
            }
            rows = (row + 1).max(MIN_DISPLAY_ROWS);
        }
        (cols, rows)
    }

    pub fn hit_tab(&self, point: Point) -> Option<usize> {
        let bounds = self.bounds();
        let top = bounds.y + bounds.height - TAB_HEIGHT;
        if point.y < top || point.y > bounds.y + bounds.height || point.x < bounds.x {
            return None;
        }
        let mut x = bounds.x + 8.0;
        for (index, sheet) in self.sheets.iter().enumerate() {
            let width = (sheet.name.len() as f32 * 7.5 + 18.0).max(48.0);
            if point.x >= x && point.x <= x + width {
                return Some(index);
            }
            x += width + 6.0;
        }
        None
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.sheets.is_empty() {
            return Err(DocumentError::Invalid(
                "a spreadsheet layer needs at least one sheet".to_owned(),
            ));
        }
        if self.active_sheet >= self.sheets.len() {
            return Err(DocumentError::Invalid(
                "spreadsheet active sheet is out of range".to_owned(),
            ));
        }
        if !self.origin.is_finite() {
            return Err(DocumentError::Invalid(
                "spreadsheet origin is not finite".to_owned(),
            ));
        }
        let mut names = std::collections::HashSet::new();
        for sheet in &self.sheets {
            if sheet.name.trim().is_empty() {
                return Err(DocumentError::Invalid(
                    "spreadsheet sheet names cannot be empty".to_owned(),
                ));
            }
            if !names.insert(sheet.name.to_ascii_lowercase()) {
                return Err(DocumentError::Invalid(format!(
                    "duplicate spreadsheet sheet name {}",
                    sheet.name
                )));
            }
            for width in sheet.column_widths.values() {
                if !width.is_finite() || *width <= 0.0 {
                    return Err(DocumentError::Invalid(format!(
                        "sheet {} has an invalid column width",
                        sheet.name
                    )));
                }
            }
            for height in sheet.row_heights.values() {
                if !height.is_finite() || *height <= 0.0 {
                    return Err(DocumentError::Invalid(format!(
                        "sheet {} has an invalid row height",
                        sheet.name
                    )));
                }
            }
            for range in &sheet.merged {
                if range.end.col >= MAX_COLS || range.end.row >= MAX_ROWS {
                    return Err(DocumentError::Invalid(format!(
                        "sheet {} has an invalid merged range",
                        sheet.name
                    )));
                }
            }
            if sheet.visible_cols > MAX_COLS || sheet.visible_rows > MAX_ROWS {
                return Err(DocumentError::Invalid(format!(
                    "sheet {} is larger than an Excel worksheet",
                    sheet.name
                )));
            }
        }
        Ok(())
    }

    pub fn translate(&mut self, delta: Point) {
        self.origin.x += delta.x;
        self.origin.y += delta.y;
    }

    pub fn svg(&self, layer_id: impl fmt::Display) -> String {
        let bounds = self.bounds();
        let sheet = self.active();
        let mut svg = format!(
            "<g data-inkstone-id=\"{layer_id}\" data-kind=\"spreadsheet\" data-sheet=\"{}\">\n\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#c5cad3\" stroke-width=\"1\"/>\n",
            crate::document::escape_xml(&sheet.name),
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height
        );
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#333333\">{}</text>\n",
            bounds.x + 10.0,
            bounds.y + 18.0,
            crate::document::escape_xml(&sheet.name)
        ));
        let cols = sheet.display_cols().min(256);
        let rows = sheet.display_rows().min(256);
        let xs = self.col_stops();
        let ys = self.row_stops();
        let mut ctx =
            EvalContext::new(&self.sheets, self.active_sheet, CellAddr { col: 0, row: 0 });
        for col in 0..cols {
            for row in 0..rows {
                let addr = CellAddr { col, row };
                if sheet.merge_anchor(addr) != addr {
                    continue;
                }
                let (x, y, width, height) = if let Some(merge) = sheet.merge_containing(addr) {
                    let x = xs.get(merge.start.col as usize).copied().unwrap_or(0.0);
                    let y = ys.get(merge.start.row as usize).copied().unwrap_or(0.0);
                    let right = xs.get((merge.end.col + 1) as usize).copied().unwrap_or(x);
                    let bottom = ys.get((merge.end.row + 1) as usize).copied().unwrap_or(y);
                    (x, y, right - x, bottom - y)
                } else {
                    let x = xs.get(col as usize).copied().unwrap_or(0.0);
                    let y = ys.get(row as usize).copied().unwrap_or(0.0);
                    (
                        x,
                        y,
                        xs.get((col + 1) as usize).copied().unwrap_or(x) - x,
                        ys.get((row + 1) as usize).copied().unwrap_or(y) - y,
                    )
                };
                let fill = sheet
                    .cells
                    .get(&addr)
                    .and_then(|cell| cell.style.fill)
                    .map(|color| color.svg())
                    .unwrap_or_else(|| "#ffffff".to_owned());
                svg.push_str(&format!(
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"{fill}\" stroke=\"#d8dde6\" stroke-width=\"0.6\"/>\n"
                ));
                if !sheet.cells.contains_key(&addr) {
                    continue;
                }
                let text = self.display_with(&mut ctx, self.active_sheet, addr);
                if !text.is_empty() {
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#222222\">{}</text>\n",
                        x + 4.0,
                        y + height * 0.72,
                        crate::document::escape_xml(&text)
                    ));
                }
            }
        }
        svg.push_str("</g>\n");
        svg
    }
}

fn shift_addr(addr: CellAddr, col0: u32, row0: u32, dcol: i32, drow: i32) -> Option<CellAddr> {
    if dcol < 0 && addr.col >= col0 && addr.col < col0 + dcol.unsigned_abs() {
        return None;
    }
    if drow < 0 && addr.row >= row0 && addr.row < row0 + drow.unsigned_abs() {
        return None;
    }
    let mut col = addr.col;
    let mut row = addr.row;
    if dcol != 0 && addr.col >= col0 {
        let shifted = i64::from(col) + i64::from(dcol);
        if shifted < 0 {
            return None;
        }
        col = shifted as u32;
    }
    if drow != 0 && addr.row >= row0 {
        let shifted = i64::from(row) + i64::from(drow);
        if shifted < 0 {
            return None;
        }
        row = shifted as u32;
    }
    CellAddr::new(col, row)
}

fn shift_range(range: CellRange, col0: u32, row0: u32, dcol: i32, drow: i32) -> Option<CellRange> {
    Some(CellRange::new(
        shift_addr(range.start, col0, row0, dcol, drow)?,
        shift_addr(range.end, col0, row0, dcol, drow)?,
    ))
}

fn shift_index_map<T>(map: BTreeMap<u32, T>, start: u32, delta: i32) -> BTreeMap<u32, T> {
    if delta == 0 {
        return map;
    }
    let mut next = BTreeMap::new();
    for (index, value) in map {
        if delta < 0 && index >= start && index < start + delta.unsigned_abs() {
            continue;
        }
        let mut shifted = index;
        if index >= start {
            let next_index = i64::from(index) + i64::from(delta);
            if next_index < 0 {
                continue;
            }
            shifted = next_index as u32;
        }
        next.insert(shifted, value);
    }
    next
}

fn serialize_cells<S: Serializer>(
    cells: &BTreeMap<CellAddr, Cell>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mapped: BTreeMap<String, &Cell> =
        cells.iter().map(|(addr, cell)| (addr.a1(), cell)).collect();
    mapped.serialize(serializer)
}

fn deserialize_cells<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<CellAddr, Cell>, D::Error> {
    let mapped = BTreeMap::<String, Cell>::deserialize(deserializer)?;
    let mut cells = BTreeMap::new();
    for (key, cell) in mapped {
        let addr = CellAddr::parse_a1(&key)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid spreadsheet cell {key}")))?;
        cells.insert(addr, cell);
    }
    Ok(cells)
}

fn serialize_widths<S: Serializer>(
    widths: &BTreeMap<u32, f32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mapped: BTreeMap<String, f32> = widths
        .iter()
        .map(|(col, width)| (col_name(*col), *width))
        .collect();
    mapped.serialize(serializer)
}

fn deserialize_widths<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<u32, f32>, D::Error> {
    let mapped = BTreeMap::<String, f32>::deserialize(deserializer)?;
    let mut widths = BTreeMap::new();
    for (key, width) in mapped {
        let col = parse_col(&key)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid column {key}")))?;
        widths.insert(col, width);
    }
    Ok(widths)
}

fn serialize_heights<S: Serializer>(
    heights: &BTreeMap<u32, f32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mapped: BTreeMap<String, f32> = heights
        .iter()
        .map(|(row, height)| ((row + 1).to_string(), *height))
        .collect();
    mapped.serialize(serializer)
}

fn deserialize_heights<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<u32, f32>, D::Error> {
    let mapped = BTreeMap::<String, f32>::deserialize(deserializer)?;
    let mut heights = BTreeMap::new();
    for (key, height) in mapped {
        let row = key.parse::<u32>().map_err(serde::de::Error::custom)?;
        if row == 0 {
            return Err(serde::de::Error::custom("row numbers start at 1"));
        }
        heights.insert(row - 1, height);
    }
    Ok(heights)
}
