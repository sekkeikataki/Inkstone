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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotebookFile {
    pub title: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub name: String,
    pub relative: PathBuf,
    pub path: PathBuf,
    pub categories: Vec<Category>,
    pub notebooks: Vec<NotebookFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub root: PathBuf,
    pub notebooks: Vec<NotebookFile>,
    pub categories: Vec<Category>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryHit {
    pub path: PathBuf,
    pub notebook_title: String,
    pub page_index: usize,
    pub page_title: String,
    pub snippet: String,
    pub element_id: Uuid,
    pub layer_id: Uuid,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Session {
    pub last_notebook: Option<PathBuf>,
    #[serde(default)]
    pub preferences: Preferences,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    pub const NAMES: [&'static str; 3] = ["System", "Light", "Dark"];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Preferences {
    #[serde(default = "default_true")]
    pub restore_last_notebook: bool,
    #[serde(default = "default_true")]
    pub show_sidebar: bool,
    #[serde(default = "default_true")]
    pub show_chrome: bool,
    #[serde(default = "default_true")]
    pub show_colors: bool,
    #[serde(default = "default_true")]
    pub show_widths: bool,
    #[serde(default)]
    pub stabilizer: bool,
    #[serde(default = "default_true")]
    pub ignore_touch: bool,
    #[serde(default)]
    pub ink_to_shape: bool,
    #[serde(default)]
    pub ruler: bool,
    #[serde(default = "default_replay_speed")]
    pub replay_speed: f32,
    #[serde(default = "default_paper")]
    pub default_paper: PaperSize,
    #[serde(default)]
    pub default_pattern: BackgroundPattern,
    #[serde(default = "default_grid_mm")]
    pub default_grid_mm: f32,
    #[serde(default)]
    pub night_paper_on_new_pages: bool,
    #[serde(default)]
    pub library_root: Option<PathBuf>,
    #[serde(default)]
    pub theme: ThemePref,
    #[serde(default = "default_true")]
    pub confirm_discard: bool,
    #[serde(default = "default_true")]
    pub remember_window: bool,
    #[serde(default = "default_window_width")]
    pub window_width: i32,
    #[serde(default = "default_window_height")]
    pub window_height: i32,
    #[serde(default = "default_toast_secs")]
    pub toast_seconds: u32,
    #[serde(default = "default_autosave_secs")]
    pub autosave_seconds: f32,
    #[serde(default = "default_true")]
    pub save_on_switch: bool,
    #[serde(default = "default_true")]
    pub show_open_tabs: bool,
    #[serde(default = "default_true")]
    pub watch_library: bool,
    #[serde(default = "default_search_limit")]
    pub search_limit: u32,
    #[serde(default = "default_true")]
    pub show_empty_hint: bool,
    #[serde(default = "default_true")]
    pub page_fade: bool,
    #[serde(default = "default_true")]
    pub selection_flash: bool,
    #[serde(default = "default_true")]
    pub animate_zoom: bool,
    #[serde(default = "default_true")]
    pub reduce_motion: bool,
    #[serde(default)]
    pub default_tool: Tool,
    #[serde(default)]
    pub default_color: Color,
    #[serde(default = "default_width_mm")]
    pub default_width_mm: f32,
    #[serde(default)]
    pub default_dashed: bool,
    #[serde(default)]
    pub default_fill: bool,
    #[serde(default = "default_shape")]
    pub default_shape: ShapeKind,
    #[serde(default = "default_font_size")]
    pub default_font_size: f32,
    #[serde(default)]
    pub default_list: ListStyle,
    #[serde(default)]
    pub default_bold: bool,
    #[serde(default = "default_table_cols")]
    pub table_cols: u32,
    #[serde(default = "default_table_rows")]
    pub table_rows: u32,
    #[serde(default)]
    pub date_stamp: DateStamp,
    #[serde(default)]
    pub default_layout: PageLayout,
    #[serde(default)]
    pub default_template: PageTemplate,
    #[serde(default = "default_true")]
    pub insert_after_current: bool,
    #[serde(default = "default_stab")]
    pub stabilizer_strength: f32,
    #[serde(default = "default_palm_ms")]
    pub palm_reject_ms: u64,
    #[serde(default = "default_true")]
    pub use_tilt: bool,
    #[serde(default = "default_true")]
    pub use_pressure: bool,
    #[serde(default = "default_true")]
    pub barrel_eraser: bool,
    #[serde(default = "default_highlighter_alpha")]
    pub highlighter_alpha: f32,
    #[serde(default = "default_highlighter_scale")]
    pub highlighter_width_scale: f32,
    #[serde(default = "default_brush_scale")]
    pub brush_width_scale: f32,
    #[serde(default = "default_eraser_scale")]
    pub eraser_scale: f32,
    #[serde(default = "default_true")]
    pub iso_angle_snap: bool,
    #[serde(default = "default_true")]
    pub tool_shortcuts: bool,
    #[serde(default = "default_startup_zoom")]
    pub startup_zoom: f32,
    #[serde(default = "default_zoom_step")]
    pub zoom_step: f32,
    #[serde(default = "default_min_zoom")]
    pub min_zoom: f32,
    #[serde(default = "default_max_zoom")]
    pub max_zoom: f32,
    #[serde(default = "default_jpeg")]
    pub jpeg_quality: u32,
    #[serde(default = "default_pdf_dpi")]
    pub pdf_dpi: u32,
}

fn default_true() -> bool {
    true
}

fn default_replay_speed() -> f32 {
    1.0
}

fn default_paper() -> PaperSize {
    PaperSize::Infinite
}

fn default_grid_mm() -> f32 {
    5.0
}

fn default_window_width() -> i32 {
    1380
}

fn default_window_height() -> i32 {
    860
}

fn default_toast_secs() -> u32 {
    2
}

fn default_autosave_secs() -> f32 {
    2.0
}

fn default_search_limit() -> u32 {
    80
}

fn default_width_mm() -> f32 {
    0.5
}

fn default_shape() -> ShapeKind {
    ShapeKind::Rectangle
}

fn default_font_size() -> f32 {
    18.0
}

fn default_table_cols() -> u32 {
    4
}

fn default_table_rows() -> u32 {
    3
}

fn default_stab() -> f32 {
    0.62
}

fn default_palm_ms() -> u64 {
    450
}

fn default_highlighter_alpha() -> f32 {
    0.35
}

fn default_highlighter_scale() -> f32 {
    6.0
}

fn default_brush_scale() -> f32 {
    1.6
}

fn default_eraser_scale() -> f32 {
    3.0
}

fn default_startup_zoom() -> f32 {
    1.0
}

fn default_zoom_step() -> f32 {
    1.2
}

fn default_min_zoom() -> f32 {
    0.08
}

fn default_max_zoom() -> f32 {
    16.0
}

fn default_jpeg() -> u32 {
    92
}

fn default_pdf_dpi() -> u32 {
    120
}

impl Default for Preferences {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("preference defaults")
    }
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, DocumentError> {
        let root = root.into();
        seed_library(&root)?;
        scan_library(&root)
    }

    pub fn open_default() -> Result<Self, DocumentError> {
        match Self::open(resolved_library_root(&Session::load())) {
            Ok(library) => Ok(library),
            Err(_) => Self::open(fallback_library_root()),
        }
    }

    pub fn rescan(&mut self) -> Result<(), DocumentError> {
        *self = scan_library(&self.root)?;
        Ok(())
    }

    pub fn notebooks_flat(&self) -> Vec<&NotebookFile> {
        let mut notebooks = Vec::new();
        collect_notebooks(&self.notebooks, &self.categories, &mut notebooks);
        notebooks
    }

    pub fn first_notebook(&self) -> Option<&NotebookFile> {
        self.notebooks_flat().into_iter().next()
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.notebooks_flat()
            .iter()
            .any(|notebook| notebook.path == path)
    }

    pub fn category_paths(&self) -> Vec<(String, PathBuf)> {
        let mut paths = vec![("Library root".to_owned(), self.root.clone())];
        collect_category_paths(&self.categories, &mut paths);
        paths
    }

    pub fn category_label_for(&self, notebook: &Path) -> String {
        let parent = notebook.parent().unwrap_or(notebook);
        if parent == self.root {
            return "Library root".to_owned();
        }
        relative_label(&self.root, parent).unwrap_or_else(|| "Opened from Files".to_owned())
    }

    pub fn create_category(&self, parent: &Path, name: &str) -> Result<PathBuf, DocumentError> {
        let parent = if parent.as_os_str().is_empty() {
            &self.root
        } else {
            parent
        };
        ensure_inside(&self.root, parent)?;
        let path = unique_dir_path(parent, &sanitize_stem(name));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn create_notebook(&self, category: &Path, title: &str) -> Result<PathBuf, DocumentError> {
        let category = if category.as_os_str().is_empty() {
            &self.root
        } else {
            category
        };
        ensure_inside(&self.root, category)?;
        fs::create_dir_all(category)?;
        let path = unique_inkstone_path(category, title, None);
        let notebook = Notebook {
            title: file_stem_title(&path),
            ..Notebook::default()
        };
        notebook.save(&path)?;
        Ok(path)
    }

    pub fn move_notebook(&self, path: &Path, category: &Path) -> Result<PathBuf, DocumentError> {
        let category = if category.as_os_str().is_empty() {
            &self.root
        } else {
            category
        };
        ensure_inside(&self.root, path)?;
        ensure_inside(&self.root, category)?;
        fs::create_dir_all(category)?;
        let title = file_stem_title(path);
        let destination = unique_inkstone_path(category, &title, Some(path));
        if destination != path {
            fs::rename(path, &destination)?;
        }
        Ok(destination)
    }

    pub fn search(&self, query: &str) -> Vec<LibraryHit> {
        self.search_limited(query, 80)
    }

    pub fn search_limited(&self, query: &str, limit: usize) -> Vec<LibraryHit> {
        let needle = query.trim();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for notebook in self.notebooks_flat() {
            let Ok(loaded) = Notebook::load(&notebook.path) else {
                continue;
            };
            for SearchHit {
                page_index,
                page_title,
                layer_id,
                element_id,
                snippet,
            } in loaded.search(needle)
            {
                hits.push(LibraryHit {
                    path: notebook.path.clone(),
                    notebook_title: notebook.title.clone(),
                    page_index,
                    page_title,
                    snippet,
                    element_id,
                    layer_id,
                });
                if hits.len() >= limit.max(1) {
                    return hits;
                }
            }
        }
        hits
    }
}

impl Session {
    pub fn load() -> Self {
        fs::read(session_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn remember(&mut self, path: &Path) -> Result<(), DocumentError> {
        self.last_notebook = Some(path.to_owned());
        self.save()
    }

    pub fn save_preferences(
        &mut self,
        edit: impl FnOnce(&mut Preferences),
    ) -> Result<(), DocumentError> {
        edit(&mut self.preferences);
        self.save()
    }

    pub fn save(&self) -> Result<(), DocumentError> {
        let path = session_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

pub fn default_library_root() -> PathBuf {
    resolved_library_root(&Session::load())
}

pub fn resolved_library_root(session: &Session) -> PathBuf {
    if let Some(path) = env_library_root() {
        return path;
    }
    session
        .preferences
        .library_root
        .clone()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(documents_library_root)
}

pub fn env_library_root() -> Option<PathBuf> {
    std::env::var("INKSTONE_LIBRARY")
        .ok()
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

pub fn documents_library_root() -> PathBuf {
    home_dir().join("Documents").join("Inkstone")
}

pub fn config_dir() -> PathBuf {
    xdg_config_home().join("inkstone")
}

pub fn fallback_library_root() -> PathBuf {
    xdg_data_home().join("inkstone").join("library")
}

pub fn sanitize_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = trimmed.trim_matches('.').trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "Notebook".to_owned()
    } else {
        trimmed.chars().take(80).collect()
    }
}

pub fn file_stem_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Notebook".to_owned())
}

pub fn unique_inkstone_path(dir: &Path, title: &str, current: Option<&Path>) -> PathBuf {
    let stem = sanitize_stem(title);
    let candidate = dir.join(format!("{stem}.inkstone"));
    if is_available(&candidate, current) {
        return candidate;
    }
    for index in 2..1000 {
        let candidate = dir.join(format!("{stem} {index}.inkstone"));
        if is_available(&candidate, current) {
            return candidate;
        }
    }
    dir.join(format!("{stem} extra.inkstone"))
}

pub fn unique_dir_path(parent: &Path, name: &str) -> PathBuf {
    let stem = sanitize_stem(name);
    let candidate = parent.join(&stem);
    if !candidate.exists() {
        return candidate;
    }
    for index in 2..1000 {
        let candidate = parent.join(format!("{stem} {index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} extra"))
}

pub fn relative_label(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(|relative| {
        relative
            .iter()
            .filter_map(|part| part.to_str())
            .collect::<Vec<_>>()
            .join(" / ")
    })
}

pub fn notebook_color_index(path: &Path) -> usize {
    let hash = path.to_string_lossy().bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(33).wrapping_add(u64::from(byte))
    });
    (hash as usize) % 8
}

fn seed_library(root: &Path) -> Result<(), DocumentError> {
    fs::create_dir_all(root)?;
    let guide = root.join(GUIDE_NAME);
    if !guide.exists() {
        fs::write(guide, GUIDE_TEXT)?;
    }
    let empty = scan_library(root)?;
    if empty.notebooks.is_empty() && empty.categories.is_empty() {
        for name in DEFAULT_CATEGORIES {
            fs::create_dir_all(root.join(name))?;
        }
        let notebook = Notebook {
            title: DEFAULT_NOTEBOOK.to_owned(),
            ..Notebook::default()
        };
        notebook.save(
            &root
                .join("Personal")
                .join(format!("{DEFAULT_NOTEBOOK}.inkstone")),
        )?;
    }
    Ok(())
}

fn scan_library(root: &Path) -> Result<Library, DocumentError> {
    Ok(Library {
        root: root.to_owned(),
        notebooks: scan_notebooks(root)?,
        categories: scan_categories(root, root, 0)?,
    })
}

fn scan_categories(root: &Path, dir: &Path, depth: usize) -> Result<Vec<Category>, DocumentError> {
    if depth >= MAX_SCAN_DEPTH {
        return Ok(Vec::new());
    }
    let mut categories = Vec::new();
    let mut entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(categories),
        Err(error) => return Err(error.into()),
    }
    .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if !entry.file_type()?.is_dir() || is_skipped_name(&entry.file_name()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        categories.push(Category {
            name,
            relative: path.strip_prefix(root).unwrap_or(path.as_path()).to_owned(),
            path: path.clone(),
            notebooks: scan_notebooks(&path)?,
            categories: scan_categories(root, &path, depth + 1)?,
        });
    }
    Ok(categories)
}

fn scan_notebooks(dir: &Path) -> Result<Vec<NotebookFile>, DocumentError> {
    let mut notebooks = Vec::new();
    let mut entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(notebooks),
        Err(error) => return Err(error.into()),
    }
    .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("inkstone") {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".tmp") || name.starts_with('.'))
        {
            continue;
        }
        notebooks.push(NotebookFile {
            title: file_stem_title(&path),
            path,
        });
    }
    Ok(notebooks)
}

