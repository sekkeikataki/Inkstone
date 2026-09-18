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

impl TagKind 