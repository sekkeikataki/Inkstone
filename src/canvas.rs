use crate::document::{
    Color, Connector, DocumentError, Element, Endpoint, MediaElement, MediaKind, Point, Rect,
    Shape, ShapeKind, Stroke, StrokeKind, StrokePoint, StrokeStyle, TextNote,
};
use crate::notebook::{Asset, Layer, Notebook, NotebookPage, SearchHit};
use crate::spreadsheet::address::{CellAddr, CellRange, col_name};
use crate::spreadsheet::formula::Value;
use crate::spreadsheet::{Cell, HAlign, LayerKind, Spreadsheet};
use base64::Engine;
use cairo::PdfSurface;
use gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk::cairo::{Context, LineCap, LineJoin};
use gtk::gdk;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 16.0;
const HISTORY_LIMIT: usize = 256;
const SELECTION_FLASH_SECS: f32 = 0.28;
const PAGE_FADE_SECS: f32 = 0.26;
const EMPTY_HINT_SECS: f32 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Pen,
    Highlighter,
    Eraser,
    Pan,
    Text,
    Shape,
    Connector,
}

#[derive(Clone)]
pub struct Canvas {
    area: gtk::DrawingArea,
    state: Rc<RefCell<CanvasState>>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

struct CanvasState {
    notebook: Notebook,
    active_page: usize,
    active_layer: usize,
    path: Option<PathBuf>,
    pan: Point,
    zoom: f32,
    tool: Tool,
    shape_kind: ShapeKind,
    style: StrokeStyle,
    pending_text: String,
    pending_label: String,
    interaction: Option<Interaction>,
    history: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    dirty: bool,
    stylus_active: bool,
    pinch_start_zoom: Option<f32>,
    selection: HashSet<Uuid>,
    image_cache: HashMap<Uuid, Pixbuf>,
    autosave_generation: u64,
    search_position: usize,
    view_animation_generation: u64,
    view_listeners: Vec<Rc<dyn Fn(u32)>>,
    busy_listeners: Vec<Rc<dyn Fn(bool)>>,
    selection_flash: Option<Instant>,
    page_fade: Option<Instant>,
    empty_hint: Option<Instant>,
    fx_generation: u64,
    sheet_range: Option<CellRange>,
    sheet_editing: Option<(CellAddr, String)>,
    sheet_listeners: Vec<Rc<dyn Fn()>>,
    sheet_clipboard: Option<(CellAddr, Vec<(CellAddr, Cell)>)>,
}

enum Interaction {
    Stroke(Stroke),
    Shape {
        start: Point,
        current: Point,
    },
    Connector {
        start: Endpoint,
        current: Point,
    },
    MoveSelection {
        last_world: Point,
        before: Vec<ElementSlot>,
    },
    Lasso {
        start: Point,
        current: Point,
    },
    Pan {
        last_screen: Point,
    },
    Erase,
    SheetSelect {
        start: CellAddr,
        current: CellAddr,
    },
    SheetMove {
        last_world: Point,
        origin_before: Point,
    },
    SheetGrow {
        cols_before: u32,
        rows_before: u32,
    },
    SheetColResize {
        col: u32,
        start_x: f32,
        width_before: f32,
    },
    SheetRowResize {
        row: u32,
        start_y: f32,
        height_before: f32,
    },
    SheetFill {
        source: CellRange,
        current: CellAddr,
    },
}

enum HistoryEntry {
    Added {
        page_id: Uuid,
        layer_id: Uuid,
        id: Uuid,
        stored: Option<Element>,
    },
    ElementsChanged {
        page_id: Uuid,
        slots: Vec<ElementSlot>,
    },
    PageAdded {
        id: Uuid,
        stored: Option<NotebookPage>,
    },
    PageRemoved {
        index: usize,
        stored: Option<NotebookPage>,
    },
    LayerAdded {
        page_id: Uuid,
        id: Uuid,
        stored: Option<Layer>,
    },
    LayerRemoved {
        page_id: Uuid,
        index: usize,
        stored: Option<Layer>,
    },
    SpreadsheetChanged {
        page_id: Uuid,
        layer_id: Uuid,
        stored: Option<Spreadsheet>,
    },
}

#[derive(Clone)]
struct ElementSlot {
    layer_id: Uuid,
    index: usize,
    id: Uuid,
    stored: Option<Element>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            notebook: Notebook::default(),
            active_page: 0,
            active_layer: 0,
            path: None,
            pan: Point::new(640.0, 380.0),
            zoom: 1.0,
            tool: Tool::Pen,
            shape_kind: ShapeKind::Rectangle,
            style: StrokeStyle::default(),
            pending_text: String::new(),
            pending_label: String::new(),
            interaction: None,
            history: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            stylus_active: false,
            pinch_start_zoom: None,
            selection: HashSet::new(),
            image_cache: HashMap::new(),
            autosave_generation: 0,
            search_position: 0,
            view_animation_generation: 0,
            view_listeners: Vec::new(),
            busy_listeners: Vec::new(),
            selection_flash: None,
            page_fade: None,
            empty_hint: Some(Instant::now()),
            fx_generation: 0,
            sheet_range: Some(CellRange::single(CellAddr { col: 0, row: 0 })),
            sheet_editing: None,
            sheet_listeners: Vec::new(),
            sheet_clipboard: None,
        }
    }
}

