use crate::canvas::Tool;
use crate::document::{
    BackgroundPattern, Color, DocumentError, ListStyle, PageLayout, PaperSize, ShapeKind,
};
use crate::local::{DateStamp, PageTemplate};
use crate::notebook::{Notebook, SearchHit};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const GUIDE_NAME: &str = "How Inkstone organizes files.txt";
pub const GUIDE_TEXT: &str = "\
Inkstone library
================

Each folder is a category. Nest folders as much as you like
(Work / Projects / Motors). Each .inkstone file is one notebook.

Sections and pages live inside a notebook — they are not extra folders.
Rename or move these files in your file manager; Inkstone shows the same layout.
";

pub const NOTEBOOK_SWATCHES: [Color; 8] = [
    Color::BLUE,
    Color::rgb(0.95, 0.62, 0.05),
    Color::rgb(0.05, 0.56, 0.32),
    Color::rgb(0.84, 0.16, 0.20),
    Color::rgb(0.48, 0.20, 0.78),
    Color::rgb(0.05, 0.60, 0.66),
    Color::rgb(0.89, 0.42, 0.07),
    Color::rgb(0.42, 0.45, 0.50),
];

const DEFAULT_CATEGORIES: [&str; 2] = ["Personal", "Work"];
const DEFAULT_NOTEBOOK: &str = "My Notebook";
const MAX_SCAN_DEPTH: usize = 12;
