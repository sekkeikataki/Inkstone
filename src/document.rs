use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

pub const FORMAT_NAME: &str = "inkstone.document";
pub const FORMAT_VERSION: u32 = 1;
/// PostScript points per millimetre (72 pt/in, ISO 216).
pub const PT_PER_MM: f32 = 72.0 / 25.4;
pub const MIN_STROKE_WIDTH: f32 = 0.13 * PT_PER_MM;
pub const MAX_STROKE_WIDTH: f32 = 70.0 * PT_PER_MM;
pub const ISO_LINE_WIDTHS_MM: [f32; 6] = [0.25, 0.35, 0.5, 0.7, 1.0, 1.4];

pub fn mm_to_pt(mm: f32) -> f32 {
    mm * PT_PER_MM
}

pub fn pt_to_mm(pt: f32) -> f32 {
    pt / PT_PER_MM
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("could not access document: {0}")]
    Io(#[from] std::io::Error),
    #[error("document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported document format: {0}")]
    Unsupported(String),
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("export failed: {0}")]
    Export(String),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub fn rotate_around(self, center: Self, degrees: f32) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let dx = self.x - center.x;
        let dy = self.y - center.y;
        Self::new(
            center.x + dx * cos - dy * sin,
            center.y + dx * sin + dy * cos,
        )
    }

    pub fn scale_from(self, origin: Self, scale_x: f32, scale_y: f32) -> Self {
        Self::new(
            origin.x + (self.x - origin.x) * scale_x,
            origin.y + (self.y - origin.y) * scale_y,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn from_points(a: Point, b: Point) -> Self {
        Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        }
    }

    pub fn expand(self, amount: f32) -> Self {
        let rect = self.normalized();
        Self {
            x: rect.x - amount,
            y: rect.y - amount,
            width: rect.width + amount * 2.0,
            height: rect.height + amount * 2.0,
        }
    }

    pub fn contains(self, point: Point) -> bool {
        let rect = self.normalized();
        point.x >= rect.x
            && point.x <= rect.x + rect.width
            && point.y >= rect.y
            && point.y <= rect.y + rect.height
    }

    pub fn intersects(self, other: Self) -> bool {
        let a = self.normalized();
        let b = other.normalized();
        a.x <= b.x + b.width
            && a.x + a.width >= b.x
            && a.y <= b.y + b.height
            && a.y + a.height >= b.y
    }

    pub fn union(self, other: Self) -> Self {
        let a = self.normalized();
        let b = other.normalized();
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let right = (a.x + a.width).max(b.x + b.width);
        let bottom = (a.y + a.height).max(b.y + b.height);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    pub fn center(self) -> Point {
        let rect = self.normalized();
        Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
    }

    pub fn from_drag(start: Point, end: Point) -> Self {
        Self {
            x: start.x,
            y: start.y,
            width: end.x - start.x,
            height: end.y - start.y,
        }
    }

    pub fn start(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn end(self) -> Point {
        Point::new(self.x + self.width, self.y + self.height)
    }

    pub fn normalized(self) -> Self {
        let (x, width) = if self.width < 0.0 {
            (self.x + self.width, -self.width)
        } else {
            (self.x, self.width)
        };
        let (y, height) = if self.height < 0.0 {
            (self.y + self.height, -self.height)
        } else {
            (self.y, self.height)
        };
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Color {
    pub const INK: Self = Self::rgb(0.10, 0.12, 0.16);
    pub const BLUE: Self = Self::rgb(0.12, 0.38, 0.88);
    pub const PAPER: Self = Self::rgb(0.98, 0.98, 0.97);
    pub const NIGHT: Self = Self::rgb(0.10, 0.12, 0.16);
    pub const NIGHT_INK: Self = Self::rgb(0.92, 0.93, 0.90);

    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    pub fn is_valid(self) -> bool {
        [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }

    pub(crate) fn svg(self) -> String {
        format!(
            "rgba({},{},{},{:.3})",
            (self.red * 255.0).round() as u8,
            (self.green * 255.0).round() as u8,
            (self.blue * 255.0).round() as u8,
            self.alpha
        )
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            let (red, green, blue, alpha) = match hex.len() {
                3 => (
                    u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
                    u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
                    u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
                    255u8,
                ),
                6 => (
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                    255u8,
                ),
                8 => (
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                    u8::from_str_radix(&hex[6..8], 16).ok()?,
                ),
                _ => return None,
            };
            return Some(Self {
                red: red as f32 / 255.0,
                green: green as f32 / 255.0,
                blue: blue as f32 / 255.0,
                alpha: alpha as f32 / 255.0,
            });
        }
        let lower = value.to_ascii_lowercase();
        let (channels, body) = if let Some(body) = lower
            .strip_prefix("rgba(")
            .and_then(|body| body.strip_suffix(')'))
        {
            (4, body)
        } else {
            let body = lower
                .strip_prefix("rgb(")
                .and_then(|body| body.strip_suffix(')'))?;
            (3, body)
        };
        let mut parts = body.split(',').map(str::trim);
        let red = parse_css_channel(parts.next()?)?;
        let green = parse_css_channel(parts.next()?)?;
        let blue = parse_css_channel(parts.next()?)?;
        let alpha = if channels == 4 {
            parts.next()?.parse::<f32>().ok()?.clamp(0.0, 1.0)
        } else {
            1.0
        };
        Some(Self {
            red,
            green,
            blue,
            alpha,
        })
    }
}

fn parse_css_channel(value: &str) -> Option<f32> {
    if let Some(percent) = value.strip_suffix('%') {
        Some((percent.parse::<f32>().ok()? / 100.0).clamp(0.0, 1.0))
    } else {
        Some((value.parse::<f32>().ok()? / 255.0).clamp(0.0, 1.0))
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::INK
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundPattern {
    None,
    #[default]
    Grid,
    Dots,
    Lines,
}

impl BackgroundPattern {
    pub const ALL: [Self; 4] = [Self::None, Self::Grid, Self::Dots, Self::Lines];
    pub const NAMES: [&'static str; 4] = ["Plain", "Grid", "Dots", "Lined"];
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageLayout {
    #[default]
    Infinite,
    Fixed,
    ContinuousVertical,
}

impl PageLayout {
    pub const ALL: [Self; 3] = [Self::Infinite, Self::Fixed, Self::ContinuousVertical];
    pub const NAMES: [&'static str; 3] = ["Infinite", "Sheet", "Vertical"];
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSize {
    Infinite,
    A5,
    #[default]
    A4,
    A3,
    A2,
}

impl PaperSize {
    pub const ALL: [Self; 5] = [Self::Infinite, Self::A5, Self::A4, Self::A3, Self::A2];
    pub const NAMES: [&'static str; 5] = [
        "Infinite",
        "A5 · 148×210 mm",
        "A4 · 210×297 mm",
        "A3 · 297×420 mm",
        "A2 · 420×594 mm",
    ];

    pub fn dimensions_mm(self) -> Option<(f32, f32)> {
        match self {
            Self::Infinite => None,
            Self::A5 => Some((148.0, 210.0)),
            Self::A4 => Some((210.0, 297.0)),
            Self::A3 => Some((297.0, 420.0)),
            Self::A2 => Some((420.0, 594.0)),
        }
    }

    pub fn dimensions_pt(self) -> Option<(f32, f32)> {
        self.dimensions_mm()
            .map(|(width, height)| (mm_to_pt(width), mm_to_pt(height)))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanvasSettings {
    pub background: Color,
    pub grid_spacing: f32,
    pub grid_visible: bool,
    #[serde(default)]
    pub pattern: Option<BackgroundPattern>,
    #[serde(default)]
    pub layout: PageLayout,
    #[serde(default = "default_page_width")]
    pub page_width: f32,
    #[serde(default = "default_page_height")]
    pub page_height: f32,
}

fn default_page_width() -> f32 {
    mm_to_pt(210.0)
}

fn default_page_height() -> f32 {
    mm_to_pt(297.0)
}

impl Default for CanvasSettings {
    fn default() -> Self {
        Self {
            background: Color::PAPER,
            grid_spacing: mm_to_pt(5.0),
            grid_visible: true,
            pattern: Some(BackgroundPattern::Grid),
            layout: PageLayout::Infinite,
            page_width: default_page_width(),
            page_height: default_page_height(),
        }
    }
}

impl CanvasSettings {
    pub fn pattern(&self) -> BackgroundPattern {
        self.pattern.unwrap_or(if self.grid_visible {
            BackgroundPattern::Grid
        } else {
            BackgroundPattern::None
        })
    }

    pub fn set_pattern(&mut self, pattern: BackgroundPattern) {
        self.pattern = Some(pattern);
        self.grid_visible = matches!(pattern, BackgroundPattern::Grid);
    }

    pub fn paper_size(&self) -> PaperSize {
        if self.layout == PageLayout::Infinite {
            return PaperSize::Infinite;
        }
        let width = pt_to_mm(self.page_width);
        let height = pt_to_mm(self.page_height);
        for size in [PaperSize::A5, PaperSize::A4, PaperSize::A3, PaperSize::A2] {
            if let Some((mm_w, mm_h)) = size.dimensions_mm()
                && (width - mm_w).abs() < 2.0
                && (height - mm_h).abs() < 2.0
            {
                return size;
            }
        }
        PaperSize::A4
    }

    pub fn set_paper_size(&mut self, size: PaperSize) {
        match size.dimensions_pt() {
            None => self.layout = PageLayout::Infinite,
            Some((width, height)) => {
                if self.layout == PageLayout::Infinite {
                    self.layout = PageLayout::Fixed;
                }
                self.page_width = width;
                self.page_height = height;
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f32,
    #[serde(default)]
    pub dashed: bool,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: Color::INK,
            width: mm_to_pt(0.5),
            dashed: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeKind {
    Pen,
    Highlighter,
    Brush,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

impl StrokePoint {
    pub fn new(point: Point, pressure: f32) -> Self {
        Self {
            x: point.x,
            y: point.y,
            pressure: pressure.clamp(0.0, 1.0),
        }
    }

    pub fn point(self) -> Point {
        Point::new(self.x, self.y)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Stroke {
    pub id: Uuid,
    pub kind: StrokeKind,
    pub style: StrokeStyle,
    pub points: Vec<StrokePoint>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Triangle,
    Resistor,
    Capacitor,
    Diode,
    Inductor,
    Switch,
    Fuse,
    Battery,
    Ground,
    Lamp,
    Transformer,
    AndGate,
    OrGate,
    NotGate,
    NandGate,
    NorGate,
    XorGate,
    Motor,
    Gear,
    Bearing,
    Spring,
    Beam,
    Dimension,
    SurfaceFinish,
    ThirdAngle,
}

impl ShapeKind {
    pub const ALL: [Self; 29] = [
        Self::Rectangle,
        Self::Ellipse,
        Self::Line,
        Self::Arrow,
        Self::Triangle,
        Self::Resistor,
        Self::Capacitor,
        Self::Diode,
        Self::Inductor,
        Self::Switch,
        Self::Fuse,
        Self::Battery,
        Self::Ground,
        Self::Lamp,
        Self::Transformer,
        Self::AndGate,
        Self::OrGate,
        Self::NotGate,
        Self::NandGate,
        Self::NorGate,
        Self::XorGate,
        Self::Motor,
        Self::Gear,
        Self::Bearing,
        Self::Spring,
        Self::Beam,
        Self::Dimension,
        Self::SurfaceFinish,
        Self::ThirdAngle,
    ];

    pub const NAMES: [&'static str; 29] = [
        "Rectangle",
        "Ellipse",
        "Line",
        "Arrow",
        "Triangle",
        "Resistor IEC",
        "Capacitor",
        "Diode",
        "Inductor",
        "Switch",
        "Fuse",
        "Battery",
        "Earth",
        "Lamp",
        "Transformer",
        "AND",
        "OR",
        "NOT",
        "NAND",
        "NOR",
        "XOR",
        "Motor",
        "Gear",
        "Bearing",
        "Spring",
        "Beam",
        "Dimension",
        "Surface finish",
        "Third-angle",
    ];

    pub const SHORT: [&'static str; 29] = [
        "Rect", "Ellipse", "Line", "Arrow", "Tri", "R", "C", "D", "L", "S", "Fuse", "Batt", "PE",
        "Lamp", "Tr", "AND", "OR", "NOT", "NAND", "NOR", "XOR", "M", "Gear", "Brg", "Spring",
        "Beam", "Dim", "Ra", "3rd",
    ];

    pub fn group(self) -> &'static str {
        match self {
            Self::Rectangle | Self::Ellipse | Self::Line | Self::Arrow | Self::Triangle => {
                "Geometry"
            }
            Self::Resistor
            | Self::Capacitor
            | Self::Diode
            | Self::Inductor
            | Self::Switch
            | Self::Fuse
            | Self::Battery
            | Self::Ground
            | Self::Lamp
            | Self::Transformer => "IEC 60617",
            Self::AndGate
            | Self::OrGate
            | Self::NotGate
            | Self::NandGate
            | Self::NorGate
            | Self::XorGate => "Logic",
            Self::Motor
            | Self::Gear
            | Self::Bearing
            | Self::Spring
            | Self::Beam
            | Self::Dimension
            | Self::SurfaceFinish
            | Self::ThirdAngle => "ISO 128/129",
        }
    }

    pub fn short_name(self) -> &'static str {
        Self::SHORT[Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)]
    }

    pub fn uses_drag_bounds(self) -> bool {
        matches!(self, Self::Line | Self::Arrow | Self::Dimension)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Shape {
    pub id: Uuid,
    pub kind: ShapeKind,
    pub bounds: Rect,
    pub rotation_degrees: f32,
    pub style: StrokeStyle,
    pub fill: Option<Color>,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    North,
    East,
    South,
    West,
    Center,
    Start,
    End,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Attachment {
    pub element_id: Uuid,
    pub anchor: Anchor,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Endpoint {
    pub point: Point,
    pub attachment: Option<Attachment>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Connector {
    pub id: Uuid,
    pub start: Endpoint,
    pub end: Endpoint,
    pub route: Vec<Point>,
    pub style: StrokeStyle,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ListStyle {
    #[default]
    None,
    Bullet,
    Numbered,
    Checklist,
}

impl ListStyle {
    pub const ALL: [Self; 4] = [Self::None, Self::Bullet, Self::Numbered, Self::Checklist];
    pub const NAMES: [&'static str; 4] = ["Plain", "Bullets", "Numbered", "To-do"];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextNote {
    pub id: Uuid,
    pub origin: Point,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub max_width: Option<f32>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub highlight: Option<Color>,
    #[serde(default)]
    pub list: ListStyle,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub checked: bool,
}

impl TextNote {
    pub fn plain(origin: Point, text: impl Into<String>, font_size: f32, color: Color) -> Self {
        Self {
            id: Uuid::new_v4(),
            origin,
            text: text.into(),
            font_size,
            color,
            max_width: None,
            bold: false,
            italic: false,
            underline: false,
            highlight: None,
            list: ListStyle::None,
            href: None,
            checked: false,
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self.list {
            ListStyle::None => "",
            ListStyle::Bullet => "• ",
            ListStyle::Numbered => "1. ",
            ListStyle::Checklist => {
                if self.checked {
                    "☑ "
                } else {
                    "☐ "
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    ToDo,
    Important,
    Question,
    Idea,
    Critical,
    Definition,
    Contact,
    Address,
    Phone,
    Date,
}

impl TagKind {
    pub const ALL: [Self; 10] = [
        Self::ToDo,
        Self::Important,
        Self::Question,
        Self::Idea,
        Self::Critical,
        Self::Definition,
        Self::Contact,
        Self::Address,
        Self::Phone,
        Self::Date,
    ];
    pub const NAMES: [&'static str; 10] = [
        "To-do",
        "Important",
        "Question",
        "Idea",
        "Critical",
        "Definition",
        "Contact",
        "Address",
        "Phone",
        "Date",
    ];

    pub fn label(self) -> &'static str {
        Self::NAMES[Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)]
    }

    pub fn color(self) -> Color {
        match self {
            Self::ToDo => Color::BLUE,
            Self::Important | Self::Critical => Color::rgb(0.84, 0.16, 0.20),
            Self::Question => Color::rgb(0.48, 0.20, 0.78),
            Self::Idea => Color::rgb(0.95, 0.62, 0.05),
            Self::Definition => Color::rgb(0.05, 0.56, 0.32),
            Self::Contact => Color::rgb(0.12, 0.38, 0.88),
            Self::Address => Color::rgb(0.05, 0.60, 0.66),
            Self::Phone => Color::rgb(0.05, 0.56, 0.32),
            Self::Date => Color::rgb(0.54, 0.35, 0.20),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TagElement {
    pub id: Uuid,
    pub origin: Point,
    pub kind: TagKind,
    pub note: String,
    pub checked: bool,
}

impl TagElement {
    pub fn size(&self) -> (f32, f32) {
        let width = (self.note.chars().count() as f32 * 8.2 + 54.0).max(108.0);
        (width, 28.0)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TableElement {
    pub id: Uuid,
    pub bounds: Rect,
    pub columns: u32,
    pub rows: u32,
    pub cells: Vec<String>,
}

impl TableElement {
    pub fn new(origin: Point, columns: u32, rows: u32) -> Self {
        let columns = columns.max(1);
        let rows = rows.max(1);
        Self {
            id: Uuid::new_v4(),
            bounds: Rect {
                x: origin.x,
                y: origin.y,
                width: 96.0 * columns as f32,
                height: 32.0 * rows as f32,
            },
            columns,
            rows,
            cells: vec![String::new(); (columns * rows) as usize],
        }
    }

    pub fn cell_size(&self) -> (f32, f32) {
        (
            self.bounds.width / self.columns.max(1) as f32,
            self.bounds.height / self.rows.max(1) as f32,
        )
    }

    pub fn cell_index(&self, point: Point) -> Option<usize> {
        let bounds = self.bounds.normalized();
        if !bounds.contains(point) || self.columns == 0 || self.rows == 0 {
            return None;
        }
        let (cell_w, cell_h) = self.cell_size();
        if cell_w <= 0.0 || cell_h <= 0.0 {
            return None;
        }
        let column = ((point.x - bounds.x) / cell_w).floor() as u32;
        let row = ((point.y - bounds.y) / cell_h).floor() as u32;
        if column >= self.columns || row >= self.rows {
            return None;
        }
        Some((row * self.columns + column) as usize)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Pdf,
    File,
    Audio,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MediaElement {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub kind: MediaKind,
    pub bounds: Rect,
    pub alt_text: String,
    pub caption: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Stroke(Stroke),
    Text(TextNote),
    Shape(Shape),
    Connector(Connector),
    Media(MediaElement),
    Table(TableElement),
    Tag(TagElement),
}

impl Element {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Stroke(value) => value.id,
            Self::Text(value) => value.id,
            Self::Shape(value) => value.id,
            Self::Connector(value) => value.id,
            Self::Media(value) => value.id,
            Self::Table(value) => value.id,
            Self::Tag(value) => value.id,
        }
    }

    pub fn set_id(&mut self, id: Uuid) {
        match self {
            Self::Stroke(value) => value.id = id,
            Self::Text(value) => value.id = id,
            Self::Shape(value) => value.id = id,
            Self::Connector(value) => value.id = id,
            Self::Media(value) => value.id = id,
            Self::Table(value) => value.id = id,
            Self::Tag(value) => value.id = id,
        }
    }

    pub fn bounds(&self) -> Rect {
        match self {
            Self::Stroke(stroke) => {
                let mut points = stroke.points.iter();
                let Some(first) = points.next() else {
                    return Rect::default();
                };
                let mut bounds = Rect {
                    x: first.x,
                    y: first.y,
                    width: 0.0,
                    height: 0.0,
                };
                for point in points {
                    bounds = bounds.union(Rect {
                        x: point.x,
                        y: point.y,
                        width: 0.0,
                        height: 0.0,
                    });
                }
                bounds.expand(stroke.style.width)
            }
            Self::Text(text) => {
                let prefix = text.prefix().chars().count() as f32;
                let longest = text
                    .text
                    .lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(1) as f32
                    + prefix;
                let lines = text.text.lines().count().max(1) as f32;
                Rect {
                    x: text.origin.x,
                    y: text.origin.y - text.font_size,
                    width: text
                        .max_width
                        .unwrap_or((longest * text.font_size * 0.62).max(text.font_size)),
                    height: lines * text.font_size * 1.25,
                }
            }
            Self::Shape(shape) => shape.bounds.normalized().expand(shape.style.width + 4.0),
            Self::Connector(connector) => {
                let mut bounds = Rect::from_points(connector.start.point, connector.end.point);
                for point in &connector.route {
                    bounds = bounds.union(Rect {
                        x: point.x,
                        y: point.y,
                        width: 0.0,
                        height: 0.0,
                    });
                }
                bounds.expand(connector.style.width + 4.0)
            }
            Self::Media(media) => media.bounds,
            Self::Table(table) => table.bounds,
            Self::Tag(tag) => {
                let (width, height) = tag.size();
                Rect {
                    x: tag.origin.x,
                    y: tag.origin.y,
                    width,
                    height,
                }
            }
        }
    }

    pub fn anchors(&self) -> Vec<(Anchor, Point)> {
        match self {
            Self::Stroke(stroke) => {
                let mut anchors = Vec::with_capacity(2);
                if let Some(first) = stroke.points.first() {
                    anchors.push((Anchor::Start, first.point()));
                }
                if let Some(last) = stroke.points.last() {
                    anchors.push((Anchor::End, last.point()));
                }
                anchors
            }
            Self::Connector(connector) => vec![
                (Anchor::Start, connector.start.point),
                (Anchor::End, connector.end.point),
            ],
            Self::Shape(shape) => cardinal_anchors(shape.bounds),
            Self::Text(_) | Self::Tag(_) => cardinal_anchors(self.bounds()),
            Self::Media(media) => cardinal_anchors(media.bounds),
            Self::Table(table) => cardinal_anchors(table.bounds),
        }
    }

    pub fn translate(&mut self, delta: Point) {
        match self {
            Self::Stroke(stroke) => {
                for point in &mut stroke.points {
                    point.x += delta.x;
                    point.y += delta.y;
                }
            }
            Self::Text(text) => {
                text.origin.x += delta.x;
                text.origin.y += delta.y;
            }
            Self::Shape(shape) => {
                shape.bounds.x += delta.x;
                shape.bounds.y += delta.y;
            }
            Self::Connector(connector) => {
                connector.start.point.x += delta.x;
                connector.start.point.y += delta.y;
                connector.end.point.x += delta.x;
                connector.end.point.y += delta.y;
                for point in &mut connector.route {
                    point.x += delta.x;
                    point.y += delta.y;
                }
            }
            Self::Media(media) => {
                media.bounds.x += delta.x;
                media.bounds.y += delta.y;
            }
            Self::Table(table) => {
                table.bounds.x += delta.x;
                table.bounds.y += delta.y;
            }
            Self::Tag(tag) => {
                tag.origin.x += delta.x;
                tag.origin.y += delta.y;
            }
        }
    }

    pub fn rotate_around(&mut self, center: Point, degrees: f32) {
        if degrees.abs() < f32::EPSILON {
            return;
        }
        match self {
            Self::Stroke(stroke) => {
                for point in &mut stroke.points {
                    let rotated = point.point().rotate_around(center, degrees);
                    point.x = rotated.x;
                    point.y = rotated.y;
                }
            }
            Self::Text(text) => {
                text.origin = text.origin.rotate_around(center, degrees);
            }
            Self::Shape(shape) => {
                let rotated = shape.bounds.center().rotate_around(center, degrees);
                shape.bounds.x = rotated.x - shape.bounds.width / 2.0;
                shape.bounds.y = rotated.y - shape.bounds.height / 2.0;
                shape.rotation_degrees = (shape.rotation_degrees + degrees).rem_euclid(360.0);
            }
            Self::Connector(connector) => {
                connector.start.point = connector.start.point.rotate_around(center, degrees);
                connector.end.point = connector.end.point.rotate_around(center, degrees);
                for point in &mut connector.route {
                    *point = point.rotate_around(center, degrees);
                }
            }
            Self::Media(media) => {
                let rotated = media.bounds.center().rotate_around(center, degrees);
                media.bounds.x = rotated.x - media.bounds.width / 2.0;
                media.bounds.y = rotated.y - media.bounds.height / 2.0;
            }
            Self::Table(table) => {
                let rotated = table.bounds.center().rotate_around(center, degrees);
                table.bounds.x = rotated.x - table.bounds.width / 2.0;
                table.bounds.y = rotated.y - table.bounds.height / 2.0;
            }
            Self::Tag(tag) => {
                tag.origin = tag.origin.rotate_around(center, degrees);
            }
        }
    }

    pub fn scale_from(&mut self, origin: Point, scale_x: f32, scale_y: f32) {
        let scale_x = if scale_x.abs() < 0.05 {
            0.05_f32.copysign(scale_x)
        } else {
            scale_x
        };
        let scale_y = if scale_y.abs() < 0.05 {
            0.05_f32.copysign(scale_y)
        } else {
            scale_y
        };
        let map = |point: Point| point.scale_from(origin, scale_x, scale_y);
        let width_scale = ((scale_x.abs() + scale_y.abs()) / 2.0).max(0.05);
        match self {
            Self::Stroke(stroke) => {
                for point in &mut stroke.points {
                    let scaled = map(point.point());
                    point.x = scaled.x;
                    point.y = scaled.y;
                }
                stroke.style.width =
                    (stroke.style.width * width_scale).clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH);
            }
            Self::Text(text) => {
                text.origin = map(text.origin);
                text.font_size = (text.font_size * scale_y.abs()).max(4.0);
                if let Some(width) = text.max_width.as_mut() {
                    *width = (*width * scale_x.abs()).max(8.0);
                }
            }
            Self::Shape(shape) => {
                let start = map(Point::new(shape.bounds.x, shape.bounds.y));
                let end = map(Point::new(
                    shape.bounds.x + shape.bounds.width,
                    shape.bounds.y + shape.bounds.height,
                ));
                shape.bounds = Rect::from_points(start, end);
                shape.style.width =
                    (shape.style.width * width_scale).clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH);
            }
            Self::Connector(connector) => {
                connector.start.point = map(connector.start.point);
                connector.end.point = map(connector.end.point);
                for point in &mut connector.route {
                    *point = map(*point);
                }
                connector.style.width =
                    (connector.style.width * width_scale).clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH);
            }
            Self::Media(media) => {
                let start = map(Point::new(media.bounds.x, media.bounds.y));
                let end = map(Point::new(
                    media.bounds.x + media.bounds.width,
                    media.bounds.y + media.bounds.height,
                ));
                media.bounds = Rect::from_points(start, end);
                if media.bounds.width < 8.0 {
                    media.bounds.width = 8.0;
                }
                if media.bounds.height < 8.0 {
                    media.bounds.height = 8.0;
                }
            }
            Self::Table(table) => {
                let start = map(Point::new(table.bounds.x, table.bounds.y));
                let end = map(Point::new(
                    table.bounds.x + table.bounds.width,
                    table.bounds.y + table.bounds.height,
                ));
                table.bounds = Rect::from_points(start, end);
                if table.bounds.width < 48.0 {
                    table.bounds.width = 48.0;
                }
                if table.bounds.height < 24.0 {
                    table.bounds.height = 24.0;
                }
            }
            Self::Tag(tag) => {
                tag.origin = map(tag.origin);
            }
        }
    }

    pub fn searchable_text(&self) -> String {
        match self {
            Self::Stroke(_) => String::new(),
            Self::Text(text) => text.text.clone(),
            Self::Shape(shape) => shape.label.clone(),
            Self::Connector(connector) => connector.label.clone(),
            Self::Media(media) => {
                if media.caption.is_empty() {
                    media.alt_text.clone()
                } else {
                    media.caption.clone()
                }
            }
            Self::Table(table) => table.cells.join(" "),
            Self::Tag(tag) => format!("{} {}", tag.kind.label(), tag.note),
        }
    }
}

fn cardinal_anchors(bounds: Rect) -> Vec<(Anchor, Point)> {
    vec![
        (Anchor::North, Point::new(bounds.center().x, bounds.y)),
        (
            Anchor::East,
            Point::new(bounds.x + bounds.width, bounds.center().y),
        ),
        (
            Anchor::South,
            Point::new(bounds.center().x, bounds.y + bounds.height),
        ),
        (Anchor::West, Point::new(bounds.x, bounds.center().y)),
        (Anchor::Center, bounds.center()),
    ]
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Document {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub canvas: CanvasSettings,
    pub elements: Vec<Element>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format: FORMAT_NAME.to_owned(),
            version: FORMAT_VERSION,
            title: "Untitled note".to_owned(),
            canvas: CanvasSettings::default(),
            elements: Vec::new(),
        }
    }
}

impl Document {
    pub fn load(path: &Path) -> Result<Self, DocumentError> {
        let document: Self = serde_json::from_slice(&fs::read(path)?)?;
        document.validate()?;
        Ok(document)
    }

    pub fn save(&self, path: &Path) -> Result<(), DocumentError> {
        self.validate()?;
        let temporary = path.with_extension("inkstone.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn export_svg(&self, path: &Path) -> Result<(), DocumentError> {
        self.validate()?;
        fs::write(path, self.to_svg())?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.format != FORMAT_NAME {
            return Err(DocumentError::Unsupported(self.format.clone()));
        }
        if self.version != FORMAT_VERSION {
            return Err(DocumentError::Unsupported(format!(
                "version {} (supported: {})",
                self.version, FORMAT_VERSION
            )));
        }
        if !self.canvas.background.is_valid()
            || !self.canvas.grid_spacing.is_finite()
            || self.canvas.grid_spacing <= 0.0
            || !self.canvas.page_width.is_finite()
            || self.canvas.page_width <= 0.0
            || !self.canvas.page_height.is_finite()
            || self.canvas.page_height <= 0.0
        {
            return Err(DocumentError::Invalid(
                "canvas settings contain invalid values".to_owned(),
            ));
        }

        let mut ids = HashSet::with_capacity(self.elements.len());
        for element in &self.elements {
            if !ids.insert(element.id()) {
                return Err(DocumentError::Invalid(format!(
                    "duplicate element id {}",
                    element.id()
                )));
            }
            Self::validate_element(element)?;
        }

        for element in &self.elements {
            if let Element::Connector(connector) = element {
                for endpoint in [&connector.start, &connector.end] {
                    if let Some(attachment) = &endpoint.attachment
                        && !ids.contains(&attachment.element_id)
                    {
                        return Err(DocumentError::Invalid(format!(
                            "connector {} references missing element {}",
                            connector.id, attachment.element_id
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_element(element: &Element) -> Result<(), DocumentError> {
        let valid_style = |style: &StrokeStyle| {
            style.color.is_valid()
                && style.width.is_finite()
                && style.width > 0.0
                && style.width <= MAX_STROKE_WIDTH
        };
        match element {
            Element::Stroke(stroke) => {
                if stroke.points.is_empty()
                    || !valid_style(&stroke.style)
                    || stroke.points.iter().any(|point| {
                        !point.point().is_finite()
                            || !point.pressure.is_finite()
                            || !(0.0..=1.0).contains(&point.pressure)
                    })
                {
                    return Err(DocumentError::Invalid(format!(
                        "stroke {} has invalid geometry or style",
                        stroke.id
                    )));
                }
            }
            Element::Text(text) => {
                if !text.origin.is_finite()
                    || text.text.is_empty()
                    || !text.font_size.is_finite()
                    || text.font_size <= 0.0
                    || !text.color.is_valid()
                    || text
                        .max_width
                        .is_some_and(|width| !width.is_finite() || width <= 0.0)
                {
                    return Err(DocumentError::Invalid(format!(
                        "text {} has invalid content or style",
                        text.id
                    )));
                }
            }
            Element::Shape(shape) => {
                if !shape.bounds.is_finite()
                    || shape.bounds.width <= 0.0
                    || shape.bounds.height <= 0.0
                    || !shape.rotation_degrees.is_finite()
                    || !valid_style(&shape.style)
                    || shape.fill.is_some_and(|color| !color.is_valid())
                {
                    return Err(DocumentError::Invalid(format!(
                        "shape {} has invalid geometry or style",
                        shape.id
                    )));
                }
            }
            Element::Connector(connector) => {
                if !connector.start.point.is_finite()
                    || !connector.end.point.is_finite()
                    || connector.route.iter().any(|point| !point.is_finite())
                    || !valid_style(&connector.style)
                {
                    return Err(DocumentError::Invalid(format!(
                        "connector {} has invalid geometry or style",
                        connector.id
                    )));
                }
            }
            Element::Media(media) => {
                if !media.bounds.is_finite()
                    || media.bounds.width <= 0.0
                    || media.bounds.height <= 0.0
                {
                    return Err(DocumentError::Invalid(format!(
                        "media {} has invalid geometry",
                        media.id
                    )));
                }
            }
            Element::Table(table) => {
                if !table.bounds.is_finite()
                    || table.bounds.width <= 0.0
                    || table.bounds.height <= 0.0
                    || table.columns == 0
                    || table.rows == 0
                    || table.cells.len() != (table.columns * table.rows) as usize
                {
                    return Err(DocumentError::Invalid(format!(
                        "table {} has invalid geometry or cells",
                        table.id
                    )));
                }
            }
            Element::Tag(tag) => {
                if !tag.origin.is_finite() || tag.note.trim().is_empty() {
                    return Err(DocumentError::Invalid(format!(
                        "tag {} has invalid content",
                        tag.id
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn snap_endpoint(&self, target: Point, max_distance: f32) -> Endpoint {
        let mut best: Option<(f32, Uuid, Anchor, Point)> = None;
        for element in &self.elements {
            if matches!(element, Element::Connector(_)) {
                continue;
            }
            for (anchor, point) in element.anchors() {
                let distance = target.distance_to(point);
                if distance <= max_distance
                    && best
                        .as_ref()
                        .is_none_or(|(best_distance, ..)| distance < *best_distance)
                {
                    best = Some((distance, element.id(), anchor, point));
                }
            }
        }
        if let Some((_, element_id, anchor, point)) = best {
            Endpoint {
                point,
                attachment: Some(Attachment { element_id, anchor }),
            }
        } else {
            Endpoint {
                point: target,
                attachment: None,
            }
        }
    }

    pub fn to_svg(&self) -> String {
        let content_bounds = self
            .elements
            .iter()
            .map(Element::bounds)
            .reduce(Rect::union)
            .unwrap_or(Rect {
                x: -640.0,
                y: -360.0,
                width: 1280.0,
                height: 720.0,
            })
            .expand(32.0);
        let mut svg = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" \
             data-inkstone-format=\"{}\" data-inkstone-version=\"{}\">\n\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            content_bounds.x,
            content_bounds.y,
            content_bounds.width,
            content_bounds.height,
            FORMAT_NAME,
            FORMAT_VERSION,
            content_bounds.x,
            content_bounds.y,
            content_bounds.width,
            content_bounds.height,
            self.canvas.background.svg()
        );
        for element in &self.elements {
            svg.push_str(&element_svg(element));
        }
        svg.push_str("</svg>\n");
        svg
    }
}

pub(crate) fn element_svg(element: &Element) -> String {
    match element {
        Element::Stroke(stroke) => {
            let points = stroke
                .points
                .iter()
                .map(|point| format!("{},{}", point.x, point.y))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "<polyline data-inkstone-id=\"{}\" data-kind=\"stroke\" points=\"{}\" \
                 fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\"{}/>\n",
                stroke.id,
                points,
                stroke.style.color.svg(),
                stroke.style.width,
                dash_attr(stroke.style.dashed)
            )
        }
        Element::Text(text) => {
            let weight = if text.bold { "bold" } else { "normal" };
            let style = if text.italic { "italic" } else { "normal" };
            let decoration = if text.underline || text.href.is_some() {
                "underline"
            } else {
                "none"
            };
            text.text
                .lines()
                .enumerate()
                .map(|(index, line)| {
                    format!(
                        "<text data-inkstone-id=\"{}\" data-kind=\"text\" x=\"{}\" y=\"{}\" \
                         font-family=\"sans-serif\" font-size=\"{}\" font-weight=\"{weight}\" \
                         font-style=\"{style}\" text-decoration=\"{decoration}\" fill=\"{}\">{}{}</text>\n",
                        text.id,
                        text.origin.x,
                        text.origin.y + index as f32 * text.font_size * 1.25,
                        text.font_size,
                        text.color.svg(),
                        escape_xml(text.prefix()),
                        escape_xml(line)
                    )
                })
                .collect()
        }
        Element::Connector(connector) => {
            let mut points = vec![connector.start.point];
            points.extend_from_slice(&connector.route);
            points.push(connector.end.point);
            let points = points
                .iter()
                .map(|point| format!("{},{}", point.x, point.y))
                .collect::<Vec<_>>()
                .join(" ");
            let start = connector
                .start
                .attachment
                .as_ref()
                .map(|value| value.element_id.to_string())
                .unwrap_or_default();
            let end = connector
                .end
                .attachment
                .as_ref()
                .map(|value| value.element_id.to_string())
                .unwrap_or_default();
            format!(
                "<polyline data-inkstone-id=\"{}\" data-kind=\"connector\" \
                 data-start-element=\"{}\" data-end-element=\"{}\" points=\"{}\" \
                 fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{}/>\n",
                connector.id,
                start,
                end,
                points,
                connector.style.color.svg(),
                connector.style.width,
                dash_attr(connector.style.dashed)
            )
        }
        Element::Shape(shape) => shape_svg(shape),
        Element::Media(media) => format!(
            "<g data-inkstone-id=\"{}\" data-kind=\"{:?}\" data-asset-id=\"{}\">\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#eeeeee\" \
             stroke=\"#555555\" stroke-width=\"1\"/>\
             <text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"14\" \
             fill=\"#333333\">{}</text></g>\n",
            media.id,
            media.kind,
            media.asset_id,
            media.bounds.x,
            media.bounds.y,
            media.bounds.width,
            media.bounds.height,
            media.bounds.x + 12.0,
            media.bounds.y + 24.0,
            escape_xml(if media.caption.is_empty() {
                &media.alt_text
            } else {
                &media.caption
            })
        ),
        Element::Table(table) => {
            let mut svg = format!(
                "<g data-inkstone-id=\"{}\" data-kind=\"table\">\
                 <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" \
                 stroke=\"#555555\" stroke-width=\"1\"/>",
                table.id, table.bounds.x, table.bounds.y, table.bounds.width, table.bounds.height
            );
            let cell_w = table.bounds.width / table.columns.max(1) as f32;
            let cell_h = table.bounds.height / table.rows.max(1) as f32;
            for column in 1..table.columns {
                let x = table.bounds.x + cell_w * column as f32;
                svg.push_str(&format!(
                    "<line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\" stroke=\"#888888\"/>",
                    table.bounds.y,
                    table.bounds.y + table.bounds.height
                ));
            }
            for row in 1..table.rows {
                let y = table.bounds.y + cell_h * row as f32;
                svg.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"#888888\"/>",
                    table.bounds.x,
                    table.bounds.x + table.bounds.width
                ));
            }
            for (index, cell) in table.cells.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let column = (index as u32) % table.columns;
                let row = (index as u32) / table.columns;
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"12\" \
                     fill=\"#222222\">{}</text>",
                    table.bounds.x + cell_w * column as f32 + 6.0,
                    table.bounds.y + cell_h * row as f32 + cell_h * 0.65,
                    escape_xml(cell)
                ));
            }
            svg.push_str("</g>\n");
            svg
        }
        Element::Tag(tag) => {
            let (width, height) = tag.size();
            format!(
                "<g data-inkstone-id=\"{}\" data-kind=\"tag\">\
                 <rect x=\"{}\" y=\"{}\" width=\"{width}\" height=\"{height}\" rx=\"8\" \
                 fill=\"{}\" fill-opacity=\"0.16\" stroke=\"{}\"