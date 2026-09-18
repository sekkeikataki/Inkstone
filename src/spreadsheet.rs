use crate::document::{Color, DocumentError, Point, Rect};
use calamine::{Data, Reader, open_workbook_auto};
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub const DEFAULT_COLUMN_WIDTH: f32 = 88.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 24.0;
pub const HEADER_WIDTH: f32 = 42.0;
pub const HEADER_HEIGHT: f32 = 22.0;
pub const TAB_HEIGHT: f32 = 26.0;
pub const DEFAULT_VISIBLE_COLS: u32 = 8;
pub const DEFAULT_VISIBLE_ROWS: u32 = 20;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub number_format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<Color>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StoredCell {
    pub value: String,
    #[serde(default)]
    pub format: CellFormat,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Sheet {
    pub name: String,
    #[serde(default)]
    pub cells: BTreeMap<String, StoredCell>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub column_widths: HashMap<u32, f32>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub row_heights: HashMap<u32, f32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpreadsheetLayer {
    pub origin: Point,
    pub bounds: Rect,
    pub active_sheet: usize,
    pub sheets: Vec<Sheet>,
    pub default_column_width: f32,
    pub default_row_height: f32,
    pub show_grid_lines: bool,
    pub frozen_rows: u32,
    pub frozen_columns: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CellAddress {
    pub row: u32,
    pub col: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Empty,
    Text(String),
    Number(f64),
    Boolean(bool),
    Error(String),
}

impl CellValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(text) => text.parse().ok(),
            _ => None,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(text) => text.clone(),
            Self::Number(value) => format_number(*value),
            Self::Boolean(value) => value.to_string(),
            Self::Error(code) => code.clone(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

impl Default for SpreadsheetLayer {
    fn default() -> Self {
        Self {
            origin: Point::new(80.0, 80.0),
            bounds: Rect {
                x: 80.0,
                y: 80.0,
                width: HEADER_WIDTH + DEFAULT_COLUMN_WIDTH * DEFAULT_VISIBLE_COLS as f32,
                height: HEADER_HEIGHT
                    + DEFAULT_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32
                    + TAB_HEIGHT,
            },
            active_sheet: 0,
            sheets: vec![Sheet {
                name: "Sheet1".to_owned(),
                ..Default::default()
            }],
            default_column_width: DEFAULT_COLUMN_WIDTH,
            default_row_height: DEFAULT_ROW_HEIGHT,
            show_grid_lines: true,
            frozen_rows: 0,
            frozen_columns: 0,
        }
    }
}

impl SpreadsheetLayer {
    pub fn active_sheet(&self) -> &Sheet {
        &self.sheets[self.active_sheet.min(self.sheets.len().saturating_sub(1))]
    }

    pub fn active_sheet_mut(&mut self) -> &mut Sheet {
        let index = self.active_sheet.min(self.sheets.len().saturating_sub(1));
        &mut self.sheets[index]
    }

    pub fn column_width(&self, sheet: &Sheet, col: u32) -> f32 {
        sheet
            .column_widths
            .get(&col)
            .copied()
            .unwrap_or(self.default_column_width)
    }

    pub fn row_height(&self, sheet: &Sheet, row: u32) -> f32 {
        sheet
            .row_heights
            .get(&row)
            .copied()
            .unwrap_or(self.default_row_height)
    }

    pub fn cell_rect(&self, address: CellAddress) -> Rect {
        let sheet = self.active_sheet();
        let mut x = self.origin.x + HEADER_WIDTH;
        for col in 0..address.col {
            x += self.column_width(sheet, col);
        }
        let mut y = self.origin.y + HEADER_HEIGHT;
        for row in 0..address.row {
            y += self.row_height(sheet, row);
        }
        Rect {
            x,
            y,
            width: self.column_width(sheet, address.col),
            height: self.row_height(sheet, address.row),
        }
    }

    pub fn hit_test(&self, point: Point) -> Option<CellAddress> {
        if !self.bounds.contains(point) {
            return None;
        }
        let sheet = self.active_sheet();
        let grid_left = self.origin.x + HEADER_WIDTH;
        let grid_top = self.origin.y + HEADER_HEIGHT;
        if point.x < grid_left || point.y < grid_top {
            return None;
        }
        let mut x = grid_left;
        let mut col = 0u32;
        loop {
            let width = self.column_width(sheet, col);
            if point.x < x + width {
                break;
            }
            x += width;
            col += 1;
            if col > 1024 {
                return None;
            }
        }
        let mut y = grid_top;
        let mut row = 0u32;
        loop {
            let height = self.row_height(sheet, row);
            if point.y < y + height {
                break;
            }
            y += height;
            row += 1;
            if row > 1_048_576 {
                return None;
            }
        }
        Some(CellAddress { row, col })
    }

    pub fn tab_hit_test(&self, point: Point) -> Option<usize> {
        let tab_top = self.origin.y + self.bounds.height - TAB_HEIGHT;
        if point.y < tab_top || point.x < self.origin.x {
            return None;
        }
        let mut x = self.origin.x;
        for (index, sheet) in self.sheets.iter().enumerate() {
            let width = (sheet.name.len() as f32 * 7.5 + 24.0).max(72.0);
            if point.x >= x && point.x < x + width {
                return Some(index);
            }
            x += width + 4.0;
        }
        None
    }

    pub fn get_raw(&self, address: CellAddress) -> Option<&str> {
        let key = address.to_a1();
        self.active_sheet()
            .cells
            .get(&key)
            .map(|cell| cell.value.as_str())
    }

    pub fn set_raw(&mut self, address: CellAddress, value: impl Into<String>) {
        let value = value.into();
        let key = address.to_a1();
        let sheet = self.active_sheet_mut();
        if value.is_empty() {
            sheet.cells.remove(&key);
        } else {
            sheet.cells.entry(key).or_default().value = value;
        }
    }

    pub fn computed(&self, address: CellAddress) -> CellValue {
        let mut visiting = HashSet::new();
        self.evaluate_cell(self.active_sheet(), address, &mut visiting)
    }

    pub fn display_text(&self, address: CellAddress) -> String {
        self.computed(address).as_text()
    }

    pub fn used_range(&self) -> (u32, u32) {
        let sheet = self.active_sheet();
        let mut max_row = 0u32;
        let mut max_col = 0u32;
        for key in sheet.cells.keys() {
            if let Ok(address) = CellAddress::from_a1(key) {
                max_row = max_row.max(address.row);
                max_col = max_col.max(address.col);
            }
        }
        (max_row, max_col)
    }

    pub fn searchable_text(&self) -> String {
        let mut parts = Vec::new();
        for sheet in &self.sheets {
            parts.push(sheet.name.clone());
            for cell in sheet.cells.values() {
                if !cell.value.is_empty() {
                    parts.push(cell.value.clone());
                }
            }
        }
        parts.join(" ")
    }

    pub fn content_bounds(&self) -> Rect {
        self.bounds
    }

    pub fn add_sheet(&mut self, name: impl Into<String>) {
        self.sheets.push(Sheet {
            name: name.into(),
            ..Default::default()
        });
        self.active_sheet = self.sheets.len() - 1;
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.sheets.is_empty() {
            return Err(DocumentError::Invalid(
                "spreadsheet layer must contain at least one sheet".to_owned(),
            ));
        }
        if self.active_sheet >= self.sheets.len() {
            return Err(DocumentError::Invalid(
                "spreadsheet active_sheet index is out of range".to_owned(),
            ));
        }
        if !self.origin.is_finite()
            || !self.bounds.is_finite()
            || self.bounds.width <= 0.0
            || self.bounds.height <= 0.0
            || self.default_column_width <= 0.0
            || self.default_row_height <= 0.0
        {
            return Err(DocumentError::Invalid(
                "spreadsheet layer has invalid geometry".to_owned(),
            ));
        }
        for sheet in &self.sheets {
            if sheet.name.trim().is_empty() {
                return Err(DocumentError::Invalid(
                    "spreadsheet sheet name cannot be empty".to_owned(),
                ));
            }
            for key in sheet.cells.keys() {
                CellAddress::from_a1(key).map_err(DocumentError::Invalid)?;
            }
        }
        Ok(())
    }

    pub fn from_xlsx(path: &Path) -> Result<Self, DocumentError> {
        let mut workbook = open_workbook_auto(path).map_err(|error| {
            DocumentError::Invalid(format!("could not read spreadsheet: {error}"))
        })?;
        let sheet_names = workbook.sheet_names().to_vec();
        if sheet_names.is_empty() {
            return Err(DocumentError::Invalid(
                "spreadsheet file contains no worksheets".to_owned(),
            ));
        }
        let mut sheets = Vec::new();
        for name in sheet_names {
            let range = workbook
                .worksheet_range(&name)
                .map_err(|error| DocumentError::Invalid(format!("sheet {name}: {error}")))?;
            let formula_range = workbook.worksheet_formula(&name).ok();
            let mut cells = BTreeMap::new();
            for (row, col, data) in range.cells() {
                let address = CellAddress {
                    row: row as u32,
                    col: col as u32,
                };
                let formula = formula_range
                    .as_ref()
                    .and_then(|formulas| formulas.get_value((row as u32, col as u32)))
                    .filter(|value| !value.is_empty())
                    .map(|value| {
                        if value.starts_with('=') {
                            value.to_owned()
                        } else {
                            format!("={value}")
                        }
                    });
                let value = formula.unwrap_or_else(|| data_to_string(data.clone()));
                if !value.is_empty() {
                    cells.insert(
                        address.to_a1(),
                        StoredCell {
                            value,
                            ..Default::default()
                        },
                    );
                }
            }
            sheets.push(Sheet {
                name,
                cells,
                ..Default::default()
            });
        }
        let (max_row, max_col) = sheets
            .iter()
            .map(|sheet| {
                sheet
                    .cells
                    .keys()
                    .fold((0u32, 0u32), |(max_row, max_col), key| {
                        if let Ok(address) = CellAddress::from_a1(key) {
                            (max_row.max(address.row), max_col.max(address.col))
                        } else {
                            (max_row, max_col)
                        }
                    })
            })
            .max()
            .unwrap_or((DEFAULT_VISIBLE_ROWS - 1, DEFAULT_VISIBLE_COLS - 1));
        let visible_rows = max_row.max(DEFAULT_VISIBLE_ROWS - 1) + 1;
        let visible_cols = max_col.max(DEFAULT_VISIBLE_COLS - 1) + 1;
        Ok(Self {
            origin: Point::new(80.0, 80.0),
            bounds: Rect {
                x: 80.0,
                y: 80.0,
                width: HEADER_WIDTH + DEFAULT_COLUMN_WIDTH * visible_cols as f32,
                height: HEADER_HEIGHT + DEFAULT_ROW_HEIGHT * visible_rows as f32 + TAB_HEIGHT,
            },
            active_sheet: 0,
            sheets,
            ..Default::default()
        })
    }

    pub fn export_xlsx(&self, path: &Path) -> Result<(), DocumentError> {
        let mut workbook = Workbook::new();
        for sheet in &self.sheets {
            let mut worksheet = Worksheet::new();
            worksheet
                .set_name(&sheet.name)
                .map_err(|error| DocumentError::Export(error.to_string()))?;
            for (key, cell) in &sheet.cells {
                let address =
                    CellAddress::from_a1(key).map_err(DocumentError::Invalid)?;
                let row = address.row;
                let col = u16::try_from(address.col).map_err(|_| {
                    DocumentError::Export("column index exceeds Excel limit".to_owned())
                })?;
                let mut format = Format::new();
                if cell.format.bold {
                    format = format.set_bold();
                }
                if cell.format.italic {
                    format = format.set_italic();
                }
                let styled = cell.format.bold || cell.format.italic;
                if cell.value.starts_with('=') {
                    let formula = &cell.value[1..];
                    if styled {
                        worksheet
                            .write_formula_with_format(row, col, formula, &format)
                            .map_err(|error| DocumentError::Export(error.to_string()))?;
                    } else {
                        worksheet
                            .write_formula(row, col, formula)
                            .map_err(|error| DocumentError::Export(error.to_string()))?;
                    }
                } else if let Ok(number) = cell.value.parse::<f64>() {
                    if styled {
                        worksheet
                            .write_number_with_format(row, col, number, &format)
                            .map_err(|error| DocumentError::Export(error.to_string()))?;
                    } else {
                        worksheet
                            .write_number(row, col, number)
                            .map_err(|error| DocumentError::Export(error.to_string()))?;
                    }
                } else if styled {
                    worksheet
                        .write_string_with_format(row, col, &cell.value, &format)
                        .map_err(|error| DocumentError::Export(error.to_string()))?;
                } else {
                    worksheet
                        .write_string(row, col, &cell.value)
                        .map_err(|error| DocumentError::Export(error.to_string()))?;
                }
            }
            workbook.push_worksheet(worksheet);
        }
        workbook
            .save(path)
            .map_err(|error| DocumentError::Export(error.to_string()))
    }

    pub fn to_svg(&self) -> String {
        let mut svg = String::new();
        let sheet = self.active_sheet();
        let (max_row, max_col) = self.used_range();
        let rows = max_row.max(5) + 1;
        let cols = max_col.max(5) + 1;
        svg.push_str(&format!(
            "<g data-inkstone-kind=\"spreadsheet\" transform=\"translate({},{})\" \
             data-active-sheet=\"{}\">\n",
            self.origin.x, self.origin.y, self.active_sheet
        ));
        svg.push_str(&format!(
            "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" \
             stroke=\"#c8ccd4\" stroke-width=\"1\"/>\n",
            self.bounds.width, self.bounds.height
        ));
        for col in 0..cols {
            let x = HEADER_WIDTH + col as f32 * self.default_column_width;
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"11\" \
                 fill=\"#666666\" text-anchor=\"middle\">{}</text>\n",
                x + self.default_column_width / 2.0,
                HEADER_HEIGHT / 2.0 + 4.0,
                column_label(col)
            ));
        }
        for row in 0..rows {
            let y = HEADER_HEIGHT + row as f32 * self.default_row_height;
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"11\" \
                 fill=\"#666666\" text-anchor=\"middle\">{}</text>\n",
                HEADER_WIDTH / 2.0,
                y + self.default_row_height / 2.0 + 4.0,
                row + 1
            ));
        }
        for row in 0..rows {
            for col in 0..cols {
                let address = CellAddress { row, col };
                let rect_x = HEADER_WIDTH + col as f32 * self.default_column_width;
                let rect_y = HEADER_HEIGHT + row as f32 * self.default_row_height;
                svg.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" \
                     stroke=\"#e2e4e8\" stroke-width=\"0.5\"/>\n",
                    rect_x, rect_y, self.default_column_width, self.default_row_height
                ));
                let text = self
                    .evaluate_cell(sheet, address, &mut HashSet::new())
                    .as_text();
                if !text.is_empty() {
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"12\" \
                         fill=\"#222222\">{}</text>\n",
                        rect_x + 4.0,
                        rect_y + self.default_row_height / 2.0 + 4.0,
                        crate::document::escape_xml(&text)
                    ));
                }
            }
        }
        svg.push_str("</g>\n");
        svg
    }

    fn evaluate_cell(
        &self,
        sheet: &Sheet,
        address: CellAddress,
        visiting: &mut HashSet<CellAddress>,
    ) -> CellValue {
        if !visiting.insert(address) {
            return CellValue::Error("#CIRC!".to_owned());
        }
        let key = address.to_a1();
        let Some(stored) = sheet.cells.get(&key) else {
            visiting.remove(&address);
            return CellValue::Empty;
        };
        let value = if stored.value.starts_with('=') {
            self.evaluate_formula(sheet, &stored.value[1..], visiting)
        } else if stored.value.eq_ignore_ascii_case("true") {
            CellValue::Boolean(true)
        } else if stored.value.eq_ignore_ascii_case("false") {
            CellValue::Boolean(false)
        } else if let Ok(number) = stored.value.parse::<f64>() {
            CellValue::Number(number)
        } else {
            CellValue::Text(stored.value.clone())
        };
        visiting.remove(&address);
        value
    }

    fn evaluate_formula(
        &self,
        sheet: &Sheet,
        expression: &str,
        visiting: &mut HashSet<CellAddress>,
    ) -> CellValue {
        let expression = expression.trim();
        if expression.is_empty() {
            return CellValue::Error("#VALUE!".to_owned());
        }
        if let Some((name, args)) = parse_function_call(expression) {
            return self.evaluate_function(sheet, &name, &args, visiting);
        }
        evaluate_expression(sheet, self, expression, visiting)
    }

    fn evaluate_function(
        &self,
        sheet: &Sheet,
        name: &str,
        args: &str,
        visiting: &mut HashSet<CellAddress>,
    ) -> CellValue {
        let parts = split_args(args);
        match name.to_ascii_uppercase().as_str() {
            "SUM" => aggregate_numeric(sheet, self, &parts, visiting, |values| values.iter().sum()),
            "AVERAGE" => {
                let values = collect_numeric(sheet, self, &parts, visiting);
                if values.is_empty() {
                    CellValue::Error("#DIV/0!".to_owned())
                } else {
                    CellValue::Number(values.iter().sum::<f64>() / values.len() as f64)
                }
            }
            "MIN" => aggregate_numeric(sheet, self, &parts, visiting, |values| {
                values.iter().copied().fold(f64::INFINITY, f64::min)
            }),
            "MAX" => aggregate_numeric(sheet, self, &parts, visiting, |values| {
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            }),
            "COUNT" => {
                let count = collect_numeric(sheet, self, &parts, visiting).len();
                CellValue::Number(count as f64)
            }
            "COUNTA" => {
                let count = collect_any(sheet, self, &parts, visiting)
                    .into_iter()
                    .filter(|value| !matches!(value, CellValue::Empty))
                    .count();
                CellValue::Number(count as f64)
            }
            "IF" => {
                if parts.len() < 2 {
                    return CellValue::Error("#VALUE!".to_owned());
                }
                let condition = evaluate_expression(sheet, self, parts[0], visiting);
                let truthy = condition.as_number().is_some_and(|value| value != 0.0)
                    || matches!(condition, CellValue::Boolean(true))
                    || matches!(condition, CellValue::Text(text) if !text.is_empty());
                let branch = if truthy {
                    parts[1]
                } else {
                    parts.get(2).copied().unwrap_or("")
                };
                if let Some(formula) = branch.strip_prefix('=') {
                    self.evaluate_formula(sheet, formula, visiting)
                } else {
                    evaluate_expression(sheet, self, branch, visiting)
                }
            }
            "AND" => CellValue::Boolean(
                parts
                    .iter()
                    .all(|part| is_truthy(evaluate_expression(sheet, self, part, visiting))),
            ),
            "OR" => CellValue::Boolean(
                parts
                    .iter()
                    .any(|part| is_truthy(evaluate_expression(sheet, self, part, visiting))),
            ),
            "NOT" => {
                let value = parts
                    .first()
                    .map(|part| evaluate_expression(sheet, self, part, visiting))
                    .unwrap_or(CellValue::Error("#VALUE!".to_owned()));
                CellValue::Boolean(!is_truthy(value))
            }
            "ABS" => unary_number(
                sheet,
                self,
                parts.first().copied().unwrap_or(""),
                visiting,
                f64::abs,
            ),
            "ROUND" => {
                let number = parts
                    .first()
                    .map(|part| evaluate_expression(sheet, self, part, visiting))
                    .and_then(|value| value.as_number())
                    .unwrap_or(f64::NAN);
                let digits = parts
                    .get(1)
                    .map(|part| evaluate_expression(sheet, self, part, visiting))
                    .and_then(|value| value.as_number())
                    .unwrap_or(0.0) as i32;
                if number.is_nan() {
                    CellValue::Error("#VALUE!".to_owned())
                } else {
                    CellValue::Number(round_to(number, digits))
                }
            }
            "CONCAT" | "CONCATENATE" => {
                let text = parts
                    .iter()
                    .map(|part| evaluate_expression(sheet, self, part, visiting).as_text())
                    .collect::<Vec<_>>()
                    .join("");
                CellValue::Text(text)
            }
            "LEN" => CellValue::Number(
                parts
                    .first()
                    .map(|part| evaluate_expression(sheet, self, part, visiting).as_text())
                    .unwrap_or_default()
                    .chars()
                    .count() as f64,
            ),
            "UPPER" => text_transform(
                sheet,
                self,
                parts.first().copied().unwrap_or(""),
                visiting,
                |text| text.to_ascii_uppercase(),
            ),
            "LOWER" => text_transform(
                sheet,
                self,
                parts.first().copied().unwrap_or(""),
                visiting,
                |text| text.to_ascii_lowercase(),
            ),
            _ => CellValue::Error("#NAME?".to_owned()),
        }
    }
}

