use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

pub const FORMAT_NAME: &str = "inkstone.document";
pub const FORMAT_VERSION: u32 = 1;

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

    pub fn scale_about(self, pivot: Self, sx: f32, sy: f32) -> Self {
        Self::new(
            pivot.x + (self.x - pivot.x) * sx,
            pivot.y + (self.y - pivot.y) * sy,
        )
    }

    pub fn rotate_about(self, center: Self, degrees: f32) -> Self {
        let radians = degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let dx = self.x - center.x;
        let dy = self.y - center.y;
        Self::new(
            center.x + dx * cos - dy * sin,
            center.y + dx * sin + dy * cos,
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
        Self {
            x: self.x - amount,
            y: self.y - amount,
            width: self.width + amount * 2.0,
            height: self.height + amount * 2.0,
        }
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    pub fn intersects(self, other: Self) -> bool {
        self.x <= other.x + other.width
            && self.x + self.width >= other.x
            && self.y <= other.y + other.height
            && self.y + self.height >= other.y
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }

    pub fn scale_about(self, pivot: Point, sx: f32, sy: f32) -> Self {
        let a = Point::new(self.x, self.y).scale_about(pivot, sx, sy);
        let b = Point::new(self.x + self.width, self.y + self.height).scale_about(pivot, sx, sy);
        Self::from_points(a, b)
    }

    pub fn corners(self) -> [Point; 4] {
        [
            Point::new(self.x, self.y),
            Point::new(self.x + self.width, self.y),
            Point::new(self.x, self.y + self.height),
            Point::new(self.x + self.width, self.y + self.height),
        ]
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
}

impl Default for Color {
    fn default() -> Self {
        Self::INK
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanvasSettings {
    pub background: Color,
    pub grid_spacing: f32,
    pub grid_visible: bool,
}

impl Default for CanvasSettings {
    fn default() -> Self {
        Self {
            background: Color::PAPER,
            grid_spacing: 24.0,
            grid_visible: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: Color::INK,
            width: 2.4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeKind {
    Pen,
    Highlighter,
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

impl Stroke {
    pub fn erase_disk(&self, center: Point, radius: f32) -> Vec<Stroke> {
        if self.points.len() < 2 || !radius.is_finite() || radius <= 0.0 {
            return vec![self.clone()];
        }
        let mut fragments = Vec::new();
        let mut current: Vec<StrokePoint> = Vec::new();
        for pair in self.points.windows(2) {
            let start = pair[0];
            let end = pair[1];
            let pieces = clip_segment_outside_disk(start, end, center, radius);
            for (a, b) in &pieces {
                if current
                    .last()
                    .is_none_or(|last| last.point().distance_to(a.point()) > 0.02)
                {
                    push_stroke_fragment(&mut fragments, self, std::mem::take(&mut current));
                }
                if current.is_empty() {
                    current.push(*a);
                }
                current.push(*b);
            }
            if pieces.is_empty() {
                push_stroke_fragment(&mut fragments, self, std::mem::take(&mut current));
            }
        }
        push_stroke_fragment(&mut fragments, self, current);
        fragments
    }
}

fn push_stroke_fragment(fragments: &mut Vec<Stroke>, source: &Stroke, points: Vec<StrokePoint>) {
    if points.len() < 2 {
        return;
    }
    fragments.push(Stroke {
        id: Uuid::new_v4(),
        kind: source.kind,
        style: source.style.clone(),
        points,
    });
}

fn clip_segment_outside_disk(
    start: StrokePoint,
    end: StrokePoint,
    center: Point,
    radius: f32,
) -> Vec<(StrokePoint, StrokePoint)> {
    let a = start.point();
    let b = end.point();
    let da = a.distance_to(center);
    let db = b.distance_to(center);
    let hits = segment_circle_hits(a, b, center, radius);
    match (da >= radius, db >= radius, hits.as_slice()) {
        (true, true, []) => vec![(start, end)],
        (true, true, [_]) => vec![(start, end)],
        (true, true, [t0, t1, ..]) => {
            let first = interpolate_stroke(start, end, (*t0).min(*t1));
            let second = interpolate_stroke(start, end, (*t0).max(*t1));
            let mut pieces = Vec::new();
            if a.distance_to(first.point()) > 0.02 {
                pieces.push((start, first));
            }
            if second.point().distance_to(b) > 0.02 {
                pieces.push((second, end));
            }
            pieces
        }
        (true, false, hits) => hits
            .first()
            .copied()
            .map(|t| {
                let mid = interpolate_stroke(start, end, t);
                if a.distance_to(mid.point()) > 0.02 {
                    vec![(start, mid)]
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default(),
        (false, true, hits) => hits
            .last()
            .copied()
            .map(|t| {
                let mid = interpolate_stroke(start, end, t);
                if mid.point().distance_to(b) > 0.02 {
                    vec![(mid, end)]
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default(),
        (false, false, _) => Vec::new(),
    }
}

fn interpolate_stroke(start: StrokePoint, end: StrokePoint, t: f32) -> StrokePoint {
    let t = t.clamp(0.0, 1.0);
    StrokePoint {
        x: start.x + (end.x - start.x) * t,
        y: start.y + (end.y - start.y) * t,
        pressure: (start.pressure + (end.pressure - start.pressure) * t).clamp(0.0, 1.0),
    }
}

fn segment_circle_hits(a: Point, b: Point, center: Point, radius: f32) -> Vec<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let fx = a.x - center.x;
    let fy = a.y - center.y;
    let aa = dx * dx + dy * dy;
    if aa < f32::EPSILON {
        return Vec::new();
    }
    let bb = 2.0 * (fx * dx + fy * dy);
    let cc = fx * fx + fy * fy - radius * radius;
    let discriminant = bb * bb - 4.0 * aa * cc;
    if discriminant < 0.0 {
        return Vec::new();
    }
    let sqrt = discriminant.sqrt();
    let mut hits = Vec::new();
    for root in [(-bb - sqrt) / (2.0 * aa), (-bb + sqrt) / (2.0 * aa)] {
        if root.is_finite() && (0.0..=1.0).contains(&root) {
            hits.push(root);
        }
    }
    hits.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    hits.dedup_by(|left, right| (*left - *right).abs() < 1e-5);
    hits
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Resistor,
    Capacitor,
    Ground,
    Motor,
    Gear,
    Bearing,
    Spring,
    Beam,
}

impl ShapeKind {
    pub const ALL: [Self; 10] = [
        Self::Rectangle,
        Self::Ellipse,
        Self::Resistor,
        Self::Capacitor,
        Self::Ground,
        Self::Motor,
        Self::Gear,
        Self::Bearing,
        Self::Spring,
        Self::Beam,
    ];

    pub const NAMES: [&'static str; 10] = [
        "Rectangle",
        "Ellipse",
        "Resistor",
        "Capacitor",
        "Ground",
        "Motor",
        "Gear",
        "Bearing",
        "Spring",
        "Beam",
    ];
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextNote {
    pub id: Uuid,
    pub origin: Point,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub max_width: Option<f32>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Pdf,
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
}

impl Element {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Stroke(value) => value.id,
            Self::Text(value) => value.id,
            Self::Shape(value) => value.id,
            Self::Connector(value) => value.id,
            Self::Media(value) => value.id,
        }
    }

    pub fn set_id(&mut self, id: Uuid) {
        match self {
            Self::Stroke(value) => value.id = id,
            Self::Text(value) => value.id = id,
            Self::Shape(value) => value.id = id,
            Self::Connector(value) => value.id = id,
            Self::Media(value) => value.id = id,
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
                let longest = text
                    .text
                    .lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(1) as f32;
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
            Self::Shape(shape) => shape.bounds.expand(shape.style.width + 4.0),
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
            Self::Text(_) => cardinal_anchors(self.bounds()),
            Self::Media(media) => cardinal_anchors(media.bounds),
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
        }
    }

    pub fn scale_about(&mut self, pivot: Point, sx: f32, sy: f32) {
        let sx = if sx.is_finite() { sx } else { 1.0 };
        let sy = if sy.is_finite() { sy } else { 1.0 };
        match self {
            Self::Stroke(stroke) => {
                for point in &mut stroke.points {
                    let scaled = point.point().scale_about(pivot, sx, sy);
                    point.x = scaled.x;
                    point.y = scaled.y;
                }
                stroke.style.width = (stroke.style.width * sx.abs().max(sy.abs())).max(0.4);
            }
            Self::Text(text) => {
                text.origin = text.origin.scale_about(pivot, sx, sy);
                text.font_size = (text.font_size * sy.abs().max(0.15)).max(6.0);
                if let Some(width) = text.max_width.as_mut() {
                    *width = (*width * sx.abs()).max(12.0);
                }
            }
            Self::Shape(shape) => {
                let scaled = shape.bounds.scale_about(pivot, sx, sy);
                if scaled.width >= 8.0 && scaled.height >= 8.0 {
                    shape.bounds = scaled;
                }
            }
            Self::Connector(connector) => {
                connector.start.point = connector.start.point.scale_about(pivot, sx, sy);
                connector.end.point = connector.end.point.scale_about(pivot, sx, sy);
                for point in &mut connector.route {
                    *point = point.scale_about(pivot, sx, sy);
                }
            }
            Self::Media(media) => {
                let scaled = media.bounds.scale_about(pivot, sx, sy);
                if scaled.width >= 8.0 && scaled.height >= 8.0 {
                    media.bounds = scaled;
                }
            }
        }
    }

    pub fn rotate_about(&mut self, center: Point, degrees: f32) {
        if !degrees.is_finite() || degrees.abs() < 0.01 {
            return;
        }
        match self {
            Self::Stroke(stroke) => {
                for point in &mut stroke.points {
                    let rotated = point.point().rotate_about(center, degrees);
                    point.x = rotated.x;
                    point.y = rotated.y;
                }
            }
            Self::Text(text) => {
                text.origin = text.origin.rotate_about(center, degrees);
            }
            Self::Shape(shape) => {
                let rotated = shape.bounds.center().rotate_about(center, degrees);
                shape.bounds.x = rotated.x - shape.bounds.width / 2.0;
                shape.bounds.y = rotated.y - shape.bounds.height / 2.0;
                shape.rotation_degrees = (shape.rotation_degrees + degrees).rem_euclid(360.0);
            }
            Self::Connector(connector) => {
                connector.start.point = connector.start.point.rotate_about(center, degrees);
                connector.end.point = connector.end.point.rotate_about(center, degrees);
                for point in &mut connector.route {
                    *point = point.rotate_about(center, degrees);
                }
            }
            Self::Media(media) => {
                let rotated = media.bounds.center().rotate_about(center, degrees);
                media.bounds.x = rotated.x - media.bounds.width / 2.0;
                media.bounds.y = rotated.y - media.bounds.height / 2.0;
            }
        }
    }

    pub fn searchable_text(&self) -> &str {
        match self {
            Self::Stroke(_) => "",
            Self::Text(text) => &text.text,
            Self::Shape(shape) => &shape.label,
            Self::Connector(connector) => &connector.label,
            Self::Media(media) => {
                if media.caption.is_empty() {
                    &media.alt_text
                } else {
                    &media.caption
                }
            }
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
            style.color.is_valid() && style.width.is_finite() && style.width > 0.0
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
                 stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
                stroke.id,
                points,
                stroke.style.color.svg(),
                stroke.style.width
            )
        }
        Element::Text(text) => text
            .text
            .lines()
            .enumerate()
            .map(|(index, line)| {
                format!(
                    "<text data-inkstone-id=\"{}\" data-kind=\"text\" x=\"{}\" y=\"{}\" \
                     font-family=\"sans-serif\" font-size=\"{}\" fill=\"{}\">{}</text>\n",
                    text.id,
                    text.origin.x,
                    text.origin.y + index as f32 * text.font_size * 1.25,
                    text.font_size,
                    text.color.svg(),
                    escape_xml(line)
                )
            })
            .collect(),
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
                 fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
                connector.id,
                start,
                end,
                points,
                connector.style.color.svg(),
                connector.style.width
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
    }
}

fn shape_svg(shape: &Shape) -> String {
    let b = shape.bounds;
    let stroke = shape.style.color.svg();
    let fill = shape
        .fill
        .map(Color::svg)
        .unwrap_or_else(|| "none".to_owned());
    let common = format!(
        "data-inkstone-id=\"{}\" data-kind=\"{:?}\" fill=\"{}\" stroke=\"{}\" \
         stroke-width=\"{}\"",
        shape.id, shape.kind, fill, stroke, shape.style.width
    );
    let geometry = match shape.kind {
        ShapeKind::Rectangle | ShapeKind::Beam => format!(
            "<rect {common} x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            b.x, b.y, b.width, b.height
        ),
        ShapeKind::Ellipse | ShapeKind::Motor | ShapeKind::Gear | ShapeKind::Bearing => format!(
            "<ellipse {common} cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>",
            b.center().x,
            b.center().y,
            b.width / 2.0,
            b.height / 2.0
        ),
        ShapeKind::Capacitor => {
            let x1 = b.x + b.width * 0.42;
            let x2 = b.x + b.width * 0.58;
            format!(
                "<g {common}><path d=\"M {} {} H {} M {} {} H {} \
                 M {} {} V {} M {} {} V {}\" fill=\"none\"/></g>",
                b.x,
                b.center().y,
                x1,
                x2,
                b.center().y,
                b.x + b.width,
                x1,
                b.y,
                b.y + b.height,
                x2,
                b.y,
                b.y + b.height
            )
        }
        ShapeKind::Ground => format!(
            "<g {common}><path d=\"M {} {} V {} M {} {} H {} M {} {} H {} M {} {} H {}\" \
             fill=\"none\"/></g>",
            b.center().x,
            b.y,
            b.y + b.height * 0.45,
            b.x,
            b.y + b.height * 0.45,
            b.x + b.width,
            b.x + b.width * 0.18,
            b.y + b.height * 0.68,
            b.x + b.width * 0.82,
            b.x + b.width * 0.36,
            b.y + b.height * 0.9,
            b.x + b.width * 0.64
        ),
        ShapeKind::Resistor | ShapeKind::Spring => {
            let mut path = format!("M {} {}", b.x, b.center().y);
            for index in 0..=8 {
                let x = b.x + b.width * (index as f32 + 1.0) / 10.0;
                let y = if index % 2 == 0 {
                    b.y + b.height * 0.2
                } else {
                    b.y + b.height * 0.8
                };
                path.push_str(&format!(" L {x} {y}"));
            }
            path.push_str(&format!(" L {} {}", b.x + b.width, b.center().y));
            format!("<path {common} d=\"{path}\" fill=\"none\"/>")
        }
    };
    let label = if shape.label.is_empty() {
        String::new()
    } else {
        format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"14\" \
             text-anchor=\"middle\" fill=\"{}\">{}</text>",
            b.center().x,
            b.y + b.height + 18.0,
            stroke,
            escape_xml(&shape.label)
        )
    };
    format!("{geometry}{label}\n")
}

pub(crate) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