fn collect_notebooks<'a>(
    notebooks: &'a [NotebookFile],
    categories: &'a [Category],
    out: &mut Vec<&'a NotebookFile>,
) {
    out.extend(notebooks);
    for category in categories {
        collect_notebooks(&category.notebooks, &category.categories, out);
    }
}

fn collect_category_paths(categories: &[Category], out: &mut Vec<(String, PathBuf)>) {
    for category in categories {
        let label = category
            .relative
            .iter()
            .filter_map(|part| part.to_str())
            .collect::<Vec<_>>()
            .join(" / ");
        out.push((label, category.path.clone()));
        collect_category_paths(&category.categories, out);
    }
}

fn is_skipped_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        name.starts_with('.') || name == GUIDE_NAME || name.eq_ignore_ascii_case("lost+found")
    })
}

fn is_available(path: &Path, current: Option<&Path>) -> bool {
    current.is_some_and(|current| current == path) || !path.exists()
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), DocumentError> {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
    let path = if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
    } else {
        path.to_owned()
    };
    if path == root || path.starts_with(&root) {
        Ok(())
    } else {
        Err(DocumentError::Invalid(
            "that folder is outside the Inkstone library".to_owned(),
        ))
    }
}

fn session_path() -> PathBuf {
    xdg_config_home().join("inkstone").join("session.json")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
}

fn xdg_data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
}