impl CellAddress {
    pub fn to_a1(self) -> String {
        format!("{}{}", column_label(self.col), self.row + 1)
    }

    pub fn from_a1(value: &str) -> Result<Self, String> {
        let value = value.trim().to_ascii_uppercase();
        let split = value
            .char_indices()
            .find(|(_, ch)| ch.is_ascii_digit())
            .ok_or_else(|| format!("invalid cell address {value}"))?;
        let (col_part, row_part) = value.split_at(split.0);
        if col_part.is_empty() || row_part.is_empty() {
            return Err(format!("invalid cell address {value}"));
        }
        if !col_part.chars().all(|ch| ch.is_ascii_uppercase()) {
            return Err(format!("invalid cell address {value}"));
        }
        let row = row_part
            .parse::<u32>()
            .map_err(|_| format!("invalid cell address {value}"))?
            .checked_sub(1)
            .ok_or_else(|| format!("invalid cell address {value}"))?;
        let mut col = 0u32;
        for ch in col_part.chars() {
            col = col
                .checked_mul(26)
                .and_then(|value| value.checked_add(ch as u32 - 'A' as u32 + 1))
                .ok_or_else(|| format!("invalid cell address {value}"))?;
        }
        col = col
            .checked_sub(1)
            .ok_or_else(|| format!("invalid cell address {value}"))?;
        Ok(Self { row, col })
    }

