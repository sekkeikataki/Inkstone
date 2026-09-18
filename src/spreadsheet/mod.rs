pub mod address;
pub mod format;
pub mod formula;

use crate::document::{Color, DocumentError, Point, Rect};
use crate::spreadsheet::address::{CellAddr, CellRange, MAX_COLS, MAX_ROWS, col_name, parse_col};
use crate::spreadsheet::formula::{
    EvalContext, Value, adjust_formula, parse_formula, parse_literal,
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
pub const MIN_DISPLAY_COLS: u32 = 8;
pub const MIN_DISPLAY_ROWS: u32 = 20;

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
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
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

    pub fn display_cols(&self) -> u32 {
        let used = self
            .cells
            .keys()
            .map(|addr| addr.col + 1)
            .max()
            .unwrap_or(0);
        used.clamp(MIN_DISPLAY_COLS, MAX_COLS)
    }

    pub fn display_rows(&self) -> u32 {
        let used = self
            .cells
            .keys()
            .map(|addr| addr.row + 1)
            .max()
            .unwrap_or(0);
        used.clamp(MIN_DISPLAY_ROWS, MAX_ROWS)
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
            self.cells.remove(&addr);
        }
    }

    pub fn merge_range(&mut self, range: CellRange) {
        self.merged
            .retain(|existing| !existing.contains(range.start));
        if range.start != range.end {
            self.merged.push(range);
        }
    }

    pub fn merge_anchor(&self, addr: CellAddr) -> CellAddr {
        self.merged
            .iter()
            .find(|range| range.contains(addr))
            .map(|range| range.start)
            .unwrap_or(addr)
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
        for (addr, mut cell) in std::mem::take(&mut self.cells) {
            if (dcol < 0 && addr.col >= col0 && addr.col < col0 + dcol.unsigned_abs())
                || (drow < 0 && addr.row >= row0 && addr.row < row0 + drow.unsigned_abs())
            {
                continue;
            }
            let mut col = addr.col;
            let mut row = addr.row;
            if addr.col >= col0 {
                let shifted = i64::from(col) + i64::from(dcol);
                if shifted < 0 {
                    continue;
                }
                col = shifted as u32;
            }
            if addr.row >= row0 {
                let shifted = i64::from(row) + i64::from(drow);
                if shifted < 0 {
                    continue;
                }
                row = shifted as u32;
            }
            if let Some(new_addr) = CellAddr::new(col, row) {
                if cell.is_formula() {
                    cell.input = adjust_formula(
                        &cell.input,
                        if addr.col >= col0 { dcol } else { 0 },
                        if addr.row >= row0 { drow } else { 0 },
                    );
                }
                next.insert(new_addr, cell);
            }
        }
        self.cells = next;
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
            let left = a.get(offset).map(|(_, cell)| parse_literal(&cell.input));
            let right = b.get(offset).map(|(_, cell)| parse_literal(&cell.input));
            match (left, right) {
                (Some(l), Some(r)) => {
                    let order = l
                        .comparable()
                        .ok()
                        .zip(r.comparable().ok())
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
        let value = self.evaluate(sheet_index, addr);
        let format = self
            .sheets
            .get(sheet_index)
            .and_then(|sheet| sheet.cells.get(&addr))
            .map(|cell| cell.style.number_format.as_str())
            .unwrap_or("");
        format::format_cell(&value, format)
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
        let cols = sheet.display_cols().min(40);
        let rows = sheet.display_rows().min(80);
        for col in 0..cols {
            for row in 0..rows {
                let addr = CellAddr { col, row };
                let rect = self.cell_rect(addr);
                let fill = sheet
                    .cells
                    .get(&addr)
                    .and_then(|cell| cell.style.fill)
                    .map(|color| color.svg())
                    .unwrap_or_else(|| "#ffffff".to_owned());
                svg.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" stroke=\"#d8dde6\" stroke-width=\"0.6\"/>\n",
                    rect.x, rect.y, rect.width, rect.height
                ));
                let text = self.display_cell(self.active_sheet, addr);
                if !text.is_empty() {
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#222222\">{}</text>\n",
                        rect.x + 4.0,
                        rect.y + rect.height * 0.72,
                        crate::document::escape_xml(&text)
                    ));
                }
            }
        }
        svg.push_str("</g>\n");
        svg
    }
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