impl Canvas {
    pub fn new() -> Self {
        let area = gtk::DrawingArea::builder()
            .hexpand(true)
            .vexpand(true)
            .focusable(true)
            .build();
        area.set_content_width(960);
        area.set_content_height(640);

        let state = Rc::new(RefCell::new(CanvasState::default()));
        area.set_draw_func({
            let state = state.clone();
            move |_, context, width, height| {
                draw_canvas(context, width, height, &state.borrow());
            }
        });

        attach_pointer_input(&area, &state);
        attach_stylus_input(&area, &state);
        attach_view_controls(&area, &state);
        attach_keyboard_input(&area, &state);
        area.connect_realize({
            let state = state.clone();
            move |area| {
                {
                    let mut state = state.borrow_mut();
                    if state.page().visible_elements().next().is_none() {
                        state.empty_hint = Some(Instant::now());
                        state.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
                        state.sheet_editing = None;
                    }
                }
                start_canvas_fx(area, &state);
            }
        });

        Self { area, state }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn tool(&self) -> Tool {
        self.state.borrow().tool
    }

    pub fn set_tool(&self, tool: Tool) {
        let mut state = self.state.borrow_mut();
        state.tool = tool;
        state.interaction = None;
        drop(state);
        let cursor = match tool {
            Tool::Select => "default",
            Tool::Pen | Tool::Highlighter | Tool::Shape | Tool::Connector => "crosshair",
            Tool::Eraser => "cell",
            Tool::Pan => "grab",
            Tool::Text => "text",
        };
        self.area.set_cursor_from_name(Some(cursor));
        self.area.queue_draw();
    }

    pub fn set_shape_kind(&self, kind: ShapeKind) {
        self.state.borrow_mut().shape_kind = kind;
    }

    pub fn set_width(&self, width: f32) {
        self.state.borrow_mut().style.width = width.clamp(0.5, 32.0);
    }

    pub fn set_color(&self, color: Color) {
        self.state.borrow_mut().style.color = color;
    }

    pub fn set_text(&self, text: String) {
        self.state.borrow_mut().pending_text = text;
    }

    pub fn set_label(&self, label: String) {
        self.state.borrow_mut().pending_label = label;
    }

    pub fn new_document(&self) {
        let mut state = self.state.borrow_mut();
        state.notebook = Notebook::default();
        state.active_page = 0;
        state.active_layer = 0;
        state.path = None;
        state.pan = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        state.zoom = 1.0;
        state.dirty = false;
        state.history.clear();
        state.redo.clear();
        state.selection.clear();
        state.image_cache.clear();
        state.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
        state.sheet_editing = None;
        state.begin_page_fade();
        drop(state);
        self.emit_view_changed();
        self.emit_sheet_changed();
        start_canvas_fx(&self.area, &self.state);
        self.area.queue_draw();
    }

    pub fn load(&self, path: &Path) -> Result<(), DocumentError> {
        let notebook = Notebook::load(path)?;
        let mut state = self.state.borrow_mut();
        state.notebook = notebook;
        state.active_page = 0;
        state.active_layer = 0;
        state.path = Some(path.to_owned());
        state.history.clear();
        state.redo.clear();
        state.interaction = None;
        state.dirty = false;
        state.selection.clear();
        state.rebuild_image_cache();
        state.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
        state.sheet_editing = None;
        state.begin_page_fade();
        drop(state);
        self.emit_view_changed();
        self.emit_sheet_changed();
        start_canvas_fx(&self.area, &self.state);
        self.area.queue_draw();
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), DocumentError> {
        let mut state = self.state.borrow_mut();
        state.notebook.save(path)?;
        state.path = Some(path.to_owned());
        state.dirty = false;
        Ok(())
    }

    pub fn save_current(&self) -> Result<bool, DocumentError> {
        let Some(path) = self.state.borrow().path.clone() else {
            return Ok(false);
        };
        self.save(&path)?;
        Ok(true)
    }

    pub fn export_svg(&self, path: &Path) -> Result<(), DocumentError> {
        let state = self.state.borrow();
        state.notebook.export_svg(path, state.active_page)
    }

    pub fn export_pdf(&self, path: &Path) -> Result<(), DocumentError> {
        let state = self.state.borrow();
        state.notebook.validate()?;
        let page_width = 842.0;
        let page_height = 595.0;
        let surface = PdfSurface::new(page_width, page_height, path)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let context =
            Context::new(&surface).map_err(|error| DocumentError::Export(error.to_string()))?;
        for (page_index, page) in state.notebook.pages.iter().enumerate() {
            context.set_source_rgb(1.0, 1.0, 1.0);
            context
                .paint()
                .map_err(|error| DocumentError::Export(error.to_string()))?;
            if let Some(bounds) = page.content_bounds() {
                let bounds = bounds.expand(24.0);
                let scale = ((page_width - 48.0) / bounds.width as f64)
                    .min((page_height - 48.0) / bounds.height as f64)
                    .min(1.0);
                context.save().ok();
                context.translate(
                    24.0 - bounds.x as f64 * scale,
                    24.0 - bounds.y as f64 * scale,
                );
                context.scale(scale, scale);
                for layer in page.layers.iter().filter(|layer| layer.visible) {
                    if let Some(spreadsheet) = &layer.spreadsheet {
                        draw_spreadsheet(&context, &state, spreadsheet, layer.id, bounds);
                    }
                    for element in &layer.elements {
                        draw_element(&context, element, &state.image_cache);
                    }
                }
                context.restore().ok();
            }
            if page_index + 1 < state.notebook.pages.len() {
                context
                    .show_page()
                    .map_err(|error| DocumentError::Export(error.to_string()))?;
            }
        }
        surface.flush();
        surface.finish();
        surface
            .status()
            .map_err(|error| DocumentError::Export(error.to_string()))
    }

    pub fn current_path(&self) -> Option<PathBuf> {
        self.state.borrow().path.clone()
    }

    pub fn document_title(&self) -> String {
        self.state.borrow().notebook.title.clone()
    }

    pub fn element_count(&self) -> usize {
        let state = self.state.borrow();
        state.page().elements().count()
            + state
                .page()
                .layers
                .iter()
                .filter_map(|layer| layer.spreadsheet.as_ref())
                .map(|book| {
                    book.sheets
                        .iter()
                        .map(|sheet| sheet.cells.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
    }

    pub fn is_dirty(&self) -> bool {
        self.state.borrow().dirty
    }

    pub fn undo(&self) {
        self.state.borrow_mut().undo();
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn redo(&self) {
        self.state.borrow_mut().redo();
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn page_object_counts(&self) -> Vec<usize> {
        self.state
            .borrow()
            .notebook
            .pages
            .iter()
            .map(|page| {
                page.elements().count()
                    + page
                        .layers
                        .iter()
                        .filter_map(|layer| layer.spreadsheet.as_ref())
                        .map(|book| {
                            book.sheets
                                .iter()
                                .map(|sheet| sheet.cells.len())
                                .sum::<usize>()
                        })
                        .sum::<usize>()
            })
            .collect()
    }

    pub fn layer_summaries(&self) -> Vec<(String, bool, bool, LayerKind)> {
        self.state
            .borrow()
            .page()
            .layers
            .iter()
            .map(|layer| (layer.name.clone(), layer.visible, layer.locked, layer.kind))
            .collect()
    }

    pub fn page_titles(&self) -> Vec<String> {
        self.state
            .borrow()
            .notebook
            .pages
            .iter()
            .map(|page| page.title.clone())
            .collect()
    }

    pub fn active_page_title(&self) -> String {
        self.state.borrow().page().title.clone()
    }

    pub fn rename_active_page(&self, title: String) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.page().title == title {
            return;
        }
        state.page_mut().title = title.to_owned();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn rename_active_layer(&self, name: String) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let index = state.active_layer;
        if state.page().layers[index].name == name {
            return;
        }
        state.page_mut().layers[index].name = name.to_owned();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn cycle_page(&self, delta: i32) -> bool {
        let count = self.state.borrow().notebook.pages.len() as i32;
        if count <= 1 {
            return false;
        }
        let current = self.state.borrow().active_page as i32;
        let next = (current + delta).rem_euclid(count) as usize;
        self.set_active_page(next);
        true
    }

    pub fn active_page_index(&self) -> usize {
        self.state.borrow().active_page
    }

    pub fn set_active_page(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if index >= state.notebook.pages.len() || index == state.active_page {
            return;
        }
        state.active_page = index;
        state.active_layer = 0;
        state.selection.clear();
        state.search_position = 0;
        state.begin_page_fade();
        state.reset_sheet_focus();
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
        self.emit_sheet_changed();
    }

    pub fn add_page(&self) {
        let mut state = self.state.borrow_mut();
        let page = NotebookPage::named(format!("Page {}", state.notebook.pages.len() + 1));
        let id = page.id;
        state.notebook.pages.push(page);
        state.active_page = state.notebook.pages.len() - 1;
        state.active_layer = 0;
        state.push_history(HistoryEntry::PageAdded { id, stored: None });
        state.dirty = true;
        state.begin_page_fade();
        state.reset_sheet_focus();
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
        self.emit_sheet_changed();
    }

    pub fn remove_active_page(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.notebook.pages.len() == 1 {
            return false;
        }
        let index = state.active_page;
        let page = state.notebook.pages.remove(index);
        state.push_history(HistoryEntry::PageRemoved {
            index,
            stored: Some(page),
        });
        state.active_page = index.min(state.notebook.pages.len() - 1);
        state.active_layer = 0;
        state.selection.clear();
        state.dirty = true;
        state.begin_page_fade();
        state.reset_sheet_focus();
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
        self.emit_sheet_changed();
        true
    }

    pub fn layer_names(&self) -> Vec<String> {
        self.state
            .borrow()
            .page()
            .layers
            .iter()
            .map(|layer| {
                let visible = if layer.visible { "●" } else { "○" };
                let locked = if layer.locked { " 🔒" } else { "" };
                format!("{visible} {}{locked}", layer.name)
            })
            .collect()
    }

    pub fn active_layer_name(&self) -> String {
        self.state.borrow().active_layer().name.clone()
    }

    pub fn active_layer_index(&self) -> usize {
        self.state.borrow().active_layer
    }

    pub fn active_layer_visible(&self) -> bool {
        self.state.borrow().active_layer().visible
    }

    pub fn active_layer_kind(&self) -> LayerKind {
        self.state.borrow().active_layer().kind
    }

    pub fn active_layer_locked(&self) -> bool {
        self.state.borrow().active_layer().locked
    }

    pub fn add_spreadsheet_layer(&self) {
        let mut state = self.state.borrow_mut();
        let page_id = state.page().id;
        let layer = Layer::spreadsheet(format!(
            "Spreadsheet {}",
            state
                .page()
                .layers
                .iter()
                .filter(|layer| layer.is_spreadsheet())
                .count()
                + 1
        ));
        let id = layer.id;
        state.page_mut().layers.push(layer);
        state.active_layer = state.page().layers.len() - 1;
        state.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
        state.sheet_editing = None;
        state.push_history(HistoryEntry::LayerAdded {
            page_id,
            id,
            stored: None,
        });
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.frame_active_spreadsheet();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn add_workbook_sheet(&self) {
        let mut state = self.state.borrow_mut();
        if !state.active_layer().is_spreadsheet() || state.active_layer().locked {
            return;
        }
        let Some(before) = state.capture_spreadsheet() else {
            return;
        };
        if let Some(spreadsheet) = state.active_layer_mut().spreadsheet.as_mut() {
            spreadsheet.add_sheet();
        }
        state.push_spreadsheet_history(before);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn sheet_formula(&self) -> String {
        let state = self.state.borrow();
        if let Some((_, buffer)) = &state.sheet_editing {
            return buffer.clone();
        }
        let Some(addr) = state.sheet_range.map(|range| range.start) else {
            return String::new();
        };
        state
            .active_layer()
            .spreadsheet
            .as_ref()
            .and_then(|book| book.active().cells.get(&addr))
            .map(|cell| cell.input.clone())
            .unwrap_or_default()
    }

    pub fn sheet_address(&self) -> String {
        self.state
            .borrow()
            .sheet_range
            .map(|range| {
                if range.start == range.end {
                    range.start.a1()
                } else {
                    range.a1()
                }
            })
            .unwrap_or_else(|| "A1".to_owned())
    }

    pub fn sheet_value(&self) -> String {
        let state = self.state.borrow();
        let Some(addr) = state.sheet_range.map(|range| range.start) else {
            return String::new();
        };
        state
            .active_layer()
            .spreadsheet
            .as_ref()
            .map(|book| book.display_cell(book.active_sheet, addr))
            .unwrap_or_default()
    }

    pub fn set_sheet_formula(&self, text: String) {
        let mut state = self.state.borrow_mut();
        if !state.active_layer().is_spreadsheet() || state.active_layer().locked {
            return;
        }
        let addr = state
            .sheet_range
            .map(|range| range.start)
            .unwrap_or(CellAddr { col: 0, row: 0 });
        state.sheet_editing = Some((addr, text));
        drop(state);
        self.area.queue_draw();
    }

    pub fn commit_sheet_formula(&self) {
        self.state.borrow_mut().commit_sheet_edit();
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn toggle_sheet_bold(&self) {
        self.mutate_selected_cells(|cell| cell.style.bold = !cell.style.bold);
    }

    pub fn cycle_sheet_align(&self) {
        self.mutate_selected_cells(|cell| {
            cell.style.h_align = match cell.style.h_align {
                HAlign::General | HAlign::Left => HAlign::Center,
                HAlign::Center => HAlign::Right,
                HAlign::Right => HAlign::General,
            };
        });
    }

    pub fn goto_sheet_address(&self, text: &str) -> bool {
        let Some(range) = CellRange::parse(text) else {
            return false;
        };
        let mut state = self.state.borrow_mut();
        if !state.active_layer().is_spreadsheet() {
            return false;
        }
        state.commit_sheet_edit();
        if let Some(book) = state.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().grow_to_include(range.end);
        }
        state.sheet_range = Some(range);
        drop(state);
        self.emit_sheet_changed();
        self.area.queue_draw();
        true
    }

    pub fn fill_sheet_selection(&self) {
        let mut state = self.state.borrow_mut();
        if !state.active_layer().is_spreadsheet() || state.active_layer().locked {
            return;
        }
        let Some(range) = state.sheet_range else {
            return;
        };
        if range.start == range.end {
            return;
        }
        let source = if range.rows() > 1 {
            CellRange::new(
                range.start,
                CellAddr {
                    col: range.end.col,
                    row: range.start.row,
                },
            )
        } else {
            CellRange::single(range.start)
        };
        let Some(before) = state.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = state.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().fill(source, range);
        }
        state.push_spreadsheet_history(before);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn fill_sheet_color(&self, color: Color) {
        self.mutate_selected_cells(|cell| cell.style.fill = Some(color));
    }

    pub fn toggle_sheet_italic(&self) {
        self.mutate_selected_cells(|cell| cell.style.italic = !cell.style.italic);
    }

    pub fn add_sheet_rows(&self) {
        self.mutate_sheet(|book| book.active_mut().add_visible_rows(5));
    }

    pub fn add_sheet_cols(&self) {
        self.mutate_sheet(|book| book.active_mut().add_visible_cols(5));
    }

    pub fn insert_sheet_row(&self) {
        let row = self
            .state
            .borrow()
            .sheet_range
            .map(|range| range.start.row)
            .unwrap_or(0);
        self.mutate_sheet(|book| book.active_mut().insert_rows(row, 1));
    }

    pub fn insert_sheet_col(&self) {
        let col = self
            .state
            .borrow()
            .sheet_range
            .map(|range| range.start.col)
            .unwrap_or(0);
        self.mutate_sheet(|book| book.active_mut().insert_cols(col, 1));
    }

    pub fn merge_sheet_selection(&self) {
        let Some(range) = self.state.borrow().sheet_range else {
            return;
        };
        self.mutate_sheet(|book| book.active_mut().merge_range(range));
    }

    pub fn sort_sheet_selection(&self) {
        let Some(range) = self.state.borrow().sheet_range else {
            return;
        };
        self.mutate_sheet(|book| book.active_mut().sort_range(range, range.start.col, true));
    }

    pub fn cycle_number_format(&self) {
        self.mutate_selected_cells(|cell| {
            cell.style.number_format = match cell.style.number_format.as_str() {
                "" | "General" => "#,##0.00".into(),
                "#,##0.00" => "0%".into(),
                "0%" => "$#,##0.00".into(),
                "$#,##0.00" => "yyyy-mm-dd".into(),
                _ => String::new(),
            };
        });
    }

    fn mutate_sheet(&self, mutator: impl Fn(&mut Spreadsheet)) {
        let mut state = self.state.borrow_mut();
        if !state.active_layer().is_spreadsheet() || state.active_layer().locked {
            return;
        }
        let Some(before) = state.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = state.active_layer_mut().spreadsheet.as_mut() {
            mutator(book);
        }
        state.push_spreadsheet_history(before);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    fn mutate_selected_cells(&self, mutator: impl Fn(&mut crate::spreadsheet::Cell)) {
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked || !state.active_layer().is_spreadsheet() {
            return;
        }
        let Some(range) = state.sheet_range else {
            return;
        };
        let Some(before) = state.capture_spreadsheet() else {
            return;
        };
        if let Some(sheet) = state.active_layer_mut().spreadsheet.as_mut() {
            for addr in range.cells() {
                mutator(sheet.active_mut().cells.entry(addr).or_default());
            }
        }
        state.push_spreadsheet_history(before);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn connect_sheet_changed(&self, callback: impl Fn() + 'static) {
        self.state
            .borrow_mut()
            .sheet_listeners
            .push(Rc::new(callback));
    }

    fn frame_active_spreadsheet(&self) {
        let width = self.area.width() as f32;
        let height = self.area.height() as f32;
        if width < 80.0 || height < 80.0 {
            return;
        }
        let mut state = self.state.borrow_mut();
        let Some(bounds) = state
            .active_layer()
            .spreadsheet
            .as_ref()
            .map(Spreadsheet::bounds)
        else {
            return;
        };
        let pad_x = 108.0;
        let pad_y = 96.0;
        let avail_w = (width - pad_x * 2.0).max(160.0);
        let avail_h = (height - pad_y * 2.0).max(120.0);
        let zoom = (avail_w / bounds.width.max(1.0))
            .min(avail_h / bounds.height.max(1.0))
            .clamp(MIN_ZOOM, 1.0);
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        state.zoom = zoom;
        state.pan = Point::new(
            width / 2.0 - (bounds.x + bounds.width / 2.0) * zoom,
            height / 2.0 - (bounds.y + bounds.height / 2.0) * zoom,
        );
        drop(state);
        self.emit_view_changed();
    }

    fn emit_sheet_changed(&self) {
        emit_sheet_changed(&self.state);
    }

    pub fn grid_visible(&self) -> bool {
        self.state.borrow().page().canvas.grid_visible
    }

    pub fn set_active_layer(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if index < state.page().layers.len() {
            state.commit_sheet_edit();
            state.active_layer = index;
            state.selection.clear();
            if state.active_layer().is_spreadsheet() {
                state.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
            }
            state.sheet_editing = None;
        }
        drop(state);
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn add_layer(&self) {
        let mut state = self.state.borrow_mut();
        let page_id = state.page().id;
        let layer = Layer::named(format!("Layer {}", state.page().layers.len() + 1));
        let id = layer.id;
        state.page_mut().layers.push(layer);
        state.active_layer = state.page().layers.len() - 1;
        state.push_history(HistoryEntry::LayerAdded {
            page_id,
            id,
            stored: None,
        });
        state.dirty = true;
        state.reset_sheet_focus();
        drop(state);
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
    }

    pub fn remove_active_layer(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.page().layers.len() == 1 {
            return false;
        }
        let page_id = state.page().id;
        let index = state.active_layer;
        let layer = state.page_mut().layers.remove(index);
        state.push_history(HistoryEntry::LayerRemoved {
            page_id,
            index,
            stored: Some(layer),
        });
        state.active_layer = index.min(state.page().layers.len() - 1);
        state.selection.clear();
        state.dirty = true;
        state.reset_sheet_focus();
        drop(state);
        self.schedule_autosave();
        self.emit_sheet_changed();
        self.area.queue_draw();
        true
    }

    pub fn toggle_active_layer_visibility(&self) {
        let mut state = self.state.borrow_mut();
        let index = state.active_layer;
        let layer = &mut state.page_mut().layers[index];
        layer.visible = !layer.visible;
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn toggle_active_layer_lock(&self) {
        let mut state = self.state.borrow_mut();
        let index = state.active_layer;
        let layer = &mut state.page_mut().layers[index];
        layer.locked = !layer.locked;
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn delete_selection(&self) -> usize {
        let mut state = self.state.borrow_mut();
        let count = state.delete_selection();
        drop(state);
        if count > 0 {
            self.schedule_autosave();
            self.emit_sheet_changed();
            self.area.queue_draw();
        }
        count
    }

    pub fn duplicate_selection(&self) -> usize {
        let mut state = self.state.borrow_mut();
        let count = state.duplicate_selection();
        if count > 0 {
            state.flash_selection();
        }
        drop(state);
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        count
    }

    pub fn find_next(&self, query: &str) -> Option<SearchHit> {
        let mut state = self.state.borrow_mut();
        let hits = state.notebook.search(query);
        if hits.is_empty() {
            state.search_position = 0;
            return None;
        }
        let index = state.search_position % hits.len();
        state.search_position = state.search_position.wrapping_add(1);
        let hit = hits[index].clone();
        state.active_page = hit.page_index;
        state.active_layer = state
            .page()
            .layers
            .iter()
            .position(|layer| layer.id == hit.layer_id)
            .unwrap_or(0);
        state.selection.clear();
        if let Some(cell) = hit.cell.as_deref().and_then(CellAddr::parse_a1) {
            state.sheet_range = Some(CellRange::single(cell));
            if let Some(book) = state.active_layer().spreadsheet.as_ref() {
                let center = book.cell_rect(cell).center();
                state.pan = Point::new(
                    self.area.width() as f32 / 2.0 - center.x * state.zoom,
                    self.area.height() as f32 / 2.0 - center.y * state.zoom,
                );
            }
        } else {
            state.selection.insert(hit.element_id);
            state.flash_selection();
            state.begin_page_fade();
            let target_bounds = {
                state
                    .page()
                    .elements()
                    .find(|element| element.id() == hit.element_id)
                    .map(Element::bounds)
            };
            if let Some(bounds) = target_bounds {
                let center = bounds.center();
                state.pan = Point::new(
                    self.area.width() as f32 / 2.0 - center.x * state.zoom,
                    self.area.height() as f32 / 2.0 - center.y * state.zoom,
                );
            }
        }
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.emit_sheet_changed();
        self.area.queue_draw();
        Some(hit)
    }

    pub fn import_media(&self, path: &Path) -> Result<(), DocumentError> {
        const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
        let metadata = fs::metadata(path)?;
        if metadata.len() > MAX_ASSET_BYTES {
            return Err(DocumentError::Invalid(
                "embedded files are limited to 64 MiB".to_owned(),
            ));
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let (kind, media_type) = match extension.as_str() {
            "png" => (MediaKind::Image, "image/png"),
            "jpg" | "jpeg" => (MediaKind::Image, "image/jpeg"),
            "webp" => (MediaKind::Image, "image/webp"),
            "gif" => (MediaKind::Image, "image/gif"),
            "pdf" => (MediaKind::Pdf, "application/pdf"),
            _ => {
                return Err(DocumentError::Invalid(
                    "supported imports are PNG, JPEG, WebP, GIF, and PDF".to_owned(),
                ));
            }
        };
        let bytes = fs::read(path)?;
        if self.state.borrow().active_layer().is_spreadsheet() {
            return Err(DocumentError::Invalid(
                "import images onto a notes layer; spreadsheet layers hold workbooks".to_owned(),
            ));
        }
        let asset_id = Uuid::new_v4();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Embedded file")
            .to_owned();
        let mut state = self.state.borrow_mut();
        let center = state.screen_to_world(Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        ));
        let (width, height) = if kind == MediaKind::Image {
            let pixbuf = decode_pixbuf(&bytes)?;
            let scale = (640.0 / pixbuf.width() as f32)
                .min(480.0 / pixbuf.height() as f32)
                .min(1.0);
            let size = (
                pixbuf.width() as f32 * scale,
                pixbuf.height() as f32 * scale,
            );
            state.image_cache.insert(asset_id, pixbuf);
            size
        } else {
            (360.0, 220.0)
        };
        state.notebook.assets.push(Asset {
            id: asset_id,
            name: name.clone(),
            media_type: media_type.to_owned(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
        state.add_element(Element::Media(MediaElement {
            id: Uuid::new_v4(),
            asset_id,
            kind,
            bounds: Rect {
                x: center.x - width / 2.0,
                y: center.y - height / 2.0,
                width,
                height,
            },
            alt_text: name.clone(),
            caption: name,
        }));
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
        Ok(())
    }

    pub fn toggle_grid(&self) {
        let mut state = self.state.borrow_mut();
        let visible = state.page().canvas.grid_visible;
        state.page_mut().canvas.grid_visible = !visible;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    fn schedule_autosave(&self) {
        schedule_autosave(&self.state);
    }

    pub fn reset_view(&self) {
        let mut state = self.state.borrow_mut();
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        state.pan = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        state.zoom = 1.0;
        drop(state);
        self.emit_view_changed();
        self.area.queue_draw();
    }

    pub fn zoom_by(&self, factor: f32) {
        let center = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        let mut state = self.state.borrow_mut();
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        let requested = state.zoom * factor;
        state.set_zoom_around(requested, center);
        drop(state);
        self.emit_view_changed();
        self.area.queue_draw();
    }

    pub fn animate_zoom_by(&self, factor: f32) -> u32 {
        if gtk::Settings::default().is_none_or(|settings| !settings.is_gtk_enable_animations())
            || self.area.frame_clock().is_none()
        {
            self.zoom_by(factor);
            return self.zoom_percent();
        }
        let center = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        let (start_zoom, target_zoom, generation) = {
            let mut state = self.state.borrow_mut();
            state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
            (
                state.zoom,
                (state.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM),
                state.view_animation_generation,
            )
        };
        let started = Instant::now();
        let state = self.state.clone();
        self.area.add_tick_callback(move |area, _clock| {
            let elapsed = started.elapsed().as_secs_f32();
            let progress = (elapsed / 0.18).min(1.0);
            let eased = 1.0 - (1.0 - progress).powi(3);
            {
                let mut state = state.borrow_mut();
                if state.view_animation_generation != generation {
                    return glib::ControlFlow::Break;
                }
                state.set_zoom_around(start_zoom + (target_zoom - start_zoom) * eased, center);
            }
            emit_view_changed(&state);
            area.queue_draw();
            if progress >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        (target_zoom * 100.0).round() as u32
    }

    pub fn zoom_percent(&self) -> u32 {
        self.state.borrow().zoom_percent()
    }

    pub fn connect_view_changed(&self, callback: impl Fn(u32) + 'static) {
        let callback = Rc::new(callback);
        self.state
            .borrow_mut()
            .view_listeners
            .push(callback.clone());
        callback(self.zoom_percent());
    }

    pub fn connect_busy(&self, callback: impl Fn(bool) + 'static) {
        self.state
            .borrow_mut()
            .busy_listeners
            .push(Rc::new(callback));
    }

    fn emit_view_changed(&self) {
        emit_view_changed(&self.state);
    }
}

impl CanvasState {
    fn zoom_percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }

    fn flash_selection(&mut self) {
        self.selection_flash = Some(Instant::now());
    }

    fn begin_page_fade(&mut self) {
        self.page_fade = Some(Instant::now());
        if self.page().visible_elements().next().is_none()
            && !self
                .page()
                .layers
                .iter()
                .any(|layer| layer.visible && layer.spreadsheet.is_some())
        {
            self.empty_hint = Some(Instant::now());
        }
    }

    fn page(&self) -> &NotebookPage {
        &self.notebook.pages[self.active_page]
    }

    fn page_mut(&mut self) -> &mut NotebookPage {
        &mut self.notebook.pages[self.active_page]
    }

    fn active_layer(&self) -> &Layer {
        &self.page().layers[self.active_layer]
    }

    fn active_layer_mut(&mut self) -> &mut Layer {
        let page = self.active_page;
        let layer = self.active_layer;
        &mut self.notebook.pages[page].layers[layer]
    }

    fn reset_sheet_focus(&mut self) {
        self.sheet_editing = None;
        if self.active_layer().is_spreadsheet() {
            self.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
        }
    }

    fn capture_spreadsheet(&self) -> Option<(Uuid, Uuid, Spreadsheet)> {
        let layer = self.active_layer();
        Some((self.page().id, layer.id, layer.spreadsheet.clone()?))
    }

    fn push_spreadsheet_history(&mut self, (page_id, layer_id, stored): (Uuid, Uuid, Spreadsheet)) {
        self.push_history(HistoryEntry::SpreadsheetChanged {
            page_id,
            layer_id,
            stored: Some(stored),
        });
    }

    fn commit_sheet_edit(&mut self) {
        let Some((addr, input)) = self.sheet_editing.take() else {
            return;
        };
        if self.active_layer().locked || !self.active_layer().is_spreadsheet() {
            return;
        }
        let current = self
            .active_layer()
            .spreadsheet
            .as_ref()
            .and_then(|book| book.active().cells.get(&addr))
            .map(|cell| cell.input.clone())
            .unwrap_or_default();
        if current == input {
            return;
        }
        let Some(before) = self.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = self.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().set_input(addr, input);
            book.active_mut().grow_to_include(addr);
        }
        self.push_spreadsheet_history(before);
        self.dirty = true;
    }

    fn clear_sheet_selection(&mut self) -> usize {
        self.commit_sheet_edit();
        if self.active_layer().locked {
            return 0;
        }
        let Some(range) = self.sheet_range else {
            return 0;
        };
        let Some(before) = self.capture_spreadsheet() else {
            return 0;
        };
        let Some(book) = self.active_layer_mut().spreadsheet.as_mut() else {
            return 0;
        };
        let count = range.cells().count();
        book.active_mut().clear_range(range);
        self.push_spreadsheet_history(before);
        self.dirty = true;
        count
    }

    fn toggle_sheet_style(&mut self, mutator: impl Fn(&mut Cell)) {
        self.commit_sheet_edit();
        let Some(range) = self.sheet_range else {
            return;
        };
        let Some(before) = self.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = self.active_layer_mut().spreadsheet.as_mut() {
            for addr in range.cells() {
                mutator(book.active_mut().cells.entry(addr).or_default());
            }
        }
        self.push_spreadsheet_history(before);
        self.dirty = true;
    }

    fn copy_sheet_selection(&mut self) {
        let Some(range) = self.sheet_range else {
            return;
        };
        let Some(book) = self.active_layer().spreadsheet.as_ref() else {
            return;
        };
        let mut cells = Vec::new();
        for addr in range.cells() {
            if let Some(cell) = book.active().cells.get(&addr) {
                cells.push((addr, cell.clone()));
            }
        }
        self.sheet_clipboard = Some((range.start, cells));
    }

    fn paste_sheet_selection(&mut self) {
        self.commit_sheet_edit();
        let Some((origin, cells)) = self.sheet_clipboard.clone() else {
            return;
        };
        let dest = self
            .sheet_range
            .map(|range| range.start)
            .unwrap_or(CellAddr { col: 0, row: 0 });
        let Some(before) = self.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = self.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().paste_from(origin, &cells, dest);
        }
        self.push_spreadsheet_history(before);
        self.dirty = true;
    }

    fn fill_sheet_down(&mut self) {
        let Some(range) = self.sheet_range else {
            return;
        };
        if range.rows() < 2 {
            return;
        }
        let source = CellRange::new(
            range.start,
            CellAddr {
                col: range.end.col,
                row: range.start.row,
            },
        );
        self.fill_sheet_range(source, range);
    }

    fn fill_sheet_right(&mut self) {
        let Some(range) = self.sheet_range else {
            return;
        };
        if range.cols() < 2 {
            return;
        }
        let source = CellRange::new(
            range.start,
            CellAddr {
                col: range.start.col,
                row: range.end.row,
            },
        );
        self.fill_sheet_range(source, range);
    }

    fn fill_sheet_range(&mut self, source: CellRange, target: CellRange) {
        self.commit_sheet_edit();
        let Some(before) = self.capture_spreadsheet() else {
            return;
        };
        if let Some(book) = self.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().fill(source, target);
            book.active_mut().grow_to_include(target.end);
        }
        self.push_spreadsheet_history(before);
        self.dirty = true;
    }

    fn handle_sheet_key(&mut self, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
        if !self.active_layer().is_spreadsheet() {
            return false;
        }
        let ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if self.active_layer().locked && !matches!(key, gtk::gdk::Key::Escape) {
            return false;
        }
        if key == gtk::gdk::Key::Escape {
            self.sheet_editing = None;
            return true;
        }
        if key == gtk::gdk::Key::F2 {
            let addr = self
                .sheet_range
                .map(|range| range.start)
                .unwrap_or(CellAddr { col: 0, row: 0 });
            let current = self
                .active_layer()
                .spreadsheet
                .as_ref()
                .and_then(|book| book.active().cells.get(&addr))
                .map(|cell| cell.input.clone())
                .unwrap_or_default();
            self.sheet_editing = Some((addr, current));
            return true;
        }
        if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
            self.commit_sheet_edit();
            self.move_sheet_selection(0, 1, shift);
            return true;
        }
        if key == gtk::gdk::Key::Tab {
            self.commit_sheet_edit();
            self.move_sheet_selection(1, 0, shift);
            return true;
        }
        if self.sheet_editing.is_none() {
            if key == gtk::gdk::Key::Left {
                self.move_sheet_selection(-1, 0, shift);
                return true;
            }
            if key == gtk::gdk::Key::Right {
                self.move_sheet_selection(1, 0, shift);
                return true;
            }
            if key == gtk::gdk::Key::Up {
                self.move_sheet_selection(0, -1, shift);
                return true;
            }
            if key == gtk::gdk::Key::Down {
                self.move_sheet_selection(0, 1, shift);
                return true;
            }
            if key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::BackSpace {
                self.clear_sheet_selection();
                return true;
            }
            if ctrl && key == gtk::gdk::Key::b {
                if let Some(range) = self.sheet_range
                    && let Some(before) = self.capture_spreadsheet()
                {
                    if let Some(sheet) = self.active_layer_mut().spreadsheet.as_mut() {
                        for addr in range.cells() {
                            let cell = sheet.active_mut().cells.entry(addr).or_default();
                            cell.style.bold = !cell.style.bold;
                        }
                    }
                    self.push_spreadsheet_history(before);
                    self.dirty = true;
                }
                return true;
            }
            if ctrl && key == gtk::gdk::Key::c {
                self.copy_sheet_selection();
                return true;
            }
            if ctrl && key == gtk::gdk::Key::x {
                self.copy_sheet_selection();
                self.clear_sheet_selection();
                return true;
            }
            if ctrl && key == gtk::gdk::Key::v {
                self.paste_sheet_selection();
                return true;
            }
            if ctrl && key == gtk::gdk::Key::i {
                self.toggle_sheet_style(|cell| cell.style.italic = !cell.style.italic);
                return true;
            }
            if ctrl && key == gtk::gdk::Key::u {
                self.toggle_sheet_style(|cell| cell.style.underline = !cell.style.underline);
                return true;
            }
            if ctrl && key == gtk::gdk::Key::d {
                self.fill_sheet_down();
                return true;
            }
            if ctrl && (key == gtk::gdk::Key::r || key == gtk::gdk::Key::R) {
                self.fill_sheet_right();
                return true;
            }
            if let Some(ch) = key.to_unicode()
                && !ctrl
                && !ch.is_control()
            {
                let addr = self
                    .sheet_range
                    .map(|range| range.start)
                    .unwrap_or(CellAddr { col: 0, row: 0 });
                self.sheet_editing = Some((addr, ch.to_string()));
                return true;
            }
            return false;
        }
        if let Some((_, buffer)) = self.sheet_editing.as_mut() {
            if key == gtk::gdk::Key::BackSpace {
                buffer.pop();
                return true;
            }
            if let Some(ch) = key.to_unicode()
                && !ctrl
                && !ch.is_control()
            {
                buffer.push(ch);
                return true;
            }
        }
        false
    }

    fn move_sheet_selection(&mut self, dcol: i32, drow: i32, extend: bool) {
        let Some(range) = self.sheet_range else {
            self.sheet_range = Some(CellRange::single(CellAddr { col: 0, row: 0 }));
            return;
        };
        let Some(next) = range.end.offset(dcol, drow) else {
            return;
        };
        if let Some(book) = self.active_layer_mut().spreadsheet.as_mut() {
            book.active_mut().grow_to_include(next);
        }
        self.sheet_range = Some(if extend {
            CellRange::new(range.start, next)
        } else {
            CellRange::single(next)
        });
    }

    fn begin_sheet_pointer(&mut self, world: Point) {
        self.commit_sheet_edit();
        let Some(book) = self.active_layer().spreadsheet.clone() else {
            return;
        };
        if book.hit_grow_handle(world) {
            let sheet = book.active();
            self.interaction = Some(Interaction::SheetGrow {
                cols_before: sheet.display_cols(),
                rows_before: sheet.display_rows(),
            });
            return;
        }
        if let Some(range) = self.sheet_range
            && hit_fill_handle(&book, range, world)
        {
            self.interaction = Some(Interaction::SheetFill {
                source: range,
                current: range.end,
            });
            return;
        }
        if let Some(col) = book.hit_col_resize(world) {
            self.interaction = Some(Interaction::SheetColResize {
                col,
                start_x: world.x,
                width_before: book.active().col_width(col),
            });
            return;
        }
        if let Some(row) = book.hit_row_resize(world) {
            self.interaction = Some(Interaction::SheetRowResize {
                row,
                start_y: world.y,
                height_before: book.active().row_height(row),
            });
            return;
        }
        if let Some(index) = book.hit_tab(world) {
            if let Some(before) = self.capture_spreadsheet() {
                if let Some(spreadsheet) = self.active_layer_mut().spreadsheet.as_mut() {
                    spreadsheet.active_sheet = index;
                }
                self.push_spreadsheet_history(before);
                self.dirty = true;
            }
            return;
        }
        if book.hit_title(world) {
            self.interaction = Some(Interaction::SheetMove {
                last_world: world,
                origin_before: book.origin,
            });
            return;
        }
        if book.hit_select_all(world) {
            let sheet = book.active();
            if let (Some(start), Some(end)) = (
                CellAddr::new(0, 0),
                CellAddr::new(sheet.display_cols() - 1, sheet.display_rows() - 1),
            ) {
                self.sheet_range = Some(CellRange::new(start, end));
            }
            return;
        }
        if let Some(col) = book.hit_col_header(world) {
            let rows = book.active().display_rows().saturating_sub(1);
            if let Some(end) = CellAddr::new(col, rows) {
                self.sheet_range = Some(CellRange::new(CellAddr { col, row: 0 }, end));
            }
            return;
        }
        if let Some(row) = book.hit_row_header(world) {
            let cols = book.active().display_cols().saturating_sub(1);
            if let Some(end) = CellAddr::new(cols, row) {
                self.sheet_range = Some(CellRange::new(CellAddr { col: 0, row }, end));
            }
            return;
        }
        if let Some(addr) = book.hit_cell(world) {
            self.sheet_range = Some(CellRange::single(addr));
            self.interaction = Some(Interaction::SheetSelect {
                start: addr,
                current: addr,
            });
        }
    }

    fn rebuild_image_cache(&mut self) {
        self.image_cache.clear();
        for asset in &self.notebook.assets {
            if asset.media_type.starts_with("image/")
                && let Ok(bytes) = asset.decoded()
                && let Ok(pixbuf) = decode_pixbuf(&bytes)
            {
                self.image_cache.insert(asset.id, pixbuf);
            }
        }
    }

    fn hit_test(&self, point: Point) -> Option<Uuid> {
        self.page()
            .layers
            .iter()
            .rev()
            .filter(|layer| layer.visible && !layer.locked)
            .flat_map(|layer| layer.elements.iter().rev())
            .find(|element| element.bounds().expand(6.0 / self.zoom).contains(point))
            .map(Element::id)
    }

    fn capture_elements(&self, ids: &HashSet<Uuid>) -> Vec<ElementSlot> {
        let mut affected = ids.clone();
        for element in self.page().elements() {
            if let Element::Connector(connector) = element
                && [&connector.start, &connector.end]
                    .into_iter()
                    .filter_map(|endpoint| endpoint.attachment.as_ref())
                    .any(|attachment| ids.contains(&attachment.element_id))
            {
                affected.insert(connector.id);
            }
        }
        let mut slots = Vec::new();
        for layer in &self.page().layers {
            for (index, element) in layer.elements.iter().enumerate() {
                if affected.contains(&element.id()) {
                    slots.push(ElementSlot {
                        layer_id: layer.id,
                        index,
                        id: element.id(),
                        stored: Some(element.clone()),
                    });
                }
            }
        }
        slots
    }

    fn translate_selection(&mut self, delta: Point) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.translate(delta);
                    continue;
                }
                if let Element::Connector(connector) = element {
                    let mut changed = false;
                    if connector
                        .start
                        .attachment
                        .as_ref()
                        .is_some_and(|attachment| selected.contains(&attachment.element_id))
                    {
                        connector.start.point.x += delta.x;
                        connector.start.point.y += delta.y;
                        changed = true;
                    }
                    if connector
                        .end
                        .attachment
                        .as_ref()
                        .is_some_and(|attachment| selected.contains(&attachment.element_id))
                    {
                        connector.end.point.x += delta.x;
                        connector.end.point.y += delta.y;
                        changed = true;
                    }
                    if changed {
                        let midpoint_x = (connector.start.point.x + connector.end.point.x) / 2.0;
                        connector.route = vec![
                            Point::new(midpoint_x, connector.start.point.y),
                            Point::new(midpoint_x, connector.end.point.y),
                        ];
                    }
                }
            }
        }
    }

    fn delete_selection(&mut self) -> usize {
        if self.active_layer().is_spreadsheet() {
            return self.clear_sheet_selection();
        }
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let slots = self.capture_elements(&selected);
        let count = selected.len();
        for layer in &mut self.page_mut().layers {
            layer
                .elements
                .retain(|element| !selected.contains(&element.id()));
            for element in &mut layer.elements {
                if let Element::Connector(connector) = element {
                    for endpoint in [&mut connector.start, &mut connector.end] {
                        if endpoint
                            .attachment
                            .as_ref()
                            .is_some_and(|attachment| selected.contains(&attachment.element_id))
                        {
                            endpoint.attachment = None;
                        }
                    }
                }
            }
        }
        let page_id = self.page().id;
        self.selection.clear();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        count
    }

    fn duplicate_selection(&mut self) -> usize {
        if self.selection.is_empty() || self.active_layer().locked {
            return 0;
        }
        let selected = self.selection.clone();
        let originals: Vec<Element> = self
            .page()
            .elements()
            .filter(|element| selected.contains(&element.id()))
            .cloned()
            .collect();
        let id_map: HashMap<Uuid, Uuid> = originals
            .iter()
            .map(|element| (element.id(), Uuid::new_v4()))
            .collect();
        let mut copies = Vec::with_capacity(originals.len());
        for mut element in originals {
            let new_id = id_map[&element.id()];
            element.set_id(new_id);
            element.translate(Point::new(24.0, 24.0));
            if let Element::Connector(connector) = &mut element {
                for endpoint in [&mut connector.start, &mut connector.end] {
                    if let Some(attachment) = &mut endpoint.attachment
                        && let Some(new_target) = id_map.get(&attachment.element_id)
                    {
                        attachment.element_id = *new_target;
                    }
                }
            }
            copies.push(element);
        }
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let start_index = self.active_layer().elements.len();
        let mut slots = Vec::with_capacity(copies.len());
        self.selection.clear();
        for (offset, element) in copies.into_iter().enumerate() {
            let id = element.id();
            self.active_layer_mut().elements.push(element);
            self.selection.insert(id);
            slots.push(ElementSlot {
                layer_id,
                index: start_index + offset,
                id,
                stored: None,
            });
        }
        let count = slots.len();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        count
    }

    fn apply_element_slots(&mut self, page_id: Uuid, slots: &mut [ElementSlot]) {
        let Some(page_index) = self
            .notebook
            .pages
            .iter()
            .position(|page| page.id == page_id)
        else {
            return;
        };
        self.active_page = page_index;
        for slot in slots {
            let Some(layer_index) = self.notebook.pages[page_index]
                .layers
                .iter()
                .position(|layer| layer.id == slot.layer_id)
            else {
                continue;
            };
            let elements = &mut self.notebook.pages[page_index].layers[layer_index].elements;
            if let Some(current_index) = elements.iter().position(|element| element.id() == slot.id)
            {
                if let Some(stored) = &mut slot.stored {
                    std::mem::swap(&mut elements[current_index], stored);
                } else {
                    slot.stored = Some(elements.remove(current_index));
                }
            } else if let Some(stored) = slot.stored.take() {
                elements.insert(slot.index.min(elements.len()), stored);
            }
        }
        self.selection.clear();
    }

    fn screen_to_world(&self, screen: Point) -> Point {
        Point::new(
            (screen.x - self.pan.x) / self.zoom,
            (screen.y - self.pan.y) / self.zoom,
        )
    }

    fn set_zoom_around(&mut self, requested_zoom: f32, screen: Point) {
        let world = self.screen_to_world(screen);
        self.zoom = requested_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = Point::new(
            screen.x - world.x * self.zoom,
            screen.y - world.y * self.zoom,
        );
    }

    fn begin_input(&mut self, screen: Point, pressure: f32, eraser_tip: bool, button: u32) {
        if button == 2 || self.tool == Tool::Pan {
            self.interaction = Some(Interaction::Pan {
                last_screen: screen,
            });
            return;
        }

        let world = self.screen_to_world(screen);
        if self.active_layer().is_spreadsheet()
            && button != 2
            && self.tool != Tool::Pan
            && !eraser_tip
        {
            if !self.active_layer().locked {
                self.begin_sheet_pointer(world);
            }
            return;
        }
        let effective_tool = if eraser_tip { Tool::Eraser } else { self.tool };
        match effective_tool {
            Tool::Select => {
                if let Some(id) = self.hit_test(world) {
                    if !self.selection.contains(&id) {
                        self.selection.clear();
                        self.selection.insert(id);
                        self.flash_selection();
                    }
                    let before = self.capture_elements(&self.selection);
                    self.interaction = Some(Interaction::MoveSelection {
                        last_world: world,
                        before,
                    });
                } else {
                    self.selection.clear();
                    self.interaction = Some(Interaction::Lasso {
                        start: world,
                        current: world,
                    });
                }
            }
            Tool::Pen | Tool::Highlighter => {
                if self.active_layer().locked {
                    return;
                }
                let mut style = self.style.clone();
                let kind = if effective_tool == Tool::Highlighter {
                    style.color.alpha = 0.35;
                    style.width = (style.width * 4.0).max(8.0);
                    StrokeKind::Highlighter
                } else {
                    StrokeKind::Pen
                };
                self.interaction = Some(Interaction::Stroke(Stroke {
                    id: Uuid::new_v4(),
                    kind,
                    style,
                    points: vec![StrokePoint::new(world, pressure.max(0.05))],
                }));
            }
            Tool::Eraser => {
                self.erase_at(world);
                self.interaction = Some(Interaction::Erase);
            }
            Tool::Text => {
                if self.active_layer().locked {
                    return;
                }
                let text = self.pending_text.trim();
                if !text.is_empty() {
                    self.add_element(Element::Text(TextNote {
                        id: Uuid::new_v4(),
                        origin: world,
                        text: text.to_owned(),
                        font_size: 18.0,
                        color: self.style.color,
                        max_width: Some(420.0),
                    }));
                }
                self.interaction = None;
            }
            Tool::Shape => {
                if self.active_layer().locked {
                    return;
                }
                self.interaction = Some(Interaction::Shape {
                    start: world,
                    current: world,
                });
            }
            Tool::Connector => {
                if self.active_layer().locked {
                    return;
                }
                let start = self.page().snap_endpoint(world, 18.0 / self.zoom);
                self.interaction = Some(Interaction::Connector {
                    start,
                    current: world,
                });
            }
            Tool::Pan => unreachable!(),
        }
    }

    fn update_input(&mut self, screen: Point, pressure: f32) {
        let world = self.screen_to_world(screen);
        let zoom = self.zoom;
        let sheet_hit = self
            .active_layer()
            .spreadsheet
            .as_ref()
            .and_then(|book| book.hit_cell(world));
        let mut selection_delta = None;
        let mut sheet_delta = None;
        let mut grow_at = None;
        let mut col_resize = None;
        let mut row_resize = None;
        let mut fill_addr = None;
        match self.interaction.as_mut() {
            Some(Interaction::Stroke(stroke)) => {
                let should_add = stroke
                    .points
                    .last()
                    .is_none_or(|last| last.point().distance_to(world) >= 0.7 / zoom);
                if should_add {
                    stroke
                        .points
                        .push(StrokePoint::new(world, pressure.max(0.05)));
                }
            }
            Some(Interaction::Shape { current, .. })
            | Some(Interaction::Connector { current, .. })
            | Some(Interaction::Lasso { current, .. }) => {
                *current = world;
            }
            Some(Interaction::MoveSelection { last_world, .. }) => {
                selection_delta = Some(Point::new(world.x - last_world.x, world.y - last_world.y));
                *last_world = world;
            }
            Some(Interaction::Pan { last_screen }) => {
                self.pan.x += screen.x - last_screen.x;
                self.pan.y += screen.y - last_screen.y;
                *last_screen = screen;
            }
            Some(Interaction::SheetSelect { current, .. }) => {
                if let Some(addr) = sheet_hit {
                    *current = addr;
                }
            }
            Some(Interaction::SheetMove { last_world, .. }) => {
                sheet_delta = Some(Point::new(world.x - last_world.x, world.y - last_world.y));
                *last_world = world;
            }
            Some(Interaction::SheetGrow { .. }) => grow_at = Some(world),
            Some(Interaction::SheetColResize {
                col,
                start_x,
                width_before,
            }) => {
                col_resize = Some((*col, *width_before + (world.x - *start_x)));
            }
            Some(Interaction::SheetRowResize {
                row,
                start_y,
                height_before,
            }) => {
                row_resize = Some((*row, *height_before + (world.y - *start_y)));
            }
            Some(Interaction::SheetFill { current, .. }) => {
                if let Some(addr) = sheet_hit {
                    *current = addr;
                    fill_addr = Some(addr);
                }
            }
            Some(Interaction::Erase) => self.erase_at(world),
            None => {}
        }
        if let Some(delta) = selection_delta {
            self.translate_selection(delta);
        }
        if let Some(delta) = sheet_delta
            && let Some(book) = self.active_layer_mut().spreadsheet.as_mut()
        {
            book.origin.x += delta.x;
            book.origin.y += delta.y;
        }
        if let Some(point) = grow_at
            && let Some(book) = self.active_layer_mut().spreadsheet.as_mut()
        {
            let (cols, rows) = book.visible_size_at(point);
            book.active_mut().set_visible_size(cols, rows);
        }
        if let Some((col, width)) = col_resize
            && let Some(book) = self.active_layer_mut().spreadsheet.as_mut()
        {
            book.active_mut().set_col_width(col, width);
        }
        if let Some((row, height)) = row_resize
            && let Some(book) = self.active_layer_mut().spreadsheet.as_mut()
        {
            book.active_mut().set_row_height(row, height);
        }
        if let Some(addr) = fill_addr
            && let Some(Interaction::SheetFill { source, .. }) = &self.interaction
        {
            self.sheet_range = Some(CellRange::new(source.start, addr));
        }
    }

    fn end_input(&mut self, screen: Point, pressure: f32) {
        self.update_input(screen, pressure);
        let Some(interaction) = self.interaction.take() else {
            return;
        };
        match interaction {
            Interaction::Stroke(mut stroke) => {
                if stroke.points.len() == 1 {
                    let point = stroke.points[0];
                    stroke.points.push(StrokePoint {
                        x: point.x + 0.01,
                        ..point
                    });
                }
                self.add_element(Element::Stroke(stroke));
            }
            Interaction::Shape { start, mut current } => {
                if start.distance_to(current) < 4.0 / self.zoom {
                    current = Point::new(start.x + 120.0, start.y + 72.0);
                }
                self.add_element(Element::Shape(Shape {
                    id: Uuid::new_v4(),
                    kind: self.shape_kind,
                    bounds: Rect::from_points(start, current),
                    rotation_degrees: 0.0,
                    style: self.style.clone(),
                    fill: None,
                    label: self.pending_label.trim().to_owned(),
                }));
            }
            Interaction::Connector { start, current } => {
                let end = self.page().snap_endpoint(current, 18.0 / self.zoom);
                let midpoint_x = (start.point.x + end.point.x) / 2.0;
                self.add_element(Element::Connector(Connector {
                    id: Uuid::new_v4(),
                    start: start.clone(),
                    end: end.clone(),
                    route: vec![
                        Point::new(midpoint_x, start.point.y),
                        Point::new(midpoint_x, end.point.y),
                    ],
                    style: self.style.clone(),
                    label: self.pending_label.trim().to_owned(),
                }));
            }
            Interaction::MoveSelection { before, .. } => {
                let changed = before.iter().any(|slot| {
                    slot.stored.as_ref().is_some_and(|stored| {
                        self.page()
                            .elements()
                            .find(|element| element.id() == slot.id)
                            .is_some_and(|current| current != stored)
                    })
                });
                if changed {
                    let page_id = self.page().id;
                    self.push_history(HistoryEntry::ElementsChanged {
                        page_id,
                        slots: before,
                    });
                    self.dirty = true;
                }
            }
            Interaction::Lasso { start, current } => {
                let lasso = Rect::from_points(start, current);
                self.selection = self
                    .page()
                    .layers
                    .iter()
                    .filter(|layer| layer.visible && !layer.locked)
                    .flat_map(|layer| layer.elements.iter())
                    .filter(|element| element.bounds().intersects(lasso))
                    .map(Element::id)
                    .collect();
                if !self.selection.is_empty() {
                    self.flash_selection();
                }
            }
            Interaction::Pan { .. } | Interaction::Erase => {}
            Interaction::SheetSelect { start, current } => {
                self.sheet_range = Some(CellRange::new(start, current));
            }
            Interaction::SheetMove { origin_before, .. } => {
                let current = self
                    .active_layer()
                    .spreadsheet
                    .as_ref()
                    .map(|book| book.origin);
                if current.is_some_and(|origin| origin != origin_before)
                    && let Some(mut before) = self.capture_spreadsheet()
                {
                    before.2.origin = origin_before;
                    self.push_spreadsheet_history(before);
                    self.dirty = true;
                }
            }
            Interaction::SheetGrow {
                cols_before,
                rows_before,
            } => {
                let changed = self
                    .active_layer()
                    .spreadsheet
                    .as_ref()
                    .is_some_and(|book| {
                        book.active().display_cols() != cols_before
                            || book.active().display_rows() != rows_before
                    });
                if changed && let Some(mut stored) = self.capture_spreadsheet() {
                    if let Some(sheet) = stored.2.sheets.get_mut(stored.2.active_sheet) {
                        sheet.visible_cols = cols_before;
                        sheet.visible_rows = rows_before;
                    }
                    self.push_spreadsheet_history(stored);
                    self.dirty = true;
                }
            }
            Interaction::SheetColResize {
                col, width_before, ..
            } => {
                let current = self
                    .active_layer()
                    .spreadsheet
                    .as_ref()
                    .map(|book| book.active().col_width(col));
                if current.is_some_and(|width| (width - width_before).abs() > 0.5)
                    && let Some(mut stored) = self.capture_spreadsheet()
                {
                    if let Some(sheet) = stored.2.sheets.get_mut(stored.2.active_sheet) {
                        sheet.set_col_width(col, width_before);
                    }
                    self.push_spreadsheet_history(stored);
                    self.dirty = true;
                }
            }
            Interaction::SheetRowResize {
                row, height_before, ..
            } => {
                let current = self
                    .active_layer()
                    .spreadsheet
                    .as_ref()
                    .map(|book| book.active().row_height(row));
                if current.is_some_and(|height| (height - height_before).abs() > 0.5)
                    && let Some(mut stored) = self.capture_spreadsheet()
                {
                    if let Some(sheet) = stored.2.sheets.get_mut(stored.2.active_sheet) {
                        sheet.set_row_height(row, height_before);
                    }
                    self.push_spreadsheet_history(stored);
                    self.dirty = true;
                }
            }
            Interaction::SheetFill { source, current } => {
                let target = CellRange::new(source.start, current);
                self.sheet_range = Some(target);
                if target != source {
                    self.fill_sheet_range(source, target);
                }
            }
        }
    }

    fn add_element(&mut self, element: Element) {
        if self.active_layer().is_spreadsheet() {
            return;
        }
        let id = element.id();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        self.active_layer_mut().elements.push(element);
        self.push_history(HistoryEntry::Added {
            page_id,
            layer_id,
            id,
            stored: None,
        });
        self.dirty = true;
    }

    fn erase_at(&mut self, point: Point) {
        let Some(id) = self.hit_test(point) else {
            return;
        };
        self.selection.clear();
        self.selection.insert(id);
        self.delete_selection();
        self.selection.clear();
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        if self.history.len() == HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.history.push(entry);
        self.redo.clear();
    }

    fn undo(&mut self) {
        let Some(mut entry) = self.history.pop() else {
            return;
        };
        self.toggle_history_entry(&mut entry);
        self.redo.push(entry);
        self.dirty = true;
    }

    fn redo(&mut self) {
        let Some(mut entry) = self.redo.pop() else {
            return;
        };
        self.toggle_history_entry(&mut entry);
        self.history.push(entry);
        self.dirty = true;
    }

    fn toggle_history_entry(&mut self, entry: &mut HistoryEntry) {
        match entry {
            HistoryEntry::Added {
                page_id,
                layer_id,
                id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                let Some(layer) = page.layers.iter_mut().find(|layer| layer.id == *layer_id) else {
                    return;
                };
                if let Some(index) = layer
                    .elements
                    .iter()
                    .position(|element| element.id() == *id)
                {
                    *stored = Some(layer.elements.remove(index));
                } else if let Some(element) = stored.take() {
                    layer.elements.push(element);
                }
            }
            HistoryEntry::ElementsChanged { page_id, slots } => {
                self.apply_element_slots(*page_id, slots);
            }
            HistoryEntry::PageAdded { id, stored } => {
                if let Some(index) = self.notebook.pages.iter().position(|page| page.id == *id) {
                    *stored = Some(self.notebook.pages.remove(index));
                    self.active_page = self.active_page.min(self.notebook.pages.len() - 1);
                } else if let Some(page) = stored.take() {
                    self.notebook.pages.push(page);
                    self.active_page = self.notebook.pages.len() - 1;
                }
                self.active_layer = 0;
            }
            HistoryEntry::PageRemoved { index, stored } => {
                if let Some(page) = stored.take() {
                    self.notebook
                        .pages
                        .insert((*index).min(self.notebook.pages.len()), page);
                    self.active_page = (*index).min(self.notebook.pages.len() - 1);
                } else if *index < self.notebook.pages.len() && self.notebook.pages.len() > 1 {
                    *stored = Some(self.notebook.pages.remove(*index));
                    self.active_page = (*index).min(self.notebook.pages.len() - 1);
                }
                self.active_layer = 0;
            }
            HistoryEntry::LayerAdded {
                page_id,
                id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                if let Some(index) = page.layers.iter().position(|layer| layer.id == *id) {
                    *stored = Some(page.layers.remove(index));
                } else if let Some(layer) = stored.take() {
                    page.layers.push(layer);
                }
                self.active_layer = self.active_layer.min(page.layers.len() - 1);
            }
            HistoryEntry::LayerRemoved {
                page_id,
                index,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                if let Some(layer) = stored.take() {
                    page.layers.insert((*index).min(page.layers.len()), layer);
                } else if *index < page.layers.len() && page.layers.len() > 1 {
                    *stored = Some(page.layers.remove(*index));
                }
                self.active_layer = (*index).min(page.layers.len() - 1);
            }
            HistoryEntry::SpreadsheetChanged {
                page_id,
                layer_id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                let Some(layer) = page.layers.iter_mut().find(|layer| layer.id == *layer_id) else {
                    return;
                };
                std::mem::swap(&mut layer.spreadsheet, stored);
            }
        }
        self.selection.clear();
        self.sheet_editing = None;
    }
}

fn emit_sheet_changed(state: &Rc<RefCell<CanvasState>>) {
    let listeners = state.borrow().sheet_listeners.clone();
    for listener in listeners {
        listener();
    }
}

fn emit_busy(state: &Rc<RefCell<CanvasState>>) {
    let (busy, listeners) = {
        let state = state.borrow();
        (state.interaction.is_some(), state.busy_listeners.clone())
    };
    for listener in listeners {
        listener(busy);
    }
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

fn start_canvas_fx(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    if !animations_enabled() {
        area.queue_draw();
        return;
    }
    let generation = {
        let mut state = state.borrow_mut();
        state.fx_generation = state.fx_generation.wrapping_add(1);
        state.fx_generation
    };
    let state = state.clone();
    area.add_tick_callback(move |area, _clock| {
        let active = {
            let state = state.borrow();
            if state.fx_generation != generation {
                return glib::ControlFlow::Break;
            }
            fx_active(state.selection_flash, SELECTION_FLASH_SECS)
                || fx_active(state.page_fade, PAGE_FADE_SECS)
                || fx_active(state.empty_hint, EMPTY_HINT_SECS)
        };
        area.queue_draw();
        if active {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn fx_active(started: Option<Instant>, duration: f32) -> bool {
    started.is_some_and(|started| started.elapsed().as_secs_f32() < duration)
}

fn fx_ease(started: Option<Instant>, duration: f32) -> f32 {
    let Some(started) = started else {
        return 1.0;
    };
    let progress = (started.elapsed().as_secs_f32() / duration).clamp(0.0, 1.0);
    1.0 - (1.0 - progress).powi(3)
}

fn emit_view_changed(state: &Rc<RefCell<CanvasState>>) {
    let (percent, listeners) = {
        let state = state.borrow();
        (state.zoom_percent(), state.view_listeners.clone())
    };
    for listener in listeners {
        listener(percent);
    }
}

fn schedule_autosave(state: &Rc<RefCell<CanvasState>>) {
    let (generation, path) = {
        let mut state = state.borrow_mut();
        if !state.dirty || state.path.is_none() {
            return;
        }
        state.autosave_generation = state.autosave_generation.wrapping_add(1);
        (state.autosave_generation, state.path.clone())
    };
    glib::timeout_add_local_once(Duration::from_secs(2), {
        let state = state.clone();
        move || {
            let mut state = state.borrow_mut();
            if state.autosave_generation != generation || !state.dirty {
                return;
            }
            if let Some(path) = path
                && state.notebook.save(&path).is_ok()
            {
                state.dirty = false;
            }
        }
    });
}

fn decode_pixbuf(bytes: &[u8]) -> Result<Pixbuf, DocumentError> {
    let loader = PixbufLoader::new();
    loader
        .write(bytes)
        .map_err(|error| DocumentError::Invalid(format!("could not decode image: {error}")))?;
    loader
        .close()
        .map_err(|error| DocumentError::Invalid(format!("could not finish image: {error}")))?;
    loader.pixbuf().ok_or_else(|| {
        DocumentError::Invalid("the selected file did not contain a readable image".to_owned())
    })
}

fn attach_pointer_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let drag = gtk::GestureDrag::new();
    drag.set_button(0);
    drag.connect_drag_begin({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            {
                let mut state = state.borrow_mut();
                if state.stylus_active
                    || gesture
                        .current_event()
                        .and_then(|event| event.device_tool())
                        .is_some()
                {
                    return;
                }
                state.begin_input(
                    Point::new(x as f32, y as f32),
                    1.0,
                    false,
                    gesture.current_button(),
                );
            }
            emit_busy(&state);
            start_canvas_fx(&area, &state);
            area.grab_focus();
            area.queue_draw();
        }
    });
    drag.connect_drag_update({
        let state = state.clone();
        let area = area.clone();
        move |gesture, offset_x, offset_y| {
            if state.borrow().stylus_active {
                return;
            }
            let Some((start_x, start_y)) = gesture.start_point() else {
                return;
            };
            state.borrow_mut().update_input(
                Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32),
                1.0,
            );
            area.queue_draw();
        }
    });
    drag.connect_drag_end({
        let state = state.clone();
        let area = area.clone();
        move |gesture, offset_x, offset_y| {
            if state.borrow().stylus_active {
                return;
            }
            let Some((start_x, start_y)) = gesture.start_point() else {
                return;
            };
            state.borrow_mut().end_input(
                Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32),
                1.0,
            );
            schedule_autosave(&state);
            emit_busy(&state);
            emit_sheet_changed(&state);
            start_canvas_fx(&area, &state);
            area.queue_draw();
        }
    });
    area.add_controller(drag);
}

fn attach_stylus_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let stylus = gtk::GestureStylus::new();
    stylus.connect_down({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let pressure = gesture.axis(gdk::AxisUse::Pressure).unwrap_or(1.0) as f32;
            let eraser = gesture
                .device_tool()
                .is_some_and(|tool| tool.tool_type() == gdk::DeviceToolType::Eraser);
            {
                let mut state = state.borrow_mut();
                state.stylus_active = true;
                state.begin_input(Point::new(x as f32, y as f32), pressure, eraser, 1);
            }
            emit_busy(&state);
            start_canvas_fx(&area, &state);
            area.grab_focus();
            area.queue_draw();
        }
    });
    stylus.connect_motion({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let pressure = gesture.axis(gdk::AxisUse::Pressure).unwrap_or(1.0) as f32;
            state
                .borrow_mut()
                .update_input(Point::new(x as f32, y as f32), pressure);
            area.queue_draw();
        }
    });
    stylus.connect_up({
        let state_ref = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let pressure = gesture.axis(gdk::AxisUse::Pressure).unwrap_or(1.0) as f32;
            let mut state = state_ref.borrow_mut();
            state.end_input(Point::new(x as f32, y as f32), pressure);
            state.stylus_active = false;
            drop(state);
            schedule_autosave(&state_ref);
            emit_busy(&state_ref);
            emit_sheet_changed(&state_ref);
            start_canvas_fx(&area, &state_ref);
            area.queue_draw();
        }
    });
    area.add_controller(stylus);
}

fn attach_view_controls(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::KINETIC,
    );
    scroll.connect_scroll({
        let state = state.clone();
        let area = area.clone();
        move |controller, delta_x, delta_y| {
            {
                let mut state = state.borrow_mut();
                let modifiers = controller.current_event_state();
                if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                    let cursor = controller
                        .current_event()
                        .and_then(|event| event.position())
                        .map(|(x, y)| Point::new(x as f32, y as f32))
                        .unwrap_or_else(|| {
                            Point::new(area.width() as f32 / 2.0, area.height() as f32 / 2.0)
                        });
                    state.view_animation_generation =
                        state.view_animation_generation.wrapping_add(1);
                    let zoom = state.zoom * (-delta_y as f32 * 0.12).exp();
                    state.set_zoom_around(zoom, cursor);
                } else {
                    let scale = if delta_x.abs().max(delta_y.abs()) <= 1.5 {
                        28.0
                    } else {
                        1.0
                    };
                    state.pan.x -= delta_x as f32 * scale;
                    state.pan.y -= delta_y as f32 * scale;
                }
            }
            area.queue_draw();
            emit_view_changed(&state);
            glib::Propagation::Stop
        }
    });
    area.add_controller(scroll);

    let pinch = gtk::GestureZoom::new();
    pinch.connect_begin({
        let state = state.clone();
        move |_, _| {
            let mut state = state.borrow_mut();
            state.interaction = None;
            state.pinch_start_zoom = Some(state.zoom);
        }
    });
    pinch.connect_scale_changed({
        let state = state.clone();
        let area = area.clone();
        move |gesture, scale| {
            {
                let mut state = state.borrow_mut();
                let start_zoom = state.pinch_start_zoom.unwrap_or(state.zoom);
                state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
                let center = gesture
                    .bounding_box_center()
                    .map(|(x, y)| Point::new(x as f32, y as f32))
                    .unwrap_or_else(|| {
                        Point::new(area.width() as f32 / 2.0, area.height() as f32 / 2.0)
                    });
                state.set_zoom_around(start_zoom * scale as f32, center);
            }
            area.queue_draw();
            emit_view_changed(&state);
        }
    });
    pinch.connect_end({
        let state = state.clone();
        move |_, _| state.borrow_mut().pinch_start_zoom = None
    });
    area.add_controller(pinch);
}

fn attach_keyboard_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let state = state.clone();
        let area = area.clone();
        move |_, key, _, modifiers| {
            let handled = state.borrow_mut().handle_sheet_key(key, modifiers);
            if handled {
                schedule_autosave(&state);
                emit_sheet_changed(&state);
                area.queue_draw();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    area.add_controller(keys);
}

fn draw_canvas(context: &Context, width: i32, height: i32, state: &CanvasState) {
    set_source(context, state.page().canvas.background);
    let _ = context.paint();

    let _ = context.save();
    context.translate(state.pan.x as f64, state.pan.y as f64);
    context.scale(state.zoom as f64, state.zoom as f64);

    let viewport = Rect {
        x: -state.pan.x / state.zoom,
        y: -state.pan.y / state.zoom,
        width: width as f32 / state.zoom,
        height: height as f32 / state.zoom,
    };
    if state.page().canvas.grid_visible {
        draw_grid(context, state, viewport);
    }

    for layer in state.page().layers.iter().filter(|layer| layer.visible) {
        if let Some(spreadsheet) = &layer.spreadsheet {
            draw_spreadsheet(context, state, spreadsheet, layer.id, viewport);
        }
        for element in &layer.elements {
            if element
                .bounds()
                .intersects(viewport.expand(48.0 / state.zoom))
            {
                draw_element(context, element, &state.image_cache);
            }
        }
    }
    draw_selection(context, state);
    draw_interaction(context, state);
    let _ = context.restore();
    draw_page_fade(context, width, height, state);
    draw_empty_hint(context, width, height, state);
}

fn draw_grid(context: &Context, state: &CanvasState, viewport: Rect) {
    let mut spacing = state.page().canvas.grid_spacing;
    while spacing * state.zoom < 16.0 {
        spacing *= 2.0;
    }
    let major = spacing * 4.0;
    let start_x = (viewport.x / spacing).floor() as i32 - 1;
    let end_x = ((viewport.x + viewport.width) / spacing).ceil() as i32 + 1;
    let start_y = (viewport.y / spacing).floor() as i32 - 1;
    let end_y = ((viewport.y + viewport.height) / spacing).ceil() as i32 + 1;
    context.set_line_width(1.0 / state.zoom as f64);
    context.set_source_rgba(0.42, 0.46, 0.54, 0.12);
    stroke_grid_lines(context, spacing, start_x, end_x, start_y, end_y, viewport);
    context.set_source_rgba(0.32, 0.36, 0.44, 0.22);
    let start_x = (viewport.x / major).floor() as i32 - 1;
    let end_x = ((viewport.x + viewport.width) / major).ceil() as i32 + 1;
    let start_y = (viewport.y / major).floor() as i32 - 1;
    let end_y = ((viewport.y + viewport.height) / major).ceil() as i32 + 1;
    stroke_grid_lines(context, major, start_x, end_x, start_y, end_y, viewport);
}

fn stroke_grid_lines(
    context: &Context,
    spacing: f32,
    start_x: i32,
    end_x: i32,
    start_y: i32,
    end_y: i32,
    viewport: Rect,
) {
    for x in start_x..=end_x {
        let x = x as f64 * spacing as f64;
        context.move_to(x, viewport.y as f64);
        context.line_to(x, (viewport.y + viewport.height) as f64);
    }
    for y in start_y..=end_y {
        let y = y as f64 * spacing as f64;
        context.move_to(viewport.x as f64, y);
        context.line_to((viewport.x + viewport.width) as f64, y);
    }
    let _ = context.stroke();
}

fn fill_handle_rect(bounds: Rect) -> Rect {
    Rect {
        x: bounds.x + bounds.width - 5.0,
        y: bounds.y + bounds.height - 5.0,
        width: 8.0,
        height: 8.0,
    }
}

fn hit_fill_handle(book: &Spreadsheet, range: CellRange, point: Point) -> bool {
    let handle = fill_handle_rect(book.cell_rect(range.start).union(book.cell_rect(range.end)));
    point.x >= handle.x
        && point.x <= handle.x + handle.width
        && point.y >= handle.y
        && point.y <= handle.y + handle.height
}

fn draw_spreadsheet(
    context: &Context,
    state: &CanvasState,
    book: &Spreadsheet,
    layer_id: Uuid,
    viewport: Rect,
) {
    let bounds = book.bounds();
    if !bounds.intersects(viewport.expand(24.0)) {
        return;
    }
    let sheet = book.active();
    let active = state.active_layer().id == layer_id;
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.rectangle(
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        bounds.height as f64,
    );
    let _ = context.fill();
    context.set_source_rgb(0.18, 0.42, 0.28);
    context.rectangle(
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        crate::spreadsheet::TITLE_HEIGHT as f64,
    );
    let _ = context.fill();
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    context.set_font_size(12.0);
    context.move_to((bounds.x + 10.0) as f64, (bounds.y + 17.0) as f64);
    let _ = context.show_text(&format!(
        "{} · {}×{} · {}",
        sheet.name,
        sheet.display_cols(),
        sheet.display_rows(),
        if active { "editing" } else { "spreadsheet" }
    ));

    let cols = sheet.display_cols();
    let rows = sheet.display_rows();
    let header_top = bounds.y + crate::spreadsheet::TITLE_HEIGHT;
    context.set_font_size(10.0);
    context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    let mut x = bounds.x + crate::spreadsheet::HEADER_COL_WIDTH;
    for col in 0..cols {
        let width = sheet.col_width(col);
        context.set_source_rgb(0.91, 0.93, 0.91);
        context.rectangle(
            x as f64,
            header_top as f64,
            width as f64,
            crate::spreadsheet::HEADER_ROW_HEIGHT as f64,
        );
        let _ = context.fill();
        context.set_source_rgb(0.28, 0.32, 0.28);
        context.move_to((x + 6.0) as f64, (header_top + 15.0) as f64);
        let _ = context.show_text(&col_name(col));
        x += width;
    }
    let mut y = header_top + crate::spreadsheet::HEADER_ROW_HEIGHT;
    for row in 0..rows {
        let height = sheet.row_height(row);
        context.set_source_rgb(0.91, 0.93, 0.91);
        context.rectangle(
            bounds.x as f64,
            y as f64,
            crate::spreadsheet::HEADER_COL_WIDTH as f64,
            height as f64,
        );
        let _ = context.fill();
        context.set_source_rgb(0.28, 0.32, 0.28);
        context.move_to((bounds.x + 8.0) as f64, (y + height * 0.7) as f64);
        let _ = context.show_text(&(row + 1).to_string());
        y += height;
    }

    let selected = if active { state.sheet_range } else { None };
    for col in 0..cols {
        for row in 0..rows {
            let addr = CellAddr { col, row };
            let rect = book.cell_rect(addr);
            if !rect.intersects(viewport.expand(8.0)) {
                continue;
            }
            let cell = sheet.cells.get(&addr);
            if let Some(fill) = cell.and_then(|cell| cell.style.fill) {
                set_source(context, fill);
                context.rectangle(
                    rect.x as f64,
                    rect.y as f64,
                    rect.width as f64,
                    rect.height as f64,
                );
                let _ = context.fill();
            }
            context.set_source_rgba(0.78, 0.82, 0.78, 1.0);
            context.set_line_width(0.6 / state.zoom as f64);
            context.rectangle(
                rect.x as f64,
                rect.y as f64,
                rect.width as f64,
                rect.height as f64,
            );
            let _ = context.stroke();
            let editing = state
                .sheet_editing
                .as_ref()
                .filter(|(edit_addr, _)| active && *edit_addr == addr);
            let text = if let Some((_, buffer)) = editing {
                buffer.clone()
            } else {
                book.display_cell(book.active_sheet, addr)
            };
            if text.is_empty() {
                continue;
            }
            let style = cell.map(|cell| &cell.style);
            context.select_font_face(
                "Sans",
                if style.is_some_and(|style| style.italic) {
                    cairo::FontSlant::Italic
                } else {
                    cairo::FontSlant::Normal
                },
                if style.is_some_and(|style| style.bold) {
                    cairo::FontWeight::Bold
                } else {
                    cairo::FontWeight::Normal
                },
            );
            context.set_font_size(
                style
                    .map(|style| style.font_size_or_default())
                    .unwrap_or(11.0) as f64,
            );
            set_source(
                context,
                style.map(|style| style.text_color()).unwrap_or(Color::INK),
            );
            let align = style.map(|style| style.h_align).unwrap_or(HAlign::General);
            let value = book.evaluate(book.active_sheet, addr);
            let numeric = matches!(value, Value::Number(_) | Value::Bool(_));
            let _ = context.save();
            context.rectangle(
                rect.x as f64,
                rect.y as f64,
                rect.width as f64,
                rect.height as f64,
            );
            context.clip();
            let width = context
                .text_extents(&text)
                .map(|ext| ext.width())
                .unwrap_or(0.0);
            let text_x = match align {
                HAlign::Center => rect.x as f64 + rect.width as f64 / 2.0 - width / 2.0,
                HAlign::Right => rect.x as f64 + rect.width as f64 - 5.0 - width,
                HAlign::General if numeric && editing.is_none() => {
                    rect.x as f64 + rect.width as f64 - 5.0 - width
                }
                _ => rect.x as f64 + 4.0,
            };
            context.move_to(text_x, (rect.y + rect.height * 0.72) as f64);
            let _ = context.show_text(&text);
            let _ = context.restore();
        }
    }

    if let Some(range) = selected {
        let a = book.cell_rect(range.start);
        let b = book.cell_rect(range.end);
        let highlight = a.union(b);
        context.set_source_rgba(0.12, 0.46, 0.28, 0.14);
        context.rectangle(
            highlight.x as f64,
            highlight.y as f64,
            highlight.width as f64,
            highlight.height as f64,
        );
        let _ = context.fill();
        context.set_source_rgb(0.12, 0.46, 0.28);
        context.set_line_width(1.8 / state.zoom as f64);
        context.rectangle(
            highlight.x as f64,
            highlight.y as f64,
            highlight.width as f64,
            highlight.height as f64,
        );
        let _ = context.stroke();
        let handle = fill_handle_rect(highlight);
        context.set_source_rgb(0.12, 0.46, 0.28);
        context.rectangle(
            handle.x as f64,
            handle.y as f64,
            handle.width as f64,
            handle.height as f64,
        );
        let _ = context.fill();
    }

    let tab_top = bounds.y + bounds.height - crate::spreadsheet::TAB_HEIGHT;
    context.set_source_rgb(0.94, 0.95, 0.94);
    context.rectangle(
        bounds.x as f64,
        tab_top as f64,
        bounds.width as f64,
        crate::spreadsheet::TAB_HEIGHT as f64,
    );
    let _ = context.fill();
    let mut tab_x = bounds.x + 8.0;
    context.set_font_size(10.0);
    for (index, tab) in book.sheets.iter().enumerate() {
        let width = (tab.name.len() as f32 * 7.5 + 18.0).max(48.0);
        if index == book.active_sheet {
            context.set_source_rgb(1.0, 1.0, 1.0);
        } else {
            context.set_source_rgb(0.88, 0.90, 0.88);
        }
        context.rectangle(tab_x as f64, (tab_top + 3.0) as f64, width as f64, 16.0);
        let _ = context.fill();
        context.set_source_rgb(0.16, 0.22, 0.18);
        context.move_to((tab_x + 8.0) as f64, (tab_top + 14.0) as f64);
        let _ = context.show_text(&tab.name);
        tab_x += width + 6.0;
    }

    let grow = book.grow_handle_rect();
    context.set_source_rgb(0.18, 0.42, 0.28);
    context.move_to(grow.x as f64, (grow.y + grow.height) as f64);
    context.line_to((grow.x + grow.width) as f64, (grow.y + grow.height) as f64);
    context.line_to((grow.x + grow.width) as f64, grow.y as f64);
    context.close_path();
    let _ = context.fill();
}

fn draw_interaction(context: &Context, state: &CanvasState) {
    match &state.interaction {
        Some(Interaction::Stroke(stroke)) => draw_stroke(context, stroke),
        Some(Interaction::Shape { start, current }) => draw_shape(
            context,
            &Shape {
                id: Uuid::nil(),
                kind: state.shape_kind,
                bounds: Rect::from_points(*start, *current),
                rotation_degrees: 0.0,
                style: state.style.clone(),
                fill: None,
                label: state.pending_label.clone(),
            },
        ),
        Some(Interaction::Connector { start, current }) => {
            set_source(context, state.style.color);
            context.set_line_width(state.style.width as f64);
            context.set_dash(&[7.0, 5.0], 0.0);
            context.move_to(start.point.x as f64, start.point.y as f64);
            context.line_to(current.x as f64, current.y as f64);
            let _ = context.stroke();
            context.set_dash(&[], 0.0);
        }
        Some(Interaction::Lasso { start, current }) => {
            let bounds = Rect::from_points(*start, *current);
            context.set_source_rgba(0.12, 0.38, 0.88, 0.12);
            context.rectangle(
                bounds.x as f64,
                bounds.y as f64,
                bounds.width as f64,
                bounds.height as f64,
            );
            let _ = context.fill_preserve();
            context.set_source_rgba(0.12, 0.38, 0.88, 0.9);
            context.set_line_width(1.0 / state.zoom as f64);
            context.set_dash(&[5.0 / state.zoom as f64, 4.0 / state.zoom as f64], 0.0);
            let _ = context.stroke();
            context.set_dash(&[], 0.0);
        }
        Some(Interaction::MoveSelection { .. })
        | Some(Interaction::Pan { .. })
        | Some(Interaction::Erase)
        | Some(Interaction::SheetMove { .. })
        | Some(Interaction::SheetGrow { .. })
        | Some(Interaction::SheetColResize { .. })
        | Some(Interaction::SheetRowResize { .. })
        | None => {}
        Some(Interaction::SheetSelect { start, current }) => {
            draw_sheet_range_preview(context, state, CellRange::new(*start, *current));
        }
        Some(Interaction::SheetFill { source, current }) => {
            draw_sheet_range_preview(context, state, CellRange::new(source.start, *current));
        }
    }
}

fn draw_sheet_range_preview(context: &Context, state: &CanvasState, range: CellRange) {
    let Some(book) = state.active_layer().spreadsheet.as_ref() else {
        return;
    };
    let a = book.cell_rect(range.start);
    let b = book.cell_rect(range.end);
    let bounds = a.union(b);
    context.set_source_rgba(0.12, 0.46, 0.28, 0.16);
    context.rectangle(
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        bounds.height as f64,
    );
    let _ = context.fill();
}

fn draw_selection(context: &Context, state: &CanvasState) {
    if state.selection.is_empty() {
        return;
    }
    let appear = fx_ease(state.selection_flash, SELECTION_FLASH_SECS);
    let pad = (6.0 + 10.0 * (1.0 - appear)) / state.zoom;
    let radius = 6.0 / state.zoom;
    let handle = 4.5 / state.zoom;
    for element in state
        .page()
        .visible_elements()
        .filter(|element| state.selection.contains(&element.id()))
    {
        let bounds = element.bounds().expand(pad);
        rounded_rect(context, bounds, radius);
        context.set_source_rgba(0.18, 0.44, 0.92, (0.08 + 0.10 * appear) as f64);
        let _ = context.fill_preserve();
        context.set_source_rgba(0.18, 0.44, 0.92, (0.45 + 0.47 * appear) as f64);
        context.set_line_width((1.2 + 1.2 * (1.0 - appear)) as f64 / state.zoom as f64);
        let _ = context.stroke();
        context.set_source_rgba(0.18, 0.44, 0.92, appear as f64);
        for (x, y) in [
            (bounds.x, bounds.y),
            (bounds.x + bounds.width, bounds.y),
            (bounds.x, bounds.y + bounds.height),
            (bounds.x + bounds.width, bounds.y + bounds.height),
        ] {
            context.rectangle(
                (x - handle / 2.0) as f64,
                (y - handle / 2.0) as f64,
                handle as f64,
                handle as f64,
            );
            let _ = context.fill();
        }
    }
}

fn draw_page_fade(context: &Context, width: i32, height: i32, state: &CanvasState) {
    if !fx_active(state.page_fade, PAGE_FADE_SECS) {
        return;
    }
    let alpha = 0.55 * (1.0 - fx_ease(state.page_fade, PAGE_FADE_SECS));
    let color = state.page().canvas.background;
    context.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        (color.alpha * alpha) as f64,
    );
    context.rectangle(0.0, 0.0, width as f64, height as f64);
    let _ = context.fill();
}

fn draw_empty_hint(context: &Context, width: i32, height: i32, state: &CanvasState) {
    if state.interaction.is_some()
        || state.page().visible_elements().next().is_some()
        || state
            .page()
            .layers
            .iter()
            .any(|layer| layer.visible && layer.spreadsheet.is_some())
    {
        return;
    }
    let appear = fx_ease(state.empty_hint, EMPTY_HINT_SECS);
    let lift = 10.0 * (1.0 - appear) as f64;
    let title = "Start a page";
    let subtitle = "Draw, type a note, or add a spreadsheet layer";
    context.set_font_size(20.0);
    context.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    let title_ext = context.text_extents(title).ok();
    context.set_font_size(13.0);
    let subtitle_ext = context.text_extents(subtitle).ok();
    let title_width = title_ext.as_ref().map(|ext| ext.width()).unwrap_or(0.0);
    let subtitle_width = subtitle_ext.as_ref().map(|ext| ext.width()).unwrap_or(0.0);
    let card_width = title_width.max(subtitle_width) + 64.0;
    let card_height = 88.0;
    let card_x = width as f64 / 2.0 - card_width / 2.0;
    let card_y = height as f64 / 2.0 - card_height / 2.0 - lift;
    let bg = state.page().canvas.background;
    let luma = 0.2126 * bg.red + 0.7152 * bg.green + 0.0722 * bg.blue;
    let card = if luma > 0.5 {
        (1.0, 1.0, 1.0, 0.62)
    } else {
        (0.16, 0.18, 0.22, 0.72)
    };
    context.set_source_rgba(card.0, card.1, card.2, card.3 * appear as f64);
    rounded_rect(
        context,
        Rect {
            x: card_x as f32,
            y: card_y as f32,
            width: card_width as f32,
            height: card_height as f32,
        },
        18.0,
    );
    let _ = context.fill();
    let (title_r, title_g, title_b) = if luma > 0.5 {
        (0.22, 0.26, 0.32)
    } else {
        (0.93, 0.95, 0.97)
    };
    context.set_font_size(20.0);
    context.set_source_rgba(title_r, title_g, title_b, (0.88 * appear) as f64);
    if let Some(ext) = title_ext {
        context.move_to(
            width as f64 / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
            card_y + 36.0,
        );
        let _ = context.show_text(title);
    }
    context.set_font_size(13.0);
    context.set_source_rgba(title_r, title_g, title_b, (0.62 * appear) as f64);
    if let Some(ext) = subtitle_ext {
        context.move_to(
            width as f64 / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
            card_y + 62.0,
        );
        let _ = context.show_text(subtitle);
    }
}

fn rounded_rect(context: &Context, bounds: Rect, radius: f32) {
    let radius = radius
        .min(bounds.width.abs() / 2.0)
        .min(bounds.height.abs() / 2.0)
        .max(0.0) as f64;
    let x = bounds.x as f64;
    let y = bounds.y as f64;
    let width = bounds.width as f64;
    let height = bounds.height as f64;
    context.new_sub_path();
    context.arc(
        x + width - radius,
        y + radius,
        radius,
        -90.0_f64.to_radians(),
        0.0,
    );
    context.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        90.0_f64.to_radians(),
    );
    context.arc(
        x + radius,
        y + height - radius,
        radius,
        90.0_f64.to_radians(),
        180.0_f64.to_radians(),
    );
    context.arc(
        x + radius,
        y + radius,
        radius,
        180.0_f64.to_radians(),
        270.0_f64.to_radians(),
    );
    context.close_path();
}

fn draw_element(context: &Context, element: &Element, image_cache: &HashMap<Uuid, Pixbuf>) {
    match element {
        Element::Stroke(stroke) => draw_stroke(context, stroke),
        Element::Text(text) => draw_text(context, text),
        Element::Shape(shape) => draw_shape(context, shape),
        Element::Connector(connector) => draw_connector(context, connector),
        Element::Media(media) => draw_media(context, media, image_cache),
    }
}

fn draw_stroke(context: &Context, stroke: &Stroke) {
    if stroke.points.len() < 2 {
        return;
    }
    set_source(context, stroke.style.color);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);
    for pair in stroke.points.windows(2) {
        let pressure = ((pair[0].pressure + pair[1].pressure) / 2.0).clamp(0.12, 1.0);
        context.set_line_width((stroke.style.width * pressure) as f64);
        context.move_to(pair[0].x as f64, pair[0].y as f64);
        context.line_to(pair[1].x as f64, pair[1].y as f64);
        let _ = context.stroke();
    }
}

fn draw_text(context: &Context, text: &TextNote) {
    set_source(context, text.color);
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    context.set_font_size(text.font_size as f64);
    for (index, line) in text.text.lines().enumerate() {
        context.move_to(
            text.origin.x as f64,
            (text.origin.y + index as f32 * text.font_size * 1.25) as f64,
        );
        let _ = context.show_text(line);
    }
}

fn draw_connector(context: &Context, connector: &Connector) {
    set_source(context, connector.style.color);
    context.set_line_width(connector.style.width as f64);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);
    context.move_to(
        connector.start.point.x as f64,
        connector.start.point.y as f64,
    );
    for point in &connector.route {
        context.line_to(point.x as f64, point.y as f64);
    }
    context.line_to(connector.end.point.x as f64, connector.end.point.y as f64);
    let _ = context.stroke();
    draw_endpoint(
        context,
        connector.start.point,
        connector.start.attachment.is_some(),
    );
    draw_endpoint(
        context,
        connector.end.point,
        connector.end.attachment.is_some(),
    );
    if !connector.label.is_empty() {
        let center = Point::new(
            (connector.start.point.x + connector.end.point.x) / 2.0,
            (connector.start.point.y + connector.end.point.y) / 2.0 - 8.0,
        );
        draw_label(context, center, &connector.label, connector.style.color);
    }
}

fn draw_endpoint(context: &Context, point: Point, attached: bool) {
    let radius = if attached { 4.0 } else { 2.5 };
    context.arc(
        point.x as f64,
        point.y as f64,
        radius,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.fill();
}

fn draw_media(context: &Context, media: &MediaElement, image_cache: &HashMap<Uuid, Pixbuf>) {
    if media.kind == MediaKind::Image
        && let Some(pixbuf) = image_cache.get(&media.asset_id)
    {
        let _ = context.save();
        context.rectangle(
            media.bounds.x as f64,
            media.bounds.y as f64,
            media.bounds.width as f64,
            media.bounds.height as f64,
        );
        context.clip();
        context.translate(media.bounds.x as f64, media.bounds.y as f64);
        context.scale(
            media.bounds.width as f64 / pixbuf.width() as f64,
            media.bounds.height as f64 / pixbuf.height() as f64,
        );
        context.set_source_pixbuf(pixbuf, 0.0, 0.0);
        let _ = context.paint();
        let _ = context.restore();
        return;
    }

    context.set_source_rgba(0.93, 0.94, 0.96, 1.0);
    context.rectangle(
        media.bounds.x as f64,
        media.bounds.y as f64,
        media.bounds.width as f64,
        media.bounds.height as f64,
    );
    let _ = context.fill_preserve();
    context.set_source_rgba(0.25, 0.28, 0.34, 1.0);
    context.set_line_width(1.5);
    let _ = context.stroke();
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Bold,
    );
    context.set_font_size(18.0);
    context.move_to(
        (media.bounds.x + 16.0) as f64,
        (media.bounds.y + 32.0) as f64,
    );
    let title = if media.kind == MediaKind::Pdf {
        format!("PDF · {}", media.caption)
    } else {
        media.caption.clone()
    };
    let _ = context.show_text(&title);
}

fn draw_shape(context: &Context, shape: &Shape) {
    let bounds = shape.bounds;
    if bounds.width < f32::EPSILON || bounds.height < f32::EPSILON {
        return;
    }
    let _ = context.save();
    let center = bounds.center();
    context.translate(center.x as f64, center.y as f64);
    context.rotate(shape.rotation_degrees.to_radians() as f64);
    context.translate(-center.x as f64, -center.y as f64);
    set_source(context, shape.style.color);
    context.set_line_width(shape.style.width as f64);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);

    match shape.kind {
        ShapeKind::Rectangle => {
            context.rectangle(
                bounds.x as f64,
                bounds.y as f64,
                bounds.width as f64,
                bounds.height as f64,
            );
            fill_and_stroke(context, shape.fill, shape.style.color);
        }
        ShapeKind::Ellipse => {
            ellipse_path(context, bounds);
            fill_and_stroke(context, shape.fill, shape.style.color);
        }
        ShapeKind::Resistor | ShapeKind::Spring => {
            context.move_to(bounds.x as f64, bounds.center().y as f64);
            for index in 0..=8 {
                let x = bounds.x + bounds.width * (index as f32 + 1.0) / 10.0;
                let y = if index % 2 == 0 {
                    bounds.y + bounds.height * 0.2
                } else {
                    bounds.y + bounds.height * 0.8
                };
                context.line_to(x as f64, y as f64);
            }
            context.line_to((bounds.x + bounds.width) as f64, bounds.center().y as f64);
            let _ = context.stroke();
        }
        ShapeKind::Capacitor => {
            let left = bounds.x + bounds.width * 0.42;
            let right = bounds.x + bounds.width * 0.58;
            context.move_to(bounds.x as f64, bounds.center().y as f64);
            context.line_to(left as f64, bounds.center().y as f64);
            context.move_to(right as f64, bounds.center().y as f64);
            context.line_to((bounds.x + bounds.width) as f64, bounds.center().y as f64);
            context.move_to(left as f64, bounds.y as f64);
            context.line_to(left as f64, (bounds.y + bounds.height) as f64);
            context.move_to(right as f64, bounds.y as f64);
            context.line_to(right as f64, (bounds.y + bounds.height) as f64);
            let _ = context.stroke();
        }
        ShapeKind::Ground => {
            let center_x = bounds.center().x;
            context.move_to(center_x as f64, bounds.y as f64);
            context.line_to(center_x as f64, (bounds.y + bounds.height * 0.45) as f64);
            for (offset, y, width) in [(0.0, 0.45, 1.0), (0.18, 0.68, 0.64), (0.36, 0.9, 0.28)] {
                context.move_to(
                    (bounds.x + bounds.width * offset) as f64,
                    (bounds.y + bounds.height * y) as f64,
                );
                context.line_to(
                    (bounds.x + bounds.width * (offset + width)) as f64,
                    (bounds.y + bounds.height * y) as f64,
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Motor => {
            ellipse_path(context, bounds);
            let _ = context.stroke();
            context.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Bold,
            );
            context.set_font_size((bounds.height * 0.45).min(bounds.width * 0.45) as f64);
            context.move_to(
                (bounds.center().x - bounds.width * 0.16) as f64,
                (bounds.center().y + bounds.height * 0.16) as f64,
            );
            let _ = context.show_text("M");
        }
        ShapeKind::Gear => {
            ellipse_path(context, bounds);
            let _ = context.stroke();
            context.arc(
                bounds.center().x as f64,
                bounds.center().y as f64,
                bounds.width.min(bounds.height) as f64 * 0.16,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.stroke();
            for index in 0..8 {
                let angle = index as f64 * std::f64::consts::TAU / 8.0;
                let inner = bounds.width.min(bounds.height) as f64 * 0.36;
                let outer = bounds.width.min(bounds.height) as f64 * 0.54;
                context.move_to(
                    bounds.center().x as f64 + inner * angle.cos(),
                    bounds.center().y as f64 + inner * angle.sin(),
                );
                context.line_to(
                    bounds.center().x as f64 + outer * angle.cos(),
                    bounds.center().y as f64 + outer * angle.sin(),
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Bearing => {
            ellipse_path(context, bounds);
            let _ = context.stroke();
            context.arc(
                bounds.center().x as f64,
                bounds.center().y as f64,
                bounds.width.min(bounds.height) as f64 * 0.18,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.stroke();
            for index in 0..6 {
                let angle = index as f64 * std::f64::consts::TAU / 6.0;
                context.arc(
                    bounds.center().x as f64
                        + bounds.width.min(bounds.height) as f64 * 0.34 * angle.cos(),
                    bounds.center().y as f64
                        + bounds.width.min(bounds.height) as f64 * 0.34 * angle.sin(),
                    bounds.width.min(bounds.height) as f64 * 0.06,
                    0.0,
                    std::f64::consts::TAU,
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Beam => {
            context.rectangle(
                bounds.x as f64,
                bounds.y as f64,
                bounds.width as f64,
                bounds.height as f64,
            );
            let _ = context.stroke();
            let mut x = bounds.x - bounds.height;
            while x < bounds.x + bounds.width {
                context.move_to(x.max(bounds.x) as f64, (bounds.y + bounds.height) as f64);
                context.line_to(
                    (x + bounds.height).min(bounds.x + bounds.width) as f64,
                    bounds.y as f64,
                );
                x += bounds.height * 0.65;
            }
            let _ = context.stroke();
        }
    }
    let _ = context.restore();

    if !shape.label.is_empty() {
        draw_label(
            context,
            Point::new(bounds.center().x, bounds.y + bounds.height + 18.0),
            &shape.label,
            shape.style.color,
        );
    }
}

fn ellipse_path(context: &Context, bounds: Rect) {
    let _ = context.save();
    context.translate(bounds.center().x as f64, bounds.center().y as f64);
    context.scale(bounds.width as f64 / 2.0, bounds.height as f64 / 2.0);
    context.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    let _ = context.restore();
}

fn fill_and_stroke(context: &Context, fill: Option<Color>, stroke: Color) {
    if let Some(fill) = fill {
        set_source(context, fill);
        let _ = context.fill_preserve();
        set_source(context, stroke);
    }
    let _ = context.stroke();
}

fn draw_label(context: &Context, origin: Point, label: &str, color: Color) {
    set_source(context, color);
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    context.set_font_size(14.0);
    let width = context
        .text_extents(label)
        .map(|value| value.width())
        .unwrap_or(0.0);
    context.move_to(origin.x as f64 - width / 2.0, origin.y as f64);
    let _ = context.show_text(label);
}

fn set_source(context: &Context, color: Color) {
    context.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        color.alpha as f64,
    );
}