    pub fn offset(self, row_delta: i32, col_delta: i32) -> Option<Self> {
        let row = self.row.checked_add_signed(row_delta)?;
        let col = self.col.checked_add_signed(col_delta)?;
        Some(Self { row, col })
    }
}

pub fn column_label(mut col: u32) -> String {
    let mut label = String::new();
    loop {
        label.insert(0, (b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    label
}

fn data_to_string(data: Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(value) => value,
        Data::Float(value) => format_number(value),
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::Error(_) => "#VALUE!".to_owned(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) => value,
        Data::DurationIso(value) => value,
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{}", value.round() as i64)
    } else {
        format!("{value}")
    }
}

fn parse_function_call(expression: &str) -> Option<(String, String)> {
    let open = expression.find('(')?;
    if open == 0 {
        return None;
    }
    let name = expression[..open].trim();
    if !name.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '_') {
        return None;
    }
    if !expression.ends_with(')') {
        return None;
    }
    Some((
        name.to_owned(),
        expression[open + 1..expression.len() - 1].to_owned(),
    ))
}

fn split_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    for (index, ch) in args.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            ';' | ',' if !in_string && depth == 0 => {
                parts.push(args[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if start <= args.len() {
        parts.push(args[start..].trim());
    }
    parts.retain(|part| !part.is_empty());
    parts
}

fn collect_references<'a>(
    sheet: &'a Sheet,
    layer: &'a SpreadsheetLayer,
    part: &str,
    visiting: &mut HashSet<CellAddress>,
) -> Vec<CellValue> {
    if let Ok(range) = CellRange::from_a1(part) {
        range
            .iter()
            .map(|address| layer.evaluate_cell(sheet, address, visiting))
            .collect()
    } else if let Ok(address) = CellAddress::from_a1(part) {
        vec![layer.evaluate_cell(sheet, address, visiting)]
    } else {
        vec![evaluate_expression(sheet, layer, part, visiting)]
    }
}

fn collect_numeric(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    parts: &[&str],
    visiting: &mut HashSet<CellAddress>,
) -> Vec<f64> {
    parts
        .iter()
        .flat_map(|part| collect_references(sheet, layer, part, visiting))
        .filter_map(|value| value.as_number())
        .collect()
}

fn collect_any(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    parts: &[&str],
    visiting: &mut HashSet<CellAddress>,
) -> Vec<CellValue> {
    parts
        .iter()
        .flat_map(|part| collect_references(sheet, layer, part, visiting))
        .collect()
}

fn aggregate_numeric<F>(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    parts: &[&str],
    visiting: &mut HashSet<CellAddress>,
    fold: F,
) -> CellValue
where
    F: FnOnce(&[f64]) -> f64,
{
    let values = collect_numeric(sheet, layer, parts, visiting);
    if values.is_empty() {
        CellValue::Number(0.0)
    } else {
        CellValue::Number(fold(&values))
    }
}

fn unary_number<F>(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    part: &str,
    visiting: &mut HashSet<CellAddress>,
    op: F,
) -> CellValue
where
    F: FnOnce(f64) -> f64,
{
    let value = evaluate_expression(sheet, layer, part, visiting);
    value
        .as_number()
        .map(op)
        .map(CellValue::Number)
        .unwrap_or(CellValue::Error("#VALUE!".to_owned()))
}

fn text_transform<F>(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    part: &str,
    visiting: &mut HashSet<CellAddress>,
    transform: F,
) -> CellValue
where
    F: FnOnce(String) -> String,
{
    CellValue::Text(transform(
        evaluate_expression(sheet, layer, part, visiting).as_text(),
    ))
}

fn is_truthy(value: CellValue) -> bool {
    match value {
        CellValue::Empty => false,
        CellValue::Boolean(value) => value,
        CellValue::Number(value) => value != 0.0,
        CellValue::Text(text) => !text.is_empty(),
        CellValue::Error(_) => false,
    }
}

fn round_to(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (value * factor).round() / factor
}

#[derive(Clone, Copy, Debug)]
struct CellRange {
    start: CellAddress,
    end: CellAddress,
}

impl CellRange {
    fn from_a1(value: &str) -> Result<Self, String> {
        let (start, end) = value
            .split_once(':')
            .ok_or_else(|| format!("invalid range {value}"))?;
        Ok(Self {
            start: CellAddress::from_a1(start)?,
            end: CellAddress::from_a1(end)?,
        })
    }

    fn iter(self) -> impl Iterator<Item = CellAddress> {
        let start_row = self.start.row.min(self.end.row);
        let end_row = self.start.row.max(self.end.row);
        let start_col = self.start.col.min(self.end.col);
        let end_col = self.start.col.max(self.end.col);
        (start_row..=end_row)
            .flat_map(move |row| (start_col..=end_col).map(move |col| CellAddress { row, col }))
    }
}

fn evaluate_expression(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    expression: &str,
    visiting: &mut HashSet<CellAddress>,
) -> CellValue {
    let expression = expression.trim();
    if expression.is_empty() {
        return CellValue::Empty;
    }
    if let Ok(address) = CellAddress::from_a1(expression) {
        return layer.evaluate_cell(sheet, address, visiting);
    }
    if (expression.starts_with('"') && expression.ends_with('"'))
        || (expression.starts_with('\'') && expression.ends_with('\''))
    {
        return CellValue::Text(expression[1..expression.len() - 1].to_owned());
    }
    if let Ok(number) = expression.parse::<f64>() {
        return CellValue::Number(number);
    }
    if let Some(index) = find_lowest_operator(expression, &['+', '-']) {
        let (left, right) = expression.split_at(index);
        let op = expression.as_bytes()[index] as char;
        return apply_binary(sheet, layer, left.trim(), right[1..].trim(), op, visiting);
    }
    if let Some(index) = find_lowest_operator(expression, &['*', '/']) {
        let (left, right) = expression.split_at(index);
        let op = expression.as_bytes()[index] as char;
        return apply_binary(sheet, layer, left.trim(), right[1..].trim(), op, visiting);
    }
    if let Some(index) = find_lowest_operator(expression, &['&']) {
        let (left, right) = expression.split_at(index);
        return CellValue::Text(format!(
            "{}{}",
            evaluate_expression(sheet, layer, left.trim(), visiting).as_text(),
            evaluate_expression(sheet, layer, &right[1..], visiting).as_text()
        ));
    }
    if let Some((name, args)) = parse_function_call(expression) {
        return layer.evaluate_function(sheet, &name, &args, visiting);
    }
    CellValue::Text(expression.to_owned())
}

fn find_lowest_operator(expression: &str, operators: &[char]) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    for (index, ch) in expression.char_indices().rev() {
        match ch {
            '"' => in_string = !in_string,
            ')' if !in_string => depth += 1,
            '(' if !in_string => depth -= 1,
            _ if !in_string && depth == 0 && operators.contains(&ch) => {
                if ch == '-' && index == 0 {
                    continue;
                }
                if ch == '-'
                    && expression[..index]
                        .chars()
                        .last()
                        .is_some_and(|previous| "+-*/&(".contains(previous))
                {
                    continue;
                }
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

fn apply_binary(
    sheet: &Sheet,
    layer: &SpreadsheetLayer,
    left: &str,
    right: &str,
    op: char,
    visiting: &mut HashSet<CellAddress>,
) -> CellValue {
    let left_value = evaluate_expression(sheet, layer, left, visiting);
    let right_value = evaluate_expression(sheet, layer, right, visiting);
    match op {
        '&' => CellValue::Text(format!("{}{}", left_value.as_text(), right_value.as_text())),
        '+' | '-' | '*' | '/' => {
            let left = left_value.as_number();
            let right = right_value.as_number();
            match (left, right) {
                (Some(left), Some(right)) => match op {
                    '+' => CellValue::Number(left + right),
                    '-' => CellValue::Number(left - right),
                    '*' => CellValue::Number(left * right),
                    '/' if right == 0.0 => CellValue::Error("#DIV/0!".to_owned()),
                    '/' => CellValue::Number(left / right),
                    _ => CellValue::Error("#VALUE!".to_owned()),
                },
                _ => CellValue::Error("#VALUE!".to_owned()),
            }
        }
        _ => CellValue::Error("#VALUE!".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_labels_match_excel() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
    }

    #[test]
    fn formulas_evaluate_basic_math_and_references() {
        let mut sheet = SpreadsheetLayer::default();
        sheet.set_raw(CellAddress { row: 0, col: 0 }, "10");
        sheet.set_raw(CellAddress { row: 1, col: 0 }, "20");
        sheet.set_raw(CellAddress { row: 2, col: 0 }, "=A1+A2");
        sheet.set_raw(CellAddress { row: 3, col: 0 }, "=SUM(A1:A2)");
        assert_eq!(sheet.display_text(CellAddress { row: 2, col: 0 }), "30");
        assert_eq!(sheet.display_text(CellAddress { row: 3, col: 0 }), "30");
    }

    #[test]
    fn xlsx_roundtrip_preserves_values() {
        let mut spreadsheet = SpreadsheetLayer::default();
        spreadsheet.set_raw(CellAddress { row: 0, col: 0 }, "Item");
        spreadsheet.set_raw(CellAddress { row: 0, col: 1 }, "15");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sheet.xlsx");
        spreadsheet.export_xlsx(&path).unwrap();
        let imported = SpreadsheetLayer::from_xlsx(&path).unwrap();
        assert_eq!(
            imported.get_raw(CellAddress { row: 0, col: 0 }),
            Some("Item")
        );
        assert_eq!(imported.get_raw(CellAddress { row: 0, col: 1 }), Some("15"));
    }

    #[test]
    fn if_function_branches() {
        let mut sheet = SpreadsheetLayer::default();
        sheet.set_raw(CellAddress { row: 0, col: 0 }, "5");
        sheet.set_raw(CellAddress { row: 0, col: 1 }, "=IF(A1>3,\"yes\",\"no\")");
        assert_eq!(sheet.display_text(CellAddress { row: 0, col: 1 }), "yes");
    }
}
