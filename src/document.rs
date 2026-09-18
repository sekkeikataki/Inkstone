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