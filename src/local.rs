use crate::document::{
    BackgroundPattern, Color, Element, ListStyle, PT_PER_MM, Point, Rect, Shape, ShapeKind, Stroke,
    StrokePoint, StrokeStyle, TagElement, TagKind, TextNote,
};
use crate::notebook::{Notebook, NotebookPage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageTemplate {
    #[default]
    Blank,
    Ruled,
    Meeting,
    ToDo,
    Lecture,
    Cornell,
    LabLog,
    TitleBlock,
}

impl PageTemplate {
    pub const ALL: [Self; 8] = [
        Self::Blank,
        Self::Ruled,
        Self::Meeting,
        Self::ToDo,
        Self::Lecture,
        Self::Cornell,
        Self::LabLog,
        Self::TitleBlock,
    ];
    pub const NAMES: [&'static str; 8] = [
        "Blank",
        "Ruled",
        "Meeting",
        "To-do",
        "Lecture",
        "Cornell",
        "Lab log",
        "ISO title block",
    ];
}

pub fn apply_template(page: &mut NotebookPage, template: PageTemplate) {
    match template {
        PageTemplate::Blank => {}
        PageTemplate::Ruled => page.canvas.set_pattern(BackgroundPattern::Lines),
        PageTemplate::Meeting => {
            page.canvas.set_pattern(BackgroundPattern::Lines);
            page.title = "Meeting notes".to_owned();
            let layer = &mut page.layers[0];
            layer.elements.extend([
                heading(Point::new(48.0, 64.0), "Meeting"),
                body(Point::new(48.0, 108.0), "Date:"),
                body(Point::new(48.0, 140.0), "Attendees:"),
                body(Point::new(48.0, 188.0), "Agenda"),
                checklist(Point::new(48.0, 236.0), "Notes"),
            ]);
        }
        PageTemplate::ToDo => {
            page.title = "To-do".to_owned();
            let layer = &mut page.layers[0];
            layer
                .elements
                .push(heading(Point::new(48.0, 64.0), "To-do"));
            for (index, label) in ["First task", "Second task", "Third task"]
                .into_iter()
                .enumerate()
            {
                layer.elements.push(checklist(
                    Point::new(48.0, 120.0 + index as f32 * 36.0),
                    label,
                ));
            }
        }
        PageTemplate::Lecture => {
            page.canvas.set_pattern(BackgroundPattern::Lines);
            page.title = "Lecture".to_owned();
            let layer = &mut page.layers[0];
            layer.elements.extend([
                heading(Point::new(48.0, 64.0), "Lecture"),
                body(Point::new(48.0, 112.0), "Topic:"),
                bullets(Point::new(48.0, 160.0), "Key points"),
                body(Point::new(48.0, 260.0), "Questions"),
            ]);
        }
        PageTemplate::Cornell => {
            page.canvas.set_pattern(BackgroundPattern::Lines);
            page.title = "Cornell notes".to_owned();
            let layer = &mut page.layers[0];
            let rule = StrokeStyle {
                color: Color::rgb(0.62, 0.66, 0.72),
                width: 0.7,
                dashed: false,
            };
            layer.elements.extend([
                heading(Point::new(48.0, 48.0), "Cornell notes"),
                body(Point::new(48.0, 96.0), "Cues"),
                Element::Shape(Shape {
                    id: Uuid::new_v4(),
                    kind: ShapeKind::Line,
                    bounds: Rect::from_drag(Point::new(168.0, 72.0), Point::new(168.0, 620.0)),
                    rotation_degrees: 0.0,
                    style: rule.clone(),
                    fill: None,
                    label: String::new(),
                }),
                body(Point::new(188.0, 96.0), "Notes"),
                body(Point::new(48.0, 660.0), "Summary"),
                Element::Shape(Shape {
                    id: Uuid::new_v4(),
                    kind: ShapeKind::Line,
                    bounds: Rect::from_drag(Point::new(48.0, 636.0), Point::new(520.0, 636.0)),
                    rotation_degrees: 0.0,
                    style: rule,
                    fill: None,
                    label: String::new(),
                }),
            ]);
        }
        PageTemplate::LabLog => {
            page.title = "Lab log".to_owned();
            let layer = &mut page.layers[0];
            layer.elements.extend([
                heading(Point::new(48.0, 56.0), "Lab log"),
                body(Point::new(48.0, 104.0), "Date:"),
                body(Point::new(48.0, 136.0), "Aim:"),
                body(Point::new(48.0, 184.0), "Method"),
                body(Point::new(48.0, 280.0), "Observations"),
                body(Point::new(48.0, 400.0), "Result"),
            ]);
        }
        PageTemplate::TitleBlock => {
            page.canvas.set_pattern(BackgroundPattern::None);
            page.canvas.set_paper_size(crate::document::PaperSize::A4);
            page.title = "Drawing".to_owned();
            let layer = &mut page.layers[0];
            let block = Rect {
                x: 48.0,
                y: 680.0,
                width: 500.0,
                height: 96.0,
            };
            layer.elements.push(Element::Shape(Shape {
                id: Uuid::new_v4(),
                kind: ShapeKind::Rectangle,
                bounds: block,
                rotation_degrees: 0.0,
                style: StrokeStyle {
                    color: Color::INK,
                    width: 1.0,
                    dashed: false,
                },
                fill: None,
                label: String::new(),
            }));
            layer.elements.extend([
                body(Point::new(56.0, 704.0), "Title:"),
                body(Point::new(56.0, 732.0), "Drawn:"),
                body(Point::new(260.0, 704.0), "Scale:"),
                body(Point::new(260.0, 732.0), "ISO 128 / A4"),
                body(Point::new(400.0, 704.0), "Sheet 1/1"),
            ]);
        }
    }
}

fn heading(origin: Point, text: &str) -> Element {
    let mut note = TextNote::plain(origin, text, 28.0, Color::INK);
    note.bold = true;
    Element::Text(note)
}

fn body(origin: Point, text: &str) -> Element {
    Element::Text(TextNote::plain(origin, text, 16.0, Color::INK))
}

fn bullets(origin: Point, text: &str) -> Element {
    let mut note = TextNote::plain(origin, text, 16.0, Color::INK);
    note.list = ListStyle::Bullet;
    Element::Text(note)
}

fn checklist(origin: Point, text: &str) -> Element {
    let mut note = TextNote::plain(origin, text, 16.0, Color::INK);
    note.list = ListStyle::Checklist;
    Element::Text(note)
}

pub fn tag_element(origin: Point, kind: TagKind) -> Element {
    Element::Tag(TagElement {
        id: Uuid::new_v4(),
        origin,
        kind,
        note: kind.label().to_owned(),
        checked: false,
    })
}

pub fn evaluate_equation(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let (expr, _) = trimmed.split_once('=')?;
    if !trimmed.ends_with('=') || trimmed.matches('=').count() != 1 {
        return None;
    }
    let value = eval_expr(expr.trim())?;
    if !value.is_finite() {
        return None;
    }
    let rendered = if (value - value.round()).abs() < 1e-9 {
        format!("{trimmed} {}", value.round() as i64)
    } else {
        format!("{trimmed} {:.4}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    Some(rendered)
}

fn eval_expr(input: &str) -> Option<f64> {
    let tokens = tokenize(input)?;
    let mut index = 0;
    let value = parse_sum(&tokens, &mut index)?;
    if index == tokens.len() {
        Some(value)
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ' ' | '\t' => index += 1,
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                index += 1;
            }
            '0'..='9' | '.' => {
                let start = index;
                index += 1;
                while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.')
                {
                    index += 1;
                }
                let number = chars[start..index]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .ok()?;
                tokens.push(Token::Number(number));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn parse_sum(tokens: &[Token], index: &mut usize) -> Option<f64> {
    let mut value = parse_product(tokens, index)?;
    while let Some(token) = tokens.get(*index) {
        match token {
            Token::Plus => {
                *index += 1;
                value += parse_product(tokens, index)?;
            }
            Token::Minus => {
                *index += 1;
                value -= parse_product(tokens, index)?;
            }
            _ => break,
        }
    }
    Some(value)
}

fn parse_product(tokens: &[Token], index: &mut usize) -> Option<f64> {
    let mut value = parse_unary(tokens, index)?;
    while let Some(token) = tokens.get(*index) {
        match token {
            Token::Star => {
                *index += 1;
                value *= parse_unary(tokens, index)?;
            }
            Token::Slash => {
                *index += 1;
                let divisor = parse_unary(tokens, index)?;
                if divisor.abs() < f64::EPSILON {
                    return None;
                }
                value /= divisor;
            }
            _ => break,
        }
    }
    Some(value)
}

fn parse_unary(tokens: &[Token], index: &mut usize) -> Option<f64> {
    match tokens.get(*index) {
        Some(Token::Plus) => {
            *index += 1;
            parse_unary(tokens, index)
        }
        Some(Token::Minus) => {
            *index += 1;
            Some(-parse_unary(tokens, index)?)
        }
        _ => parse_primary(tokens, index),
    }
}

fn parse_primary(tokens: &[Token], index: &mut usize) -> Option<f64> {
    match tokens.get(*index).copied() {
        Some(Token::Number(value)) => {
            *index += 1;
            Some(value)
        }
        Some(Token::LParen) => {
            *index += 1;
            let value = parse_sum(tokens, index)?;
            if !matches!(tokens.get(*index), Some(Token::RParen)) {
                return None;
            }
            *index += 1;
            Some(value)
        }
        _ => None,
    }
}

pub fn stroke_to_shape(stroke: &Stroke) -> Option<Shape> {
    if stroke.points.len() < 6 {
        return None;
    }
    let mut min = Point::new(f32::MAX, f32::MAX);
    let mut max = Point::new(f32::MIN, f32::MIN);
    for point in &stroke.points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let bounds = Rect::from_points(min, max);
    if bounds.width < 8.0 && bounds.height < 8.0 {
        return None;
    }
    let first = stroke.points[0].point();
    let last = stroke.points[stroke.points.len() - 1].point();
    let closed = first.distance_to(last) < bounds.width.max(bounds.height) * 0.28;
    let line_len = first.distance_to(last).max(1.0);
    let mut line_error = 0.0_f32;
    for point in &stroke.points {
        let t = ((point.x - first.x) * (last.x - first.x)
            + (point.y - first.y) * (last.y - first.y))
            / (line_len * line_len);
        let t = t.clamp(0.0, 1.0);
        let projected = Point::new(
            first.x + (last.x - first.x) * t,
            first.y + (last.y - first.y) * t,
        );
        line_error = line_error.max(point.point().distance_to(projected));
    }
    let kind = if line_error < 10.0 && !closed {
        ShapeKind::Line
    } else if closed && circularity(stroke, bounds) > 0.82 {
        ShapeKind::Ellipse
    } else if closed {
        ShapeKind::Rectangle
    } else {
        return None;
    };
    Some(Shape {
        id: Uuid::new_v4(),
        kind,
        bounds,
        rotation_degrees: 0.0,
        style: StrokeStyle {
            color: stroke.style.color,
            width: stroke.style.width,
            dashed: stroke.style.dashed,
        },
        fill: None,
        label: String::new(),
    })
}

fn circularity(stroke: &Stroke, bounds: Rect) -> f32 {
    let center = bounds.center();
    let radius = (bounds.width.max(bounds.height) / 2.0).max(1.0);
    let mut error = 0.0_f32;
    for point in &stroke.points {
        error += (point.point().distance_to(center) - radius).abs();
    }
    1.0 - (error / (stroke.points.len() as f32 * radius)).clamp(0.0, 1.0)
}

pub fn snap_to_ruler(origin: Point, current: Point) -> Point {
    if (current.x - origin.x).abs() >= (current.y - origin.y).abs() {
        Point::new(current.x, origin.y)
    } else {
        Point::new(origin.x, current.y)
    }
}

/// Snap to 15° increments (ISO 128 preferred angles).
pub fn snap_to_iso_angle(origin: Point, current: Point) -> Point {
    let dx = current.x - origin.x;
    let dy = current.y - origin.y;
    let len = dx.hypot(dy);
    if len < 0.5 {
        return current;
    }
    let step = std::f32::consts::PI / 12.0;
    let snapped = (dy.atan2(dx) / step).round() * step;
    Point::new(
        origin.x + len * snapped.cos(),
        origin.y + len * snapped.sin(),
    )
}

pub fn constrain_to_square(origin: Point, current: Point) -> Point {
    let dx = current.x - origin.x;
    let dy = current.y - origin.y;
    let size = dx.abs().max(dy.abs());
    if size < f32::EPSILON {
        return current;
    }
    Point::new(origin.x + size.copysign(dx), origin.y + size.copysign(dy))
}

pub fn dimension_label(start: Point, end: Point) -> String {
    let mm = start.distance_to(end) / PT_PER_MM;
    if mm >= 100.0 {
        format!("{:.0} mm", mm.round())
    } else if mm >= 10.0 {
        format!("{:.1} mm", (mm * 10.0).round() / 10.0)
    } else {
        format!("{:.2} mm", (mm * 100.0).round() / 100.0)
    }
}

pub const ISO_DATETIME: &str = "%Y-%m-%d %H:%M";
pub const ISO_DATE: &str = "%Y-%m-%d";
pub const ISO_WEEK: &str = "%G-W%V";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateStamp {
    #[default]
    DateTime,
    Date,
    Week,
}

impl DateStamp {
    pub const ALL: [Self; 3] = [Self::DateTime, Self::Date, Self::Week];
    pub const NAMES: [&'static str; 3] = ["ISO date and time", "ISO date", "ISO week"];

    pub fn glib_format(self) -> &'static str {
        match self {
            Self::DateTime => ISO_DATETIME,
            Self::Date => ISO_DATE,
            Self::Week => ISO_WEEK,
        }
    }
}

pub fn night_paper() -> Color {
    Color::NIGHT
}

pub fn apply_night_paper(page: &mut NotebookPage) {
    page.canvas.background = Color::NIGHT;
}

pub fn stabilize_point(last: Point, incoming: Point, strength: f32) -> Point {
    let follow = (1.0 - strength.clamp(0.0, 0.95)).max(0.05);
    Point::new(
        last.x + (incoming.x - last.x) * follow,
        last.y + (incoming.y - last.y) * follow,
    )
}

/// Split a stroke where the eraser disc overlaps it. `None` means the eraser missed.
/// An empty vec means the whole stroke was erased.
pub fn split_stroke(stroke: &Stroke, eraser: Point, radius: f32) -> Option<Vec<Stroke>> {
    if stroke.points.is_empty() {
        return None;
    }
    let hit = stroke
        .points
        .windows(2)
        .any(|pair| segment_hits_disc(pair[0].point(), pair[1].point(), eraser, radius))
        || stroke
            .points
            .iter()
            .any(|point| point.point().distance_to(eraser) <= radius);
    if !hit {
        return None;
    }
    let mut pieces = Vec::new();
    let mut current = Vec::new();
    for point in &stroke.points {
        if point.point().distance_to(eraser) <= radius {
            push_stroke_piece(stroke, &mut current, &mut pieces);
        } else {
            current.push(*point);
        }
    }
    push_stroke_piece(stroke, &mut current, &mut pieces);
    Some(pieces)
}

fn push_stroke_piece(source: &Stroke, current: &mut Vec<StrokePoint>, pieces: &mut Vec<Stroke>) {
    if current.len() >= 2 {
        pieces.push(Stroke {
            id: Uuid::new_v4(),
            kind: source.kind,
            style: source.style.clone(),
            points: std::mem::take(current),
        });
    } else {
        current.clear();
    }
}

fn segment_hits_disc(a: Point, b: Point, center: Point, radius: f32) -> bool {
    let ab = Point::new(b.x - a.x, b.y - a.y);
    let ac = Point::new(center.x - a.x, center.y - a.y);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 < f32::EPSILON {
        return a.distance_to(center) <= radius;
    }
    let t = ((ac.x * ab.x + ac.y * ab.y) / len2).clamp(0.0, 1.0);
    let closest = Point::new(a.x + ab.x * t, a.y + ab.y * t);
    closest.distance_to(center) <= radius
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    CenterX,
    Right,
    Top,
    MiddleY,
    Bottom,
    DistributeX,
    DistributeY,
    SameWidth,
    SameHeight,
}

pub fn align_bounds(bounds: &[Rect], mode: AlignMode) -> Vec<Point> {
    if bounds.is_empty() {
        return Vec::new();
    }
    let min_x = bounds
        .iter()
        .map(|rect| rect.normalized().x)
        .fold(f32::MAX, f32::min);
    let max_right = bounds
        .iter()
        .map(|rect| {
            let rect = rect.normalized();
            rect.x + rect.width
        })
        .fold(f32::MIN, f32::max);
    let min_y = bounds
        .iter()
        .map(|rect| rect.normalized().y)
        .fold(f32::MAX, f32::min);
    let max_bottom = bounds
        .iter()
        .map(|rect| {
            let rect = rect.normalized();
            rect.y + rect.height
        })
        .fold(f32::MIN, f32::max);
    let center_x = (min_x + max_right) / 2.0;
    let center_y = (min_y + max_bottom) / 2.0;
    let max_w = bounds
        .iter()
        .map(|rect| rect.normalized().width)
        .fold(0.0_f32, f32::max);
    let max_h = bounds
        .iter()
        .map(|rect| rect.normalized().height)
        .fold(0.0_f32, f32::max);

    match mode {
        AlignMode::DistributeX if bounds.len() > 2 => distribute_axis(bounds, true),
        AlignMode::DistributeY if bounds.len() > 2 => distribute_axis(bounds, false),
        _ => bounds
            .iter()
            .map(|rect| {
                let rect = rect.normalized();
                match mode {
                    AlignMode::Left => Point::new(min_x - rect.x, 0.0),
                    AlignMode::CenterX => Point::new(center_x - rect.center().x, 0.0),
                    AlignMode::Right => Point::new(max_right - (rect.x + rect.width), 0.0),
                    AlignMode::Top => Point::new(0.0, min_y - rect.y),
                    AlignMode::MiddleY => Point::new(0.0, center_y - rect.center().y),
                    AlignMode::Bottom => Point::new(0.0, max_bottom - (rect.y + rect.height)),
                    AlignMode::SameWidth => Point::new(max_w / rect.width.max(1.0), 1.0),
                    AlignMode::SameHeight => Point::new(1.0, max_h / rect.height.max(1.0)),
                    AlignMode::DistributeX | AlignMode::DistributeY => Point::new(0.0, 0.0),
                }
            })
            .collect(),
    }
}

fn distribute_axis(bounds: &[Rect], horizontal: bool) -> Vec<Point> {
    let mut order: Vec<usize> = (0..bounds.len()).collect();
    order.sort_by(|a, b| {
        let left = bounds[*a].normalized();
        let right = bounds[*b].normalized();
        if horizontal {
            left.center()
                .x
                .partial_cmp(&right.center().x)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            left.center()
                .y
                .partial_cmp(&right.center().y)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    let first = bounds[order[0]].normalized();
    let last = bounds[order[order.len() - 1]].normalized();
    let start = if horizontal {
        first.center().x
    } else {
        first.center().y
    };
    let end = if horizontal {
        last.center().x
    } else {
        last.center().y
    };
    let step = (end - start) / (order.len() - 1) as f32;
    let mut deltas = vec![Point::new(0.0, 0.0); bounds.len()];
    for (rank, index) in order.into_iter().enumerate() {
        let rect = bounds[index].normalized();
        let target = start + step * rank as f32;
        if horizontal {
            deltas[index] = Point::new(target - rect.center().x, 0.0);
        } else {
            deltas[index] = Point::new(0.0, target - rect.center().y);
        }
    }
    deltas
}

pub fn parse_page_links(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let name = rest[..end].trim();
            if !name.is_empty() {
                names.push(name.to_owned());
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    names
}

pub fn page_link_href(page_id: Uuid) -> String {
    format!("inkstone:page:{page_id}")
}

pub fn parse_page_link_href(href: &str) -> Option<Uuid> {
    href.strip_prefix("inkstone:page:")
        .and_then(|value| Uuid::parse_str(value.trim()).ok())
}

pub fn resolve_page_name(notebook: &Notebook, name: &str) -> Option<usize> {
    let needle = name.trim().to_lowercase();
    notebook
        .pages
        .iter()
        .position(|page| page.title.to_lowercase() == needle)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoItem {
    pub page_index: usize,
    pub page_title: String,
    pub element_id: Uuid,
    pub text: String,
    pub checked: bool,
}

pub fn collect_todos(notebook: &Notebook) -> Vec<TodoItem> {
    let mut items = Vec::new();
    for (page_index, page) in notebook.pages.iter().enumerate() {
        for element in page.elements() {
            match element {
                Element::Text(text) if text.list == ListStyle::Checklist => {
                    items.push(TodoItem {
                        page_index,
                        page_title: page.title.clone(),
                        element_id: text.id,
                        text: text.text.clone(),
                        checked: text.checked,
                    });
                }
                Element::Tag(tag) if tag.kind == TagKind::ToDo => {
                    items.push(TodoItem {
                        page_index,
                        page_title: page.title.clone(),
                        element_id: tag.id,
                        text: tag.note.clone(),
                        checked: tag.checked,
                    });
                }
                _ => {}
            }
        }
    }
    items
}

/// Typical handwriting speed in canvas points per second (~170 mm/s).
const INK_REPLAY_PT_PER_SEC: f32 = 480.0;
const MIN_STROKE_REPLAY_SECS: f32 = 0.07;
const MAX_STROKE_REPLAY_SECS: f32 = 3.6;
const APPEAR_REPLAY_SECS: f32 = 0.09;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayEvent {
    pub id: Uuid,
    pub start: f32,
    pub duration: f32,
}

pub fn stroke_path_length(stroke: &Stroke) -> f32 {
    stroke
        .points
        .windows(2)
        .map(|pair| pair[0].point().distance_to(pair[1].point()))
        .sum()
}

pub fn stroke_prefix(stroke: &Stroke, progress: f32) -> Stroke {
    let mut prefix = stroke.clone();
    if stroke.points.len() < 2 {
        return prefix;
    }
    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        return prefix;
    }
    let total = stroke_path_length(stroke);
    if total <= f32::EPSILON || progress <= 0.0 {
        prefix.points.truncate(1);
        return prefix;
    }
    let target = total * progress;
    let mut acc = 0.0;
    let mut points = vec![stroke.points[0]];
    for pair in stroke.points.windows(2) {
        let span = pair[0].point().distance_to(pair[1].point());
        if acc + span >= target {
            let t = if span > f32::EPSILON {
                ((target - acc) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            points.push(StrokePoint {
                x: pair[0].x + (pair[1].x - pair[0].x) * t,
                y: pair[0].y + (pair[1].y - pair[0].y) * t,
                pressure: pair[0].pressure + (pair[1].pressure - pair[0].pressure) * t,
            });
            break;
        }
        acc += span;
        points.push(pair[1]);
    }
    prefix.points = points;
    prefix
}

pub fn element_replay_secs(element: &Element) -> f32 {
    match element {
        Element::Stroke(stroke) => (stroke_path_length(stroke) / INK_REPLAY_PT_PER_SEC)
            .clamp(MIN_STROKE_REPLAY_SECS, MAX_STROKE_REPLAY_SECS),
        _ => APPEAR_REPLAY_SECS,
    }
}

pub fn replay_timeline<'a>(elements: impl IntoIterator<Item = &'a Element>) -> Vec<ReplayEvent> {
    let mut start = 0.0;
    let mut events = Vec::new();
    for element in elements {
        let duration = element_replay_secs(element);
        events.push(ReplayEvent {
            id: element.id(),
            start,
            duration,
        });
        start += duration;
    }
    events
}

pub fn replay_duration(events: &[ReplayEvent]) -> f32 {
    events
        .last()
        .map(|event| event.start + event.duration)
        .unwrap_or(0.0)
}

pub fn replay_progress(seconds: f32, event: &ReplayEvent, is_stroke: bool) -> Option<f32> {
    if seconds < event.start {
        return None;
    }
    if !is_stroke || event.duration <= f32::EPSILON {
        return Some(1.0);
    }
    Some(((seconds - event.start) / event.duration).clamp(0.0, 1.0))
}
