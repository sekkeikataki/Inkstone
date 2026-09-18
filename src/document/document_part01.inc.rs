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
