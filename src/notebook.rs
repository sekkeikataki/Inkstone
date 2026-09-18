use crate::document::{
    Anchor, Attachment, CanvasSettings, Color, Document, DocumentError, Element, Endpoint,
    FORMAT_NAME, MediaKind, Point, Rect,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub const NOTEBOOK_FORMAT: &str = "inkstone.notebook";
pub const NOTEBOOK_VERSION: u32 = 3;
