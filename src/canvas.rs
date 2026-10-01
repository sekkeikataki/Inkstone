use crate::document::{
    BackgroundPattern, Color, Connector, DocumentError, Element, Endpoint, ListStyle,
    MAX_STROKE_WIDTH, MIN_STROKE_WIDTH, MediaElement, MediaKind, PageLayout, PaperSize, Point,
    Rect, Shape, ShapeKind, Stroke, StrokeKind, StrokePoint, StrokeStyle, TableElement, TagElement,
    TagKind, TextNote, import_svg_elements, mm_to_pt, pt_to_mm,
};
use crate::local::{self, AlignMode, PageTemplate};
use crate::notebook::{Asset, Layer, Notebook, NotebookPage, SearchHit, Section};
use crate::pdf;
use base64::Engine;
use cairo::{Format, ImageSurface, PdfSurface};
use gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk::cairo::{Context, LineCap, LineJoin};
use gtk::gdk;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 16.0;
const HISTORY_LIMIT: usize = 256;
const SELECTION_FLASH_SECS: f32 = 0.28;
const PAGE_FADE_SECS: f32 = 0.26;
const EMPTY_HINT_SECS: f32 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayStatus {
    pub active: bool,
    pub playing: bool,
    pub finished: bool,
    pub progress: f32,
    pub speed: f32,
    pub elapsed_secs: f32,
    pub duration_secs: f32,
}

impl ReplayStatus {
    pub fn idle() -> Self {
        Self {
            active: false,
            playing: false,
            finished: false,
            progress: 0.0,
            speed: 1.0,
            elapsed_secs: 0.0,
            duration_secs: 0.0,
        }
    }
}

pub const REPLAY_SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

struct ReplayPlayback {
    elapsed_secs: f32,
    last_tick: Instant,
    playing: bool,
    speed: f32,
    generation: u64,
}

impl ReplayPlayback {
    fn commit(&mut self) {
        if !self.playing {
            return;
        }
        let dt = self.last_tick.elapsed().as_secs_f32() * self.speed;
        self.elapsed_secs += dt;
        self.last_tick = Instant::now();
    }

    fn seconds(&self) -> f32 {
        if self.playing {
            self.elapsed_secs + self.last_tick.elapsed().as_secs_f32() * self.speed
        } else {
            self.elapsed_secs
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Select,
    #[default]
    Pen,
    Brush,
    Highlighter,
    Eraser,
    Pan,
    Text,
    Shape,
    Connector,
    Space,
    Measure,
}

impl Tool {
    pub const ALL: [Self; 11] = [
        Self::Select,
        Self::Pen,
        Self::Brush,
        Self::Highlighter,
        Self::Eraser,
        Self::Pan,
        Self::Text,
        Self::Shape,
        Self::Connector,
        Self::Space,
        Self::Measure,
    ];
    pub const NAMES: [&'static str; 11] = [
        "Select",
        "Pen",
        "Brush",
        "Highlighter",
        "Eraser",
        "Pan",
        "Text",
        "Shape",
        "Connector",
        "Space",
        "Measure",
    ];
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
    pending_font_size: f32,
    pending_bold: bool,
    pending_italic: bool,
    pending_underline: bool,
    pending_list: ListStyle,
    pending_href: Option<String>,
    ink_to_shape: bool,
    ruler: bool,
    constrain: bool,
    fill_enabled: bool,
    stabilizer: bool,
    ignore_touch: bool,
    last_stylus: Option<Instant>,
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
    text_listeners: Vec<Rc<dyn Fn(String)>>,
    tool_listeners: Vec<Rc<dyn Fn(Tool)>>,
    text_loaded: bool,
    busy_listeners: Vec<Rc<dyn Fn(bool)>>,
    selection_flash: Option<Instant>,
    page_fade: Option<Instant>,
    empty_hint: Option<Instant>,
    fx_generation: u64,
    editing_table: Option<(Uuid, usize)>,
    audio_child: Option<Child>,
    audio_path: Option<PathBuf>,
    replay: Option<ReplayPlayback>,
    replay_speed: f32,
    replay_generation: u64,
    replay_listeners: Vec<Rc<dyn Fn(ReplayStatus)>>,
    page_defaults: PageDefaults,
    runtime: RuntimePrefs,
    ink_pref_listeners: Vec<Rc<dyn Fn()>>,
}

#[derive(Clone, Copy, Debug)]
pub struct PageDefaults {
    pub paper: PaperSize,
    pub pattern: BackgroundPattern,
    pub grid_mm: f32,
    pub night: bool,
    pub layout: PageLayout,
    pub template: PageTemplate,
}

impl Default for PageDefaults {
    fn default() -> Self {
        Self {
            paper: PaperSize::Infinite,
            pattern: BackgroundPattern::Grid,
            grid_mm: 5.0,
            night: false,
            layout: PageLayout::Infinite,
            template: PageTemplate::Blank,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimePrefs {
    pub stabilizer_strength: f32,
    pub palm_reject_ms: u64,
    pub use_tilt: bool,
    pub use_pressure: bool,
    pub barrel_eraser: bool,
    pub highlighter_alpha: f32,
    pub highlighter_width_scale: f32,
    pub brush_width_scale: f32,
    pub eraser_scale: f32,
    pub iso_angle_snap: bool,
    pub tool_shortcuts: bool,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub animate_zoom: bool,
    pub show_empty_hint: bool,
    pub page_fade: bool,
    pub selection_flash: bool,
    pub autosave_ms: u64,
    pub jpeg_quality: i32,
    pub pdf_dpi: u32,
    pub table_cols: u32,
    pub table_rows: u32,
    pub date_stamp: local::DateStamp,
    pub insert_after_current: bool,
    pub zoom_step: f32,
    pub startup_zoom: f32,
}

impl Default for RuntimePrefs {
    fn default() -> Self {
        Self {
            stabilizer_strength: 0.62,
            palm_reject_ms: 450,
            use_tilt: true,
            use_pressure: true,
            barrel_eraser: true,
            highlighter_alpha: 0.35,
            highlighter_width_scale: 6.0,
            brush_width_scale: 1.6,
            eraser_scale: 3.0,
            iso_angle_snap: true,
            tool_shortcuts: true,
            min_zoom: MIN_ZOOM,
            max_zoom: MAX_ZOOM,
            animate_zoom: true,
            show_empty_hint: true,
            page_fade: true,
            selection_flash: true,
            autosave_ms: 2000,
            jpeg_quality: 92,
            pdf_dpi: 120,
            table_cols: 4,
            table_rows: 3,
            date_stamp: local::DateStamp::DateTime,
            insert_after_current: true,
            zoom_step: 1.2,
            startup_zoom: 1.0,
        }
    }
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
    Resize {
        origin: Point,
        start: Point,
        before: Vec<ElementSlot>,
    },
    Rotate {
        center: Point,
        start_angle: f32,
        before: Vec<ElementSlot>,
    },
    Space {
        start_y: f32,
        last_y: f32,
        before: Vec<ElementSlot>,
    },
    Pan {
        last_screen: Point,
    },
    Erase {
        page_id: Uuid,
        layer_id: Uuid,
        before: Vec<Element>,
        changed: bool,
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
    LayerReordered {
        page_id: Uuid,
        layer_id: Uuid,
        stored: Vec<Element>,
    },
}

#[derive(Clone)]
struct ElementSlot {
    layer_id: Uuid,
    index: usize,
    id: Uuid,
    stored: Option<Element>,
}

const CLIPBOARD_FORMAT: &str = "inkstone.clipboard";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ClipboardPayload {
    format: String,
    version: u32,
    elements: Vec<Element>,
    assets: Vec<Asset>,
}

#[derive(Clone, Copy)]
enum TransformHandle {
    Resize { origin: Point },
    Rotate { center: Point },
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
            pending_font_size: 18.0,
            pending_bold: false,
            pending_italic: false,
            pending_underline: false,
            pending_list: ListStyle::None,
            pending_href: None,
            ink_to_shape: false,
            ruler: false,
            constrain: false,
            fill_enabled: false,
            stabilizer: false,
            ignore_touch: true,
            last_stylus: None,
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
            text_listeners: Vec::new(),
            tool_listeners: Vec::new(),
            text_loaded: false,
            busy_listeners: Vec::new(),
            selection_flash: None,
            page_fade: None,
            empty_hint: Some(Instant::now()),
            fx_generation: 0,
            editing_table: None,
            audio_child: None,
            audio_path: None,
            replay: None,
            replay_speed: 1.0,
            replay_generation: 0,
            replay_listeners: Vec::new(),
            page_defaults: PageDefaults::default(),
            runtime: RuntimePrefs::default(),
            ink_pref_listeners: Vec::new(),
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
        attach_keys(&area, &state);
        attach_file_drop(&area, &state);
        area.connect_realize({
            let state = state.clone();
            move |area| {
                {
                    let mut state = state.borrow_mut();
                    if state.runtime.show_empty_hint
                        && state.page().visible_elements().next().is_none()
                    {
                        state.empty_hint = Some(Instant::now());
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

    pub fn set_tool(&self, tool: Tool) {
        let mut state = self.state.borrow_mut();
        if state.tool == tool && state.interaction.is_none() {
            return;
        }
        state.tool = tool;
        state.interaction = None;
        let listeners = state.tool_listeners.clone();
        drop(state);
        for listener in listeners {
            listener(tool);
        }
        let cursor = match tool {
            Tool::Select => "default",
            Tool::Pen | Tool::Brush | Tool::Highlighter | Tool::Shape | Tool::Connector => {
                "crosshair"
            }
            Tool::Eraser => "cell",
            Tool::Pan => "grab",
            Tool::Text => "text",
            Tool::Space => "ns-resize",
            Tool::Measure => "crosshair",
        };
        self.area.set_cursor_from_name(Some(cursor));
        self.area.queue_draw();
    }

    pub fn current_tool(&self) -> Tool {
        self.state.borrow().tool
    }

    pub fn connect_tool_changed(&self, callback: impl Fn(Tool) + 'static) {
        self.state
            .borrow_mut()
            .tool_listeners
            .push(Rc::new(callback));
    }

    pub fn connect_replay_changed(&self, callback: impl Fn(ReplayStatus) + 'static) {
        self.state
            .borrow_mut()
            .replay_listeners
            .push(Rc::new(callback));
    }

    pub fn replay_status(&self) -> ReplayStatus {
        self.state.borrow().replay_status()
    }

    pub fn start_replay(&self) -> bool {
        let started = {
            let mut state = self.state.borrow_mut();
            state.start_replay()
        };
        if started {
            emit_replay(&self.state);
            start_replay_ticks(&self.area, &self.state);
            self.area.queue_draw();
        }
        started
    }

    pub fn stop_replay(&self) {
        self.state.borrow_mut().replay = None;
        emit_replay(&self.state);
        self.area.queue_draw();
    }

    pub fn toggle_replay(&self) {
        if self.state.borrow().replay.is_none() {
            let _ = self.start_replay();
            return;
        }
        let should_tick = {
            let mut state = self.state.borrow_mut();
            let duration = state.replay_duration();
            let Some(replay) = state.replay.as_mut() else {
                return;
            };
            if replay.playing {
                replay.commit();
                replay.playing = false;
                false
            } else {
                if duration > 0.0 && replay.elapsed_secs >= duration - 0.0005 {
                    replay.elapsed_secs = 0.0;
                }
                replay.last_tick = Instant::now();
                replay.playing = true;
                true
            }
        };
        emit_replay(&self.state);
        if should_tick {
            start_replay_ticks(&self.area, &self.state);
        }
        self.area.queue_draw();
    }

    pub fn set_replay_speed(&self, speed: f32) {
        let speed = if REPLAY_SPEEDS
            .iter()
            .any(|candidate| (*candidate - speed).abs() < f32::EPSILON)
        {
            speed
        } else {
            1.0
        };
        {
            let mut state = self.state.borrow_mut();
            state.replay_speed = speed;
            if let Some(replay) = state.replay.as_mut() {
                replay.commit();
                replay.speed = speed;
                replay.last_tick = Instant::now();
            }
        }
        emit_replay(&self.state);
        self.area.queue_draw();
    }

    pub fn set_shape_kind(&self, kind: ShapeKind) {
        self.state.borrow_mut().shape_kind = kind;
    }

    pub fn set_width(&self, width: f32) {
        self.state.borrow_mut().style.width = width.clamp(MIN_STROKE_WIDTH, MAX_STROKE_WIDTH);
    }

    pub fn current_width(&self) -> f32 {
        self.state.borrow().style.width
    }

    pub fn set_dashed(&self, dashed: bool) {
        self.state.borrow_mut().style.dashed = dashed;
    }

    pub fn is_dashed(&self) -> bool {
        self.state.borrow().style.dashed
    }

    pub fn set_fill_enabled(&self, enabled: bool) {
        self.state.borrow_mut().fill_enabled = enabled;
    }

    pub fn fill_enabled(&self) -> bool {
        self.state.borrow().fill_enabled
    }

    pub fn set_font_size(&self, size: f32) {
        self.state.borrow_mut().pending_font_size = size.clamp(6.0, 240.0);
    }

    pub fn current_font_size(&self) -> f32 {
        self.state.borrow().pending_font_size
    }

    pub fn set_text_bold(&self, bold: bool) {
        self.state.borrow_mut().pending_bold = bold;
    }

    pub fn text_bold(&self) -> bool {
        self.state.borrow().pending_bold
    }

    pub fn set_text_italic(&self, italic: bool) {
        self.state.borrow_mut().pending_italic = italic;
    }

    pub fn text_italic(&self) -> bool {
        self.state.borrow().pending_italic
    }

    pub fn set_text_underline(&self, underline: bool) {
        self.state.borrow_mut().pending_underline = underline;
    }

    pub fn text_underline(&self) -> bool {
        self.state.borrow().pending_underline
    }

    pub fn set_list_style(&self, list: ListStyle) {
        self.state.borrow_mut().pending_list = list;
    }

    pub fn list_style(&self) -> ListStyle {
        self.state.borrow().pending_list
    }

    pub fn set_link(&self, href: Option<String>) {
        self.state.borrow_mut().pending_href = href.filter(|value| !value.trim().is_empty());
    }

    pub fn set_ink_to_shape(&self, enabled: bool) {
        self.state.borrow_mut().ink_to_shape = enabled;
        emit_ink_prefs(&self.state);
    }

    pub fn ink_to_shape(&self) -> bool {
        self.state.borrow().ink_to_shape
    }

    pub fn set_ruler(&self, enabled: bool) {
        self.state.borrow_mut().ruler = enabled;
        self.area.queue_draw();
        emit_ink_prefs(&self.state);
    }

    pub fn ruler(&self) -> bool {
        self.state.borrow().ruler
    }

    pub fn set_stabilizer(&self, enabled: bool) {
        self.state.borrow_mut().stabilizer = enabled;
        emit_ink_prefs(&self.state);
    }

    pub fn stabilizer(&self) -> bool {
        self.state.borrow().stabilizer
    }

    pub fn set_ignore_touch(&self, enabled: bool) {
        self.state.borrow_mut().ignore_touch = enabled;
        emit_ink_prefs(&self.state);
    }

    pub fn ignore_touch(&self) -> bool {
        self.state.borrow().ignore_touch
    }

    pub fn connect_ink_prefs_changed(&self, callback: impl Fn() + 'static) {
        self.state
            .borrow_mut()
            .ink_pref_listeners
            .push(Rc::new(callback));
    }

    pub fn set_page_defaults(&self, defaults: PageDefaults) {
        self.state.borrow_mut().page_defaults = defaults;
    }

    pub fn page_defaults(&self) -> PageDefaults {
        self.state.borrow().page_defaults
    }

    pub fn apply_page_defaults(&self) {
        let defaults = self.state.borrow().page_defaults;
        self.set_paper_size(defaults.paper);
        self.set_pattern(defaults.pattern);
        self.set_grid_spacing_mm(defaults.grid_mm);
        self.set_layout(defaults.layout);
        if defaults.night {
            self.apply_night_paper();
        }
        if defaults.template != PageTemplate::Blank {
            self.apply_template(defaults.template);
        }
    }

    pub fn set_runtime_prefs(&self, prefs: RuntimePrefs) {
        self.state.borrow_mut().runtime = prefs;
    }

    pub fn runtime_prefs(&self) -> RuntimePrefs {
        self.state.borrow().runtime
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_style_defaults(
        &self,
        tool: Tool,
        color: Color,
        width_mm: f32,
        dashed: bool,
        fill: bool,
        shape: ShapeKind,
        font_size: f32,
        list: ListStyle,
        bold: bool,
    ) {
        self.set_tool(tool);
        self.set_color(color);
        self.set_width(mm_to_pt(width_mm));
        self.set_dashed(dashed);
        self.set_fill_enabled(fill);
        self.set_shape_kind(shape);
        self.set_font_size(font_size);
        self.set_list_style(list);
        self.state.borrow_mut().pending_bold = bold;
    }

    pub fn apply_ink_preferences(
        &self,
        stabilizer: bool,
        ignore_touch: bool,
        ink_to_shape: bool,
        ruler: bool,
        replay_speed: f32,
    ) {
        {
            let mut state = self.state.borrow_mut();
            state.stabilizer = stabilizer;
            state.ignore_touch = ignore_touch;
            state.ink_to_shape = ink_to_shape;
            state.ruler = ruler;
            state.replay_speed = replay_speed;
            if let Some(replay) = state.replay.as_mut() {
                replay.commit();
                replay.speed = replay_speed;
                replay.last_tick = Instant::now();
            }
        }
        emit_ink_prefs(&self.state);
        emit_replay(&self.state);
        self.area.queue_draw();
    }

    pub fn set_color(&self, color: Color) {
        self.state.borrow_mut().style.color = color;
    }

    pub fn current_color(&self) -> Color {
        self.state.borrow().style.color
    }

    pub fn set_text(&self, text: String) {
        let mut state = self.state.borrow_mut();
        state.pending_text = text.clone();
        if let Some((table_id, cell)) = state.editing_table {
            for element in state
                .page_mut()
                .layers
                .iter_mut()
                .flat_map(|layer| layer.elements.iter_mut())
            {
                if let Element::Table(table) = element
                    && table.id == table_id
                    && let Some(slot) = table.cells.get_mut(cell)
                {
                    *slot = text.clone();
                    state.dirty = true;
                    break;
                }
            }
        }
        if state.tool == Tool::Text && state.selection.len() == 1 {
            let id = *state.selection.iter().next().expect("one selected");
            let href = local::parse_page_links(&text).into_iter().find_map(|name| {
                state
                    .notebook
                    .pages
                    .iter()
                    .find(|page| page.title.eq_ignore_ascii_case(&name))
                    .map(|page| local::page_link_href(page.id))
            });
            for element in state
                .page_mut()
                .layers
                .iter_mut()
                .flat_map(|layer| layer.elements.iter_mut())
            {
                if let Element::Text(note) = element
                    && note.id == id
                {
                    note.text = text.clone();
                    if href.is_some() {
                        note.href = href.clone();
                    }
                    state.dirty = true;
                    break;
                }
            }
        }
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
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
        state.zoom = state
            .runtime
            .startup_zoom
            .clamp(state.runtime.min_zoom, state.runtime.max_zoom);
        state.dirty = false;
        state.history.clear();
        state.redo.clear();
        state.selection.clear();
        state.image_cache.clear();
        let defaults = state.page_defaults;
        apply_page_defaults_to(&mut state.notebook.pages[0], defaults);
        state.begin_page_fade();
        drop(state);
        self.emit_view_changed();
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
        state.begin_page_fade();
        drop(state);
        self.emit_view_changed();
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

    pub fn set_document_title(&self, title: String) {
        let mut state = self.state.borrow_mut();
        if state.notebook.title == title {
            return;
        }
        state.notebook.title = title;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn rename_current_notebook(&self, title: &str) -> Result<PathBuf, DocumentError> {
        let title = crate::library::sanitize_stem(title);
        self.set_document_title(title.clone());
        let Some(path) = self.current_path() else {
            return Ok(PathBuf::new());
        };
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let dest = crate::library::unique_inkstone_path(parent, &title, Some(&path));
        if dest != path {
            self.save_current()?;
            fs::rename(&path, &dest)?;
            self.state.borrow_mut().path = Some(dest.clone());
        } else {
            let _ = self.save_current();
        }
        Ok(dest)
    }

    pub fn move_current_notebook(&self, category: &Path) -> Result<PathBuf, DocumentError> {
        fs::create_dir_all(category)?;
        let title = self.document_title();
        let Some(path) = self.current_path() else {
            let dest = crate::library::unique_inkstone_path(category, &title, None);
            self.save(&dest)?;
            return Ok(dest);
        };
        let dest = crate::library::unique_inkstone_path(category, &title, Some(&path));
        if dest != path {
            self.save_current()?;
            fs::rename(&path, &dest)?;
            self.state.borrow_mut().path = Some(dest.clone());
        }
        Ok(self.current_path().unwrap_or(dest))
    }

    pub fn export_svg(&self, path: &Path) -> Result<(), DocumentError> {
        let state = self.state.borrow();
        state.notebook.export_svg(path, state.active_page)
    }

    pub fn export_pdf(&self, path: &Path) -> Result<(), DocumentError> {
        let state = self.state.borrow();
        state.notebook.validate()?;
        let first = &state.notebook.pages[0].canvas;
        let (page_width, page_height) = first
            .paper_size()
            .dimensions_pt()
            .unwrap_or((mm_to_pt(210.0), mm_to_pt(297.0)));
        let page_width = f64::from(page_width);
        let page_height = f64::from(page_height);
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
                for element in page.visible_elements() {
                    draw_element(&context, element, &state.image_cache);
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
        self.state.borrow().page().elements().count()
    }

    pub fn is_dirty(&self) -> bool {
        self.state.borrow().dirty
    }

    pub fn undo(&self) {
        self.state.borrow_mut().undo();
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn redo(&self) {
        self.state.borrow_mut().redo();
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn page_object_counts(&self) -> Vec<usize> {
        self.state
            .borrow()
            .notebook
            .pages
            .iter()
            .map(|page| page.elements().count())
            .collect()
    }

    pub fn layer_summaries(&self) -> Vec<(String, bool, bool)> {
        self.state
            .borrow()
            .page()
            .layers
            .iter()
            .map(|layer| (layer.name.clone(), layer.visible, layer.locked))
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
        state.notebook.active_section = state.notebook.pages[index].section_id;
        state.selection.clear();
        state.search_position = 0;
        state.begin_page_fade();
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
    }

    pub fn add_page(&self) {
        let mut state = self.state.borrow_mut();
        let mut page = NotebookPage::named(format!("Page {}", state.notebook.pages.len() + 1));
        page.section_id = state.notebook.active_section;
        apply_page_defaults_to(&mut page, state.page_defaults);
        let id = page.id;
        if state.runtime.insert_after_current {
            let index = state.active_page + 1;
            state.notebook.pages.insert(index, page);
            state.active_page = index;
        } else {
            state.notebook.pages.push(page);
            state.active_page = state.notebook.pages.len() - 1;
        }
        state.active_layer = 0;
        state.push_history(HistoryEntry::PageAdded { id, stored: None });
        state.dirty = true;
        state.begin_page_fade();
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
    }

    pub fn add_subpage(&self) {
        let mut state = self.state.borrow_mut();
        let index = state.active_page;
        let section_id = state.page().section_id;
        let parent_title = state.page().title.clone();
        let mut page = NotebookPage::named(format!("{parent_title} subpage"));
        page.section_id = section_id;
        page.level = 1;
        apply_page_defaults_to(&mut page, state.page_defaults);
        let id = page.id;
        state.notebook.pages.insert(index + 1, page);
        state.active_page = index + 1;
        state.active_layer = 0;
        state.push_history(HistoryEntry::PageAdded { id, stored: None });
        state.dirty = true;
        state.begin_page_fade();
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
    }

    pub fn apply_template(&self, template: PageTemplate) {
        let mut state = self.state.borrow_mut();
        local::apply_template(state.page_mut(), template);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn sections(&self) -> Vec<(Uuid, String, Color)> {
        self.state
            .borrow()
            .notebook
            .sections
            .iter()
            .map(|section| (section.id, section.name.clone(), section.color))
            .collect()
    }

    pub fn active_section_id(&self) -> Uuid {
        self.state.borrow().notebook.active_section
    }

    pub fn set_active_section(&self, id: Uuid) {
        let mut state = self.state.borrow_mut();
        if !state
            .notebook
            .sections
            .iter()
            .any(|section| section.id == id)
        {
            return;
        }
        state.notebook.active_section = id;
        if let Some(index) = state
            .notebook
            .pages
            .iter()
            .position(|page| page.section_id == id)
            && index != state.active_page
        {
            state.active_page = index;
            state.active_layer = 0;
            state.selection.clear();
            state.begin_page_fade();
        }
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
    }

    pub fn add_section(&self, name: impl Into<String>) {
        let mut state = self.state.borrow_mut();
        let palette = [
            Color::BLUE,
            Color::rgb(0.05, 0.56, 0.32),
            Color::rgb(0.95, 0.62, 0.05),
            Color::rgb(0.84, 0.16, 0.20),
            Color::rgb(0.48, 0.20, 0.78),
            Color::rgb(0.05, 0.60, 0.66),
        ];
        let color = palette[state.notebook.sections.len() % palette.len()];
        let section = Section::named(name, color);
        let id = section.id;
        state.notebook.sections.push(section);
        state.notebook.active_section = id;
        let mut page = NotebookPage::named("Page 1");
        page.section_id = id;
        let page_id = page.id;
        state.notebook.pages.push(page);
        state.active_page = state.notebook.pages.len() - 1;
        state.active_layer = 0;
        state.push_history(HistoryEntry::PageAdded {
            id: page_id,
            stored: None,
        });
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
    }

    pub fn rename_active_section(&self, name: String) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let id = state.notebook.active_section;
        if let Some(section) = state
            .notebook
            .sections
            .iter_mut()
            .find(|section| section.id == id)
        {
            if section.name == name {
                return;
            }
            section.name = name.to_owned();
            state.dirty = true;
        }
        drop(state);
        self.schedule_autosave();
    }

    pub fn remove_active_section(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.notebook.sections.len() <= 1 {
            return false;
        }
        let id = state.notebook.active_section;
        let Some(index) = state
            .notebook
            .sections
            .iter()
            .position(|section| section.id == id)
        else {
            return false;
        };
        let mut kept = Vec::new();
        let mut trashed = Vec::new();
        for page in state.notebook.pages.drain(..) {
            if page.section_id == id {
                trashed.push(page);
            } else {
                kept.push(page);
            }
        }
        if kept.is_empty() {
            state.notebook.pages = trashed;
            return false;
        }
        state.notebook.pages = kept;
        state.notebook.trash.extend(trashed);
        state.notebook.sections.remove(index);
        state.notebook.active_section =
            state.notebook.sections[index.min(state.notebook.sections.len() - 1)].id;
        state.active_page = state
            .notebook
            .pages
            .iter()
            .position(|page| page.section_id == state.notebook.active_section)
            .unwrap_or(0);
        state.active_layer = 0;
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
        true
    }

    pub fn page_summaries(&self) -> Vec<(usize, String, usize, u32, Uuid)> {
        self.state
            .borrow()
            .notebook
            .pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                (
                    index,
                    page.title.clone(),
                    page.elements().count(),
                    page.level,
                    page.section_id,
                )
            })
            .collect()
    }

    pub fn trash_titles(&self) -> Vec<String> {
        self.state
            .borrow()
            .notebook
            .trash
            .iter()
            .map(|page| page.title.clone())
            .collect()
    }

    pub fn restore_trashed_page(&self, index: usize) -> bool {
        let mut state = self.state.borrow_mut();
        if index >= state.notebook.trash.len() {
            return false;
        }
        let mut page = state.notebook.trash.remove(index);
        if !state
            .notebook
            .sections
            .iter()
            .any(|section| section.id == page.section_id)
        {
            page.section_id = state.notebook.active_section;
        }
        let id = page.id;
        state.notebook.pages.push(page);
        state.active_page = state.notebook.pages.len() - 1;
        state.active_layer = 0;
        state.push_history(HistoryEntry::PageAdded { id, stored: None });
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
        true
    }

    pub fn empty_trash(&self) {
        let mut state = self.state.borrow_mut();
        if state.notebook.trash.is_empty() {
            return;
        }
        state.notebook.trash.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn insert_table(&self, columns: u32, rows: u32) {
        let origin = self.world_center();
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked {
            return;
        }
        state.add_element(Element::Table(TableElement::new(origin, columns, rows)));
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn insert_tag(&self, kind: TagKind) {
        let origin = self.world_center();
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked {
            return;
        }
        state.add_element(local::tag_element(origin, kind));
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn insert_date(&self) {
        let format = self.state.borrow().runtime.date_stamp.glib_format();
        let stamp = glib::DateTime::now_local()
            .ok()
            .and_then(|value| value.format(format).ok())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Date".to_owned());
        self.set_text(stamp.clone());
        let origin = self.world_center();
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked {
            return;
        }
        let mut note = TextNote::plain(origin, stamp, state.pending_font_size, state.style.color);
        note.bold = true;
        state.add_element(Element::Text(note));
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn calculate_selection_or_pending(&self) -> Option<String> {
        let mut state = self.state.borrow_mut();
        let selected = state.selection.clone();
        let mut updated = None;
        for element in state
            .page_mut()
            .layers
            .iter_mut()
            .flat_map(|layer| layer.elements.iter_mut())
        {
            if let Element::Text(text) = element
                && selected.contains(&text.id)
                && let Some(result) = local::evaluate_equation(&text.text)
            {
                text.text = result.clone();
                updated = Some(result);
            }
        }
        if updated.is_none()
            && let Some(result) = local::evaluate_equation(&state.pending_text)
        {
            state.pending_text = result.clone();
            updated = Some(result);
        }
        if updated.is_some() {
            state.dirty = true;
        }
        drop(state);
        if updated.is_some() {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        updated
    }

    pub fn open_selected_attachment(&self) -> Result<bool, DocumentError> {
        let state = self.state.borrow();
        let Some(id) = state.selection.iter().copied().next() else {
            return Ok(false);
        };
        let Some(Element::Media(media)) =
            state.page().elements().find(|element| element.id() == id)
        else {
            return Ok(false);
        };
        if matches!(media.kind, MediaKind::Image) {
            return Ok(false);
        }
        let Some(asset) = state.notebook.asset(media.asset_id) else {
            return Ok(false);
        };
        let bytes = asset.decoded()?;
        let mut path = std::env::temp_dir();
        path.push(format!("inkstone-{}-{}", asset.id, asset.name));
        fs::write(&path, bytes)?;
        drop(state);
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
        Ok(true)
    }

    pub fn remove_active_page(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.notebook.pages.len() == 1 {
            return false;
        }
        let index = state.active_page;
        let page = state.notebook.pages.remove(index);
        state.notebook.trash.push(page.clone());
        state.push_history(HistoryEntry::PageRemoved {
            index,
            stored: Some(page),
        });
        state.active_page = index.min(state.notebook.pages.len() - 1);
        state.active_layer = 0;
        state.selection.clear();
        state.dirty = true;
        state.begin_page_fade();
        drop(state);
        self.schedule_autosave();
        start_canvas_fx(&self.area, &self.state);
        self.reset_view();
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

    pub fn active_layer_locked(&self) -> bool {
        self.state.borrow().active_layer().locked
    }

    pub fn grid_visible(&self) -> bool {
        self.state.borrow().page().canvas.grid_visible
    }

    pub fn set_active_layer(&self, index: usize) {
        let mut state = self.state.borrow_mut();
        if index < state.page().layers.len() {
            state.active_layer = index;
            state.selection.clear();
        }
        drop(state);
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
        drop(state);
        self.schedule_autosave();
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
        drop(state);
        self.schedule_autosave();
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
        drop(state);
        start_canvas_fx(&self.area, &self.state);
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
            "png" => (MediaKind::Image, "image/png".to_owned()),
            "jpg" | "jpeg" => (MediaKind::Image, "image/jpeg".to_owned()),
            "webp" => (MediaKind::Image, "image/webp".to_owned()),
            "gif" => (MediaKind::Image, "image/gif".to_owned()),
            "pdf" => (MediaKind::Pdf, "application/pdf".to_owned()),
            "mp3" => (MediaKind::Audio, "audio/mpeg".to_owned()),
            "wav" => (MediaKind::Audio, "audio/wav".to_owned()),
            "ogg" | "oga" => (MediaKind::Audio, "audio/ogg".to_owned()),
            "flac" => (MediaKind::Audio, "audio/flac".to_owned()),
            "m4a" => (MediaKind::Audio, "audio/mp4".to_owned()),
            "" => {
                return Err(DocumentError::Invalid(
                    "the file needs an extension so Inkstone can attach it".to_owned(),
                ));
            }
            other => (MediaKind::File, format!("application/{other}")),
        };
        let bytes = fs::read(path)?;
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
            match kind {
                MediaKind::Audio => (280.0, 72.0),
                _ => (320.0, 88.0),
            }
        };
        state.notebook.assets.push(Asset {
            id: asset_id,
            name: name.clone(),
            media_type: media_type.clone(),
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
        let next = if state.page().canvas.pattern() == BackgroundPattern::Grid {
            BackgroundPattern::None
        } else {
            BackgroundPattern::Grid
        };
        state.page_mut().canvas.set_pattern(next);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn pattern(&self) -> BackgroundPattern {
        self.state.borrow().page().canvas.pattern()
    }

    pub fn set_pattern(&self, pattern: BackgroundPattern) {
        let mut state = self.state.borrow_mut();
        if state.page().canvas.pattern() == pattern {
            return;
        }
        state.page_mut().canvas.set_pattern(pattern);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn layout(&self) -> PageLayout {
        self.state.borrow().page().canvas.layout
    }

    pub fn set_layout(&self, layout: PageLayout) {
        let mut state = self.state.borrow_mut();
        if state.page().canvas.layout == layout {
            return;
        }
        state.page_mut().canvas.layout = layout;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn paper_size(&self) -> PaperSize {
        self.state.borrow().page().canvas.paper_size()
    }

    pub fn set_paper_size(&self, size: PaperSize) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.set_paper_size(size);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn grid_spacing_mm(&self) -> f32 {
        pt_to_mm(self.state.borrow().page().canvas.grid_spacing)
    }

    pub fn set_grid_spacing_mm(&self, mm: f32) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.grid_spacing = mm_to_pt(mm.clamp(1.0, 50.0));
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn background_color(&self) -> Color {
        self.state.borrow().page().canvas.background
    }

    pub fn set_background_color(&self, color: Color) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.background = color;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn page_count(&self) -> usize {
        self.state.borrow().notebook.pages.len()
    }

    pub fn copy_selection(&self) -> Option<String> {
        self.state.borrow().selection_json()
    }

    pub fn cut_selection(&self) -> (Option<String>, usize) {
        let json = self.copy_selection();
        let count = self.delete_selection();
        (json, count)
    }

    pub fn paste_json(&self, json: &str) -> Result<usize, DocumentError> {
        let count = self.state.borrow_mut().paste_json(json)?;
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        Ok(count)
    }

    pub fn bring_selection_to_front(&self) -> usize {
        let count = self.state.borrow_mut().reorder_selection(true);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn send_selection_to_back(&self) -> usize {
        let count = self.state.borrow_mut().reorder_selection(false);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn rotate_selection(&self, degrees: f32) -> usize {
        let count = self.state.borrow_mut().rotate_selection_by(degrees);
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        count
    }

    pub fn export_png(&self, path: &Path) -> Result<(), DocumentError> {
        self.export_raster(path, "png")
    }

    pub fn export_jpeg(&self, path: &Path) -> Result<(), DocumentError> {
        self.export_raster(path, "jpeg")
    }

    fn export_raster(&self, path: &Path, format: &str) -> Result<(), DocumentError> {
        let png = {
            let state = self.state.borrow();
            state.render_page_png(state.active_page)?
        };
        if format == "png" {
            fs::write(path, png)?;
            return Ok(());
        }
        let loader = PixbufLoader::new();
        loader
            .write(&png)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        loader
            .close()
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let pixbuf = loader
            .pixbuf()
            .ok_or_else(|| DocumentError::Export("could not encode the page image".to_owned()))?;
        pixbuf
            .savev(
                path,
                format,
                &[(
                    "quality",
                    &self.state.borrow().runtime.jpeg_quality.to_string(),
                )],
            )
            .map_err(|error| DocumentError::Export(error.to_string()))
    }

    pub fn import_svg(&self, path: &Path) -> Result<(), DocumentError> {
        let svg = fs::read_to_string(path)?;
        let elements = import_svg_elements(&svg)?;
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked {
            return Err(DocumentError::Invalid(
                "the active layer is locked".to_owned(),
            ));
        }
        for element in elements {
            state.add_element(element);
        }
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
        Ok(())
    }

    pub fn import_path(&self, path: &Path) -> Result<(), DocumentError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "svg" => self.import_svg(path),
            "pdf" => {
                let pages = self.import_pdf_as_pages(path)?;
                if pages == 0 {
                    self.import_media(path)
                } else {
                    Ok(())
                }
            }
            _ => self.import_media(path),
        }
    }

    pub fn render_page_to(
        &self,
        context: &Context,
        page_index: usize,
        width: f64,
        height: f64,
    ) -> Result<(), DocumentError> {
        self.state
            .borrow()
            .render_page(context, page_index, width, height)
    }

    fn schedule_autosave(&self) {
        schedule_autosave(&self.state);
    }

    fn world_center(&self) -> Point {
        self.state.borrow().screen_to_world(Point::new(
            self.area.width().max(1) as f32 / 2.0,
            self.area.height().max(1) as f32 / 2.0,
        ))
    }

    pub fn reset_view(&self) {
        let mut state = self.state.borrow_mut();
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        state.pan = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        state.zoom = state
            .runtime
            .startup_zoom
            .clamp(state.runtime.min_zoom, state.runtime.max_zoom);
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
        let animate = self.state.borrow().runtime.animate_zoom;
        if !animate
            || gtk::Settings::default().is_none_or(|settings| !settings.is_gtk_enable_animations())
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
                (state.zoom * factor).clamp(state.runtime.min_zoom, state.runtime.max_zoom),
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

    pub fn connect_text_loaded(&self, callback: impl Fn(String) + 'static) {
        self.state
            .borrow_mut()
            .text_listeners
            .push(Rc::new(callback));
    }

    pub fn pending_text(&self) -> String {
        self.state.borrow().pending_text.clone()
    }

    pub fn connect_busy(&self, callback: impl Fn(bool) + 'static) {
        self.state
            .borrow_mut()
            .busy_listeners
            .push(Rc::new(callback));
    }

    pub fn apply_night_paper(&self) {
        let mut state = self.state.borrow_mut();
        local::apply_night_paper(state.page_mut());
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn align_selection(&self, mode: AlignMode) -> usize {
        let count = self.state.borrow_mut().align_selection(mode);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn copy_selection_svg(&self) -> Option<String> {
        self.state.borrow().selection_svg()
    }

    pub fn copy_selection_png(&self) -> Result<Option<Vec<u8>>, DocumentError> {
        self.state.borrow().selection_png()
    }

    pub fn export_layers_folder(&self, dir: &Path) -> Result<usize, DocumentError> {
        let (notebook, page_index) = {
            let state = self.state.borrow();
            (state.notebook.clone(), state.active_page)
        };
        fs::create_dir_all(dir)?;
        let page = notebook
            .pages
            .get(page_index)
            .ok_or_else(|| DocumentError::Invalid("the current page is missing".to_owned()))?;
        for (index, layer) in page.layers.iter().enumerate() {
            let name = format!("{:02} {}.svg", index + 1, layer.name);
            notebook.export_layer_svg(&dir.join(name), page_index, index)?;
        }
        Ok(page.layers.len())
    }

    pub fn export_notebook_folder(&self, dir: &Path) -> Result<(), DocumentError> {
        let notebook = self.state.borrow().notebook.clone();
        notebook.export_folder(dir)?;
        for (index, page) in notebook.pages.iter().enumerate() {
            let stem = format!("{:02} {}", index + 1, page.title);
            let png = {
                let state = self.state.borrow();
                state.render_page_png(index).ok()
            };
            if let Some(png) = png {
                let name = sanitize_export_name(&format!("{stem}.png"));
                fs::write(dir.join(name), png)?;
            }
        }
        Ok(())
    }

    pub fn todos(&self) -> Vec<local::TodoItem> {
        local::collect_todos(&self.state.borrow().notebook)
    }

    pub fn copy_page_link(&self) -> String {
        let state = self.state.borrow();
        local::page_link_href(state.page().id)
    }

    pub fn insert_page_link(&self) {
        let link = {
            let state = self.state.borrow();
            format!("[[{}]]", state.page().title)
        };
        self.set_text(link);
    }

    pub fn goto_page_link(&self, href: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let index = if let Some(id) = local::parse_page_link_href(href) {
            state.notebook.page_by_id(id)
        } else {
            local::parse_page_links(href)
                .into_iter()
                .find_map(|name| local::resolve_page_name(&state.notebook, &name))
        };
        let Some(index) = index else {
            return false;
        };
        state.active_page = index;
        state.active_layer = 0;
        state.begin_page_fade();
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.area.queue_draw();
        true
    }

    pub fn notebook_color(&self) -> Option<Color> {
        self.state.borrow().notebook.color
    }

    pub fn set_notebook_color(&self, color: Color) {
        let mut state = self.state.borrow_mut();
        state.notebook.color = Some(color);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn cycle_notebook_color(&self) -> Color {
        let mut state = self.state.borrow_mut();
        let current = state.notebook.color.unwrap_or(Color::BLUE);
        let next = crate::library::NOTEBOOK_SWATCHES
            .iter()
            .position(|color| *color == current)
            .map(|index| crate::library::NOTEBOOK_SWATCHES[(index + 1) % 8])
            .unwrap_or(crate::library::NOTEBOOK_SWATCHES[0]);
        state.notebook.color = Some(next);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        next
    }

    pub fn import_pdf_as_pages(&self, path: &Path) -> Result<usize, DocumentError> {
        const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
        if fs::metadata(path)?.len() > MAX_ASSET_BYTES {
            return Err(DocumentError::Invalid(
                "embedded files are limited to 64 MiB".to_owned(),
            ));
        }
        let bytes = fs::read(path)?;
        let dpi = self.state.borrow().runtime.pdf_dpi;
        let rasters = pdf::rasterize_pdf_pages(&bytes, dpi).unwrap_or_default();
        let page_count = rasters.len().max(pdf::count_pdf_pages(&bytes)).max(1);
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("PDF")
            .to_owned();
        let mut state = self.state.borrow_mut();
        let section = state.notebook.active_section;
        let mut created = 0;
        for index in 0..page_count {
            let mut page = NotebookPage::named(format!("{name} {}", index + 1));
            page.section_id = section;
            page.canvas.set_paper_size(PaperSize::A4);
            let asset_id = Uuid::new_v4();
            if let Some(png) = rasters.get(index) {
                if let Ok(pixbuf) = decode_pixbuf(png) {
                    let width = page.canvas.page_width;
                    let height = width * pixbuf.height() as f32 / pixbuf.width().max(1) as f32;
                    page.canvas.page_height = height;
                    state.image_cache.insert(asset_id, pixbuf);
                    state.notebook.assets.push(Asset {
                        id: asset_id,
                        name: format!("{name}-{}.png", index + 1),
                        media_type: "image/png".to_owned(),
                        data_base64: base64::engine::general_purpose::STANDARD.encode(png),
                    });
                    page.layers[0].elements.push(Element::Media(MediaElement {
                        id: Uuid::new_v4(),
                        asset_id,
                        kind: MediaKind::Image,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height,
                        },
                        alt_text: format!("{name} page {}", index + 1),
                        caption: format!("{name} page {}", index + 1),
                    }));
                }
            } else {
                state.notebook.assets.push(Asset {
                    id: asset_id,
                    name: format!("{name}.pdf"),
                    media_type: "application/pdf".to_owned(),
                    data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                });
                let width = page.canvas.page_width;
                let height = page.canvas.page_height;
                page.layers[0].elements.push(Element::Media(MediaElement {
                    id: Uuid::new_v4(),
                    asset_id,
                    kind: MediaKind::Pdf,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                    alt_text: format!("{name} page {}", index + 1),
                    caption: format!("{name} page {}", index + 1),
                }));
            }
            let id = page.id;
            state.notebook.pages.push(page);
            state.push_history(HistoryEntry::PageAdded { id, stored: None });
            created += 1;
            if rasters.is_empty() {
                break;
            }
        }
        if created > 0 {
            state.active_page = state.notebook.pages.len() - created;
            state.active_layer = 0;
            state.dirty = true;
        }
        drop(state);
        if created > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.reset_view();
        }
        Ok(created)
    }

    pub fn start_audio_capture(&self) -> Result<(), DocumentError> {
        let mut state = self.state.borrow_mut();
        if state.audio_child.is_some() {
            return Ok(());
        }
        let Some((bin, args)) = pdf::record_command() else {
            return Err(DocumentError::Invalid(
                "no local recorder (pw-record, parecord, or arecord) is installed".to_owned(),
            ));
        };
        let path = std::env::temp_dir().join(format!("inkstone-{}.wav", Uuid::new_v4()));
        let mut command = std::process::Command::new(bin);
        command.args(args).arg(&path);
        let child = command
            .spawn()
            .map_err(|error| DocumentError::Invalid(format!("could not start {bin}: {error}")))?;
        state.audio_child = Some(child);
        state.audio_path = Some(path);
        Ok(())
    }

    pub fn stop_audio_capture(&self) -> Result<bool, DocumentError> {
        let (path, mut child) = {
            let mut state = self.state.borrow_mut();
            match (state.audio_path.take(), state.audio_child.take()) {
                (Some(path), Some(child)) => (path, child),
                _ => return Ok(false),
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        if path.exists() {
            self.import_media(&path)?;
            let _ = fs::remove_file(&path);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn is_recording_audio(&self) -> bool {
        self.state.borrow().audio_child.is_some()
    }

    pub fn render_page_thumbnail(
        &self,
        page_index: usize,
        width: i32,
        height: i32,
    ) -> Option<Pixbuf> {
        let png = self.state.borrow().render_page_png(page_index).ok()?;
        let loader = PixbufLoader::new();
        loader.write(&png).ok()?;
        loader.close().ok()?;
        let pixbuf = loader.pixbuf()?;
        Some(
            pixbuf
                .scale_simple(
                    width.max(1),
                    height.max(1),
                    gdk_pixbuf::InterpType::Bilinear,
                )
                .unwrap_or(pixbuf),
        )
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
        if self.runtime.selection_flash {
            self.selection_flash = Some(Instant::now());
        }
    }

    fn begin_page_fade(&mut self) {
        self.replay = None;
        if self.runtime.page_fade {
            self.page_fade = Some(Instant::now());
        }
        if self.runtime.show_empty_hint && self.page().visible_elements().next().is_none() {
            self.empty_hint = Some(Instant::now());
        }
    }

    fn replay_duration(&self) -> f32 {
        local::replay_duration(&local::replay_timeline(self.page().visible_elements()))
    }

    fn replay_status(&self) -> ReplayStatus {
        let Some(replay) = self.replay.as_ref() else {
            return ReplayStatus {
                speed: self.replay_speed,
                ..ReplayStatus::idle()
            };
        };
        let duration = self.replay_duration();
        let elapsed = replay.seconds().clamp(0.0, duration.max(0.0));
        let finished = duration > 0.0 && elapsed >= duration - 0.0005;
        ReplayStatus {
            active: true,
            playing: replay.playing && !finished,
            finished,
            progress: if duration > 0.0 {
                (elapsed / duration).clamp(0.0, 1.0)
            } else {
                1.0
            },
            speed: replay.speed,
            elapsed_secs: elapsed,
            duration_secs: duration,
        }
    }

    fn start_replay(&mut self) -> bool {
        let duration = self.replay_duration();
        if duration <= 0.0 {
            return false;
        }
        self.selection.clear();
        self.replay_generation = self.replay_generation.wrapping_add(1);
        self.replay = Some(ReplayPlayback {
            elapsed_secs: 0.0,
            last_tick: Instant::now(),
            playing: true,
            speed: self.replay_speed,
            generation: self.replay_generation,
        });
        true
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

    fn restore_slots(&mut self, slots: &[ElementSlot]) {
        let page_index = self.active_page;
        for slot in slots {
            let Some(layer_index) = self.notebook.pages[page_index]
                .layers
                .iter()
                .position(|layer| layer.id == slot.layer_id)
            else {
                continue;
            };
            let Some(stored) = &slot.stored else {
                continue;
            };
            if let Some(element) = self.notebook.pages[page_index].layers[layer_index]
                .elements
                .iter_mut()
                .find(|element| element.id() == slot.id)
            {
                *element = stored.clone();
            }
        }
    }

    fn scale_selection(&mut self, origin: Point, scale_x: f32, scale_y: f32) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.scale_from(origin, scale_x, scale_y);
                }
            }
        }
    }

    fn rotate_selection(&mut self, center: Point, degrees: f32) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.rotate_around(center, degrees);
                }
            }
        }
    }

    fn rotate_selection_by(&mut self, degrees: f32) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let before = self.capture_elements(&selected);
        let Some(bounds) = self.selection_bounds() else {
            return 0;
        };
        self.rotate_selection(bounds.center(), degrees);
        let page_id = self.page().id;
        let count = selected.len();
        self.push_history(HistoryEntry::ElementsChanged {
            page_id,
            slots: before,
        });
        self.dirty = true;
        self.flash_selection();
        count
    }

    fn shift_below(&mut self, threshold: f32, delta_y: f32) {
        for layer in &mut self.page_mut().layers {
            if layer.locked {
                continue;
            }
            for element in &mut layer.elements {
                if element.bounds().y >= threshold - 0.5 {
                    element.translate(Point::new(0.0, delta_y));
                }
            }
        }
    }

    fn selection_bounds(&self) -> Option<Rect> {
        self.page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .map(Element::bounds)
            .reduce(Rect::union)
    }

    fn hit_transform_handle(&self, world: Point) -> Option<TransformHandle> {
        if self.selection.is_empty() {
            return None;
        }
        let bounds = self.selection_bounds()?.expand(6.0 / self.zoom);
        let tolerance = 10.0 / self.zoom;
        let corners = [
            (
                Point::new(bounds.x, bounds.y),
                Point::new(bounds.x + bounds.width, bounds.y + bounds.height),
            ),
            (
                Point::new(bounds.x + bounds.width, bounds.y),
                Point::new(bounds.x, bounds.y + bounds.height),
            ),
            (
                Point::new(bounds.x, bounds.y + bounds.height),
                Point::new(bounds.x + bounds.width, bounds.y),
            ),
            (
                Point::new(bounds.x + bounds.width, bounds.y + bounds.height),
                Point::new(bounds.x, bounds.y),
            ),
        ];
        for (handle, origin) in corners {
            if handle.distance_to(world) <= tolerance {
                return Some(TransformHandle::Resize { origin });
            }
        }
        let rotate = Point::new(bounds.center().x, bounds.y - 22.0 / self.zoom);
        if rotate.distance_to(world) <= tolerance {
            return Some(TransformHandle::Rotate {
                center: bounds.center(),
            });
        }
        None
    }

    fn selection_json(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let elements: Vec<Element> = self
            .page()
            .elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if elements.is_empty() {
            return None;
        }
        let mut asset_ids = HashSet::new();
        for element in &elements {
            if let Element::Media(media) = element {
                asset_ids.insert(media.asset_id);
            }
        }
        let assets = self
            .notebook
            .assets
            .iter()
            .filter(|asset| asset_ids.contains(&asset.id))
            .cloned()
            .collect();
        serde_json::to_string(&ClipboardPayload {
            format: CLIPBOARD_FORMAT.to_owned(),
            version: 1,
            elements,
            assets,
        })
        .ok()
    }

    fn paste_json(&mut self, json: &str) -> Result<usize, DocumentError> {
        if self.active_layer().locked {
            return Err(DocumentError::Invalid(
                "the active layer is locked".to_owned(),
            ));
        }
        let payload: ClipboardPayload = serde_json::from_str(json).map_err(|_| {
            DocumentError::Invalid("clipboard does not contain Inkstone objects".to_owned())
        })?;
        if payload.format != CLIPBOARD_FORMAT || payload.elements.is_empty() {
            return Err(DocumentError::Invalid(
                "clipboard does not contain Inkstone objects".to_owned(),
            ));
        }
        let mut asset_map = HashMap::new();
        for asset in payload.assets {
            let new_id = Uuid::new_v4();
            asset_map.insert(asset.id, new_id);
            self.notebook.assets.push(Asset {
                id: new_id,
                ..asset
            });
            if let Ok(bytes) = self.notebook.assets.last().unwrap().decoded()
                && let Ok(pixbuf) = decode_pixbuf(&bytes)
            {
                self.image_cache.insert(new_id, pixbuf);
            }
        }
        let id_map: HashMap<Uuid, Uuid> = payload
            .elements
            .iter()
            .map(|element| (element.id(), Uuid::new_v4()))
            .collect();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let start_index = self.active_layer().elements.len();
        let mut slots = Vec::new();
        self.selection.clear();
        for (offset, mut element) in payload.elements.into_iter().enumerate() {
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
            if let Element::Media(media) = &mut element
                && let Some(new_asset) = asset_map.get(&media.asset_id)
            {
                media.asset_id = *new_asset;
            }
            self.selection.insert(new_id);
            self.active_layer_mut().elements.push(element);
            slots.push(ElementSlot {
                layer_id,
                index: start_index + offset,
                id: new_id,
                stored: None,
            });
        }
        let count = slots.len();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        self.flash_selection();
        Ok(count)
    }

    fn reorder_selection(&mut self, to_front: bool) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let stored = self.active_layer().elements.clone();
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for element in self.active_layer_mut().elements.drain(..) {
            if selected.contains(&element.id()) {
                moved.push(element);
            } else {
                kept.push(element);
            }
        }
        let count = moved.len();
        if count == 0 {
            self.active_layer_mut().elements = kept;
            return 0;
        }
        self.active_layer_mut().elements = if to_front {
            kept.extend(moved);
            kept
        } else {
            moved.extend(kept);
            moved
        };
        self.push_history(HistoryEntry::LayerReordered {
            page_id,
            layer_id,
            stored,
        });
        self.dirty = true;
        count
    }

    fn render_page(
        &self,
        context: &Context,
        page_index: usize,
        width: f64,
        height: f64,
    ) -> Result<(), DocumentError> {
        let page = self.notebook.pages.get(page_index).ok_or_else(|| {
            DocumentError::Invalid(format!("page index {page_index} is out of range"))
        })?;
        context.set_source_rgb(1.0, 1.0, 1.0);
        let _ = context.paint();
        if let Some(bounds) = page.content_bounds() {
            let bounds = bounds.expand(24.0);
            let scale = ((width - 36.0) / bounds.width as f64)
                .min((height - 36.0) / bounds.height as f64)
                .min(1.0);
            context.save().ok();
            context.translate(
                18.0 - bounds.x as f64 * scale,
                18.0 - bounds.y as f64 * scale,
            );
            context.scale(scale, scale);
            for element in page.visible_elements() {
                draw_element(context, element, &self.image_cache);
            }
            context.restore().ok();
        }
        Ok(())
    }

    fn render_page_png(&self, page_index: usize) -> Result<Vec<u8>, DocumentError> {
        let page = self.notebook.pages.get(page_index).ok_or_else(|| {
            DocumentError::Invalid(format!("page index {page_index} is out of range"))
        })?;
        let bounds = page
            .content_bounds()
            .unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                width: 640.0,
                height: 360.0,
            })
            .expand(24.0);
        let width = bounds.width.ceil().max(1.0) as i32;
        let height = bounds.height.ceil().max(1.0) as i32;
        let surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let context =
            Context::new(&surface).map_err(|error| DocumentError::Export(error.to_string()))?;
        set_source(&context, page.canvas.background);
        let _ = context.paint();
        context.translate(-bounds.x as f64, -bounds.y as f64);
        for element in page.visible_elements() {
            draw_element(&context, element, &self.image_cache);
        }
        surface.flush();
        let mut png = Vec::new();
        surface
            .write_to_png(&mut Cursor::new(&mut png))
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        Ok(png)
    }

    fn screen_to_world(&self, screen: Point) -> Point {
        Point::new(
            (screen.x - self.pan.x) / self.zoom,
            (screen.y - self.pan.y) / self.zoom,
        )
    }

    fn set_zoom_around(&mut self, requested_zoom: f32, screen: Point) {
        let world = self.screen_to_world(screen);
        self.zoom = requested_zoom.clamp(self.runtime.min_zoom, self.runtime.max_zoom);
        self.pan = Point::new(
            screen.x - world.x * self.zoom,
            screen.y - world.y * self.zoom,
        );
    }

    fn begin_input(
        &mut self,
        screen: Point,
        pressure: f32,
        eraser_tip: bool,
        button: u32,
        shift: bool,
    ) {
        if shift {
            self.constrain = true;
        }
        if button == 2 && !eraser_tip || self.tool == Tool::Pan {
            self.interaction = Some(Interaction::Pan {
                last_screen: screen,
            });
            return;
        }
        self.replay = None;

        let world = self.screen_to_world(screen);
        let effective_tool = if eraser_tip { Tool::Eraser } else { self.tool };
        match effective_tool {
            Tool::Select => {
                if let Some(handle) = self.hit_transform_handle(world) {
                    let before = self.capture_elements(&self.selection);
                    self.interaction = match handle {
                        TransformHandle::Resize { origin } => Some(Interaction::Resize {
                            origin,
                            start: world,
                            before,
                        }),
                        TransformHandle::Rotate { center } => Some(Interaction::Rotate {
                            center,
                            start_angle: (world.y - center.y).atan2(world.x - center.x),
                            before,
                        }),
                    };
                } else if let Some(id) = self.hit_test(world) {
                    if self.toggle_checkable(id, world) {
                        return;
                    }
                    if self.follow_text_link(id) {
                        return;
                    }
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
            Tool::Pen | Tool::Brush | Tool::Highlighter => {
                if self.active_layer().locked {
                    return;
                }
                let mut style = self.style.clone();
                let kind = match effective_tool {
                    Tool::Highlighter => {
                        style.color.alpha = self.runtime.highlighter_alpha;
                        style.width =
                            (style.width * self.runtime.highlighter_width_scale).max(mm_to_pt(2.0));
                        StrokeKind::Highlighter
                    }
                    Tool::Brush => {
                        style.width =
                            (style.width * self.runtime.brush_width_scale).max(style.width);
                        StrokeKind::Brush
                    }
                    _ => StrokeKind::Pen,
                };
                self.interaction = Some(Interaction::Stroke(Stroke {
                    id: Uuid::new_v4(),
                    kind,
                    style,
                    points: vec![StrokePoint::new(world, pressure.max(0.05))],
                }));
            }
            Tool::Eraser => {
                let layer = self.active_layer();
                let before = layer.elements.clone();
                let page_id = self.page().id;
                let layer_id = layer.id;
                self.interaction = Some(Interaction::Erase {
                    page_id,
                    layer_id,
                    before,
                    changed: false,
                });
                self.erase_at(world, true);
            }
            Tool::Text => {
                if self.active_layer().locked {
                    return;
                }
                if let Some(id) = self.hit_test(world) {
                    if self.load_table_cell(id, world) {
                        self.interaction = None;
                        return;
                    }
                    if self.load_text_element(id) {
                        self.interaction = None;
                        return;
                    }
                }
                let mut text = self.pending_text.trim().to_owned();
                if let Some(calculated) = local::evaluate_equation(&text) {
                    text = calculated;
                }
                if !text.is_empty() {
                    let mut note = TextNote::plain(
                        world,
                        text.clone(),
                        self.pending_font_size,
                        self.style.color,
                    );
                    note.max_width = Some(420.0);
                    note.bold = self.pending_bold;
                    note.italic = self.pending_italic;
                    note.underline = self.pending_underline;
                    note.list = self.pending_list;
                    note.href = self.pending_href.clone().or_else(|| {
                        local::parse_page_links(&text).into_iter().find_map(|name| {
                            local::resolve_page_name(&self.notebook, &name)
                                .map(|index| local::page_link_href(self.notebook.pages[index].id))
                        })
                    });
                    self.add_element(Element::Text(note));
                }
                self.editing_table = None;
                self.interaction = None;
            }
            Tool::Shape | Tool::Measure => {
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
            Tool::Space => {
                if self.active_layer().locked {
                    return;
                }
                let ids: HashSet<Uuid> = self.page().elements().map(Element::id).collect();
                let before = self.capture_elements(&ids);
                self.interaction = Some(Interaction::Space {
                    start_y: world.y,
                    last_y: world.y,
                    before,
                });
            }
        }
    }

    fn update_input(&mut self, screen: Point, pressure: f32, shift: bool) {
        if shift {
            self.constrain = true;
        }
        let mut world = self.screen_to_world(screen);
        if self.ruler
            && let Some(Interaction::Stroke(stroke)) = &self.interaction
            && let Some(first) = stroke.points.first()
        {
            world = local::snap_to_ruler(first.point(), world);
        } else if self.constrain
            && self.runtime.iso_angle_snap
            && let Some(Interaction::Stroke(stroke)) = &self.interaction
            && let Some(first) = stroke.points.first()
        {
            world = local::snap_to_iso_angle(first.point(), world);
        }
        let zoom = self.zoom;
        if let Some((origin, start, before)) = match &self.interaction {
            Some(Interaction::Resize {
                origin,
                start,
                before,
            }) => Some((*origin, *start, before.clone())),
            _ => None,
        } {
            let mut sx = if (start.x - origin.x).abs() < 0.5 {
                1.0
            } else {
                (world.x - origin.x) / (start.x - origin.x)
            };
            let mut sy = if (start.y - origin.y).abs() < 0.5 {
                1.0
            } else {
                (world.y - origin.y) / (start.y - origin.y)
            };
            if sx.abs() < 0.05 {
                sx = 0.05_f32.copysign(sx);
            }
            if sy.abs() < 0.05 {
                sy = 0.05_f32.copysign(sy);
            }
            self.restore_slots(&before);
            self.scale_selection(origin, sx, sy);
            return;
        }
        if let Some((center, start_angle, before)) = match &self.interaction {
            Some(Interaction::Rotate {
                center,
                start_angle,
                before,
            }) => Some((*center, *start_angle, before.clone())),
            _ => None,
        } {
            let angle = (world.y - center.y).atan2(world.x - center.x);
            let degrees = (angle - start_angle).to_degrees();
            self.restore_slots(&before);
            self.rotate_selection(center, degrees);
            return;
        }
        if let Some((start_y, last_y)) = match &self.interaction {
            Some(Interaction::Space {
                start_y, last_y, ..
            }) => Some((*start_y, *last_y)),
            _ => None,
        } {
            let dy = world.y - last_y;
            if let Some(Interaction::Space { last_y, .. }) = self.interaction.as_mut() {
                *last_y = world.y;
            }
            if dy.abs() > f32::EPSILON {
                self.shift_below(start_y, dy);
            }
            return;
        }
        let mut selection_delta = None;
        match self.interaction.as_mut() {
            Some(Interaction::Stroke(stroke)) => {
                let mut target = world;
                if self.stabilizer
                    && let Some(last) = stroke.points.last()
                {
                    target = local::stabilize_point(
                        last.point(),
                        world,
                        self.runtime.stabilizer_strength,
                    );
                }
                let should_add = stroke
                    .points
                    .last()
                    .is_none_or(|last| last.point().distance_to(target) >= 0.7 / zoom);
                if should_add {
                    stroke
                        .points
                        .push(StrokePoint::new(target, pressure.max(0.05)));
                }
            }
            Some(Interaction::Shape { start, current }) => {
                let start = *start;
                let constrain = self.constrain;
                let tool = self.tool;
                let kind = self.shape_kind;
                *current = constrain_shape_point(
                    constrain,
                    self.runtime.iso_angle_snap,
                    tool,
                    kind,
                    start,
                    world,
                );
            }
            Some(Interaction::Connector { current, .. })
            | Some(Interaction::Lasso { current, .. }) => {
                *current = world;
            }
            Some(Interaction::MoveSelection { last_world, .. }) => {
                let mut delta = Point::new(world.x - last_world.x, world.y - last_world.y);
                if self.constrain {
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                selection_delta = Some(delta);
                *last_world = Point::new(last_world.x + delta.x, last_world.y + delta.y);
            }
            Some(Interaction::Pan { last_screen }) => {
                self.pan.x += screen.x - last_screen.x;
                self.pan.y += screen.y - last_screen.y;
                *last_screen = screen;
            }
            Some(Interaction::Erase { .. }) => self.erase_at(world, false),
            Some(Interaction::Resize { .. })
            | Some(Interaction::Rotate { .. })
            | Some(Interaction::Space { .. })
            | None => {}
        }
        if let Some(delta) = selection_delta {
            self.translate_selection(delta);
        }
    }

    fn constrained_shape_point(&self, start: Point, current: Point) -> Point {
        constrain_shape_point(
            self.constrain,
            self.runtime.iso_angle_snap,
            self.tool,
            self.shape_kind,
            start,
            current,
        )
    }

    fn load_text_element(&mut self, id: Uuid) -> bool {
        let Some(Element::Text(note)) = self
            .page()
            .elements()
            .find(|element| element.id() == id)
            .cloned()
        else {
            return false;
        };
        self.selection.clear();
        self.selection.insert(id);
        self.pending_text = note.text.clone();
        self.pending_font_size = note.font_size;
        self.pending_bold = note.bold;
        self.pending_italic = note.italic;
        self.pending_underline = note.underline;
        self.pending_list = note.list;
        self.pending_href = note.href.clone();
        self.flash_selection();
        self.text_loaded = true;
        true
    }

    fn load_table_cell(&mut self, id: Uuid, world: Point) -> bool {
        let Some(Element::Table(table)) = self
            .page()
            .elements()
            .find(|element| element.id() == id)
            .cloned()
        else {
            return false;
        };
        let Some(index) = table.cell_index(world) else {
            return false;
        };
        self.selection.clear();
        self.selection.insert(id);
        self.editing_table = Some((id, index));
        self.pending_text = table.cells.get(index).cloned().unwrap_or_default();
        self.flash_selection();
        self.text_loaded = true;
        true
    }

    fn follow_text_link(&mut self, id: Uuid) -> bool {
        let Some(Element::Text(note)) = self.page().elements().find(|element| element.id() == id)
        else {
            return false;
        };
        let href = note.href.clone().or_else(|| {
            local::parse_page_links(&note.text)
                .into_iter()
                .next()
                .map(|name| format!("[[{name}]]"))
        });
        let Some(href) = href else {
            return false;
        };
        let index = if let Some(id) = local::parse_page_link_href(&href) {
            self.notebook.page_by_id(id)
        } else {
            local::parse_page_links(&href)
                .into_iter()
                .find_map(|name| local::resolve_page_name(&self.notebook, &name))
        };
        let Some(index) = index else {
            return false;
        };
        self.active_page = index;
        self.active_layer = 0;
        self.selection.clear();
        self.begin_page_fade();
        true
    }

    fn align_selection(&mut self, mode: AlignMode) -> usize {
        let ids: Vec<Uuid> = self.selection.iter().copied().collect();
        if ids.len() < 2 && !matches!(mode, AlignMode::SameWidth | AlignMode::SameHeight) {
            return 0;
        }
        if ids.is_empty() {
            return 0;
        }
        let bounds: Vec<Rect> = ids
            .iter()
            .filter_map(|id| {
                self.page()
                    .elements()
                    .find(|element| element.id() == *id)
                    .map(Element::bounds)
            })
            .collect();
        if bounds.len() != ids.len() {
            return 0;
        }
        let deltas = local::align_bounds(&bounds, mode);
        let before = self.capture_elements(&self.selection);
        match mode {
            AlignMode::SameWidth | AlignMode::SameHeight => {
                for (id, scale) in ids.iter().zip(deltas.iter()) {
                    if let Some(element) = self.page_mut().element_mut(*id) {
                        let origin = element.bounds().normalized();
                        let origin_pt = Point::new(origin.x, origin.y);
                        element.scale_from(origin_pt, scale.x, scale.y);
                    }
                }
            }
            _ => {
                for (id, delta) in ids.iter().zip(deltas.iter()) {
                    if let Some(element) = self.page_mut().element_mut(*id) {
                        element.translate(*delta);
                    }
                }
            }
        }
        let page_id = self.page().id;
        self.push_history(HistoryEntry::ElementsChanged {
            page_id,
            slots: before,
        });
        self.dirty = true;
        ids.len()
    }

    fn selection_svg(&self) -> Option<String> {
        let selected: Vec<Element> = self
            .page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if selected.is_empty() {
            return None;
        }
        let bounds = selected
            .iter()
            .map(Element::bounds)
            .reduce(Rect::union)?
            .expand(16.0);
        let mut svg = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">\n",
            bounds.x, bounds.y, bounds.width, bounds.height
        );
        for element in selected {
            svg.push_str(&crate::document::element_svg(&element));
        }
        svg.push_str("</svg>\n");
        Some(svg)
    }

    fn selection_png(&self) -> Result<Option<Vec<u8>>, DocumentError> {
        let selected: Vec<Element> = self
            .page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if selected.is_empty() {
            return Ok(None);
        }
        let bounds = selected
            .iter()
            .map(Element::bounds)
            .reduce(Rect::union)
            .unwrap()
            .expand(16.0);
        let width = bounds.width.ceil().max(8.0) as i32;
        let height = bounds.height.ceil().max(8.0) as i32;
        let surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let context =
            Context::new(&surface).map_err(|error| DocumentError::Export(error.to_string()))?;
        set_source(&context, self.page().canvas.background);
        let _ = context.paint();
        context.translate(-bounds.x as f64, -bounds.y as f64);
        for element in &selected {
            draw_element(&context, element, &self.image_cache);
        }
        surface.flush();
        let mut png = Vec::new();
        surface
            .write_to_png(&mut Cursor::new(&mut png))
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        Ok(Some(png))
    }

    fn cancel_input(&mut self) {
        self.interaction = None;
        self.replay = None;
    }

    fn end_input(&mut self, screen: Point, pressure: f32, shift: bool) {
        self.update_input(screen, pressure, shift);
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
                if self.ink_to_shape
                    && let Some(shape) = local::stroke_to_shape(&stroke)
                {
                    self.add_element(Element::Shape(shape));
                } else {
                    self.add_element(Element::Stroke(stroke));
                }
            }
            Interaction::Shape { start, mut current } => {
                current = self.constrained_shape_point(start, current);
                if start.distance_to(current) < 4.0 / self.zoom {
                    current = Point::new(start.x + mm_to_pt(40.0), start.y + mm_to_pt(24.0));
                }
                let kind = if self.tool == Tool::Measure {
                    ShapeKind::Dimension
                } else {
                    self.shape_kind
                };
                let bounds = if kind.uses_drag_bounds() {
                    Rect::from_drag(start, current)
                } else {
                    Rect::from_points(start, current)
                };
                let label = if kind == ShapeKind::Dimension {
                    local::dimension_label(start, current)
                } else {
                    self.pending_label.trim().to_owned()
                };
                self.add_element(Element::Shape(Shape {
                    id: Uuid::new_v4(),
                    kind,
                    bounds,
                    rotation_degrees: 0.0,
                    style: self.style.clone(),
                    fill: self.fill_enabled.then_some(Color {
                        alpha: 0.22,
                        ..self.style.color
                    }),
                    label,
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
            Interaction::MoveSelection { before, .. }
            | Interaction::Resize { before, .. }
            | Interaction::Rotate { before, .. }
            | Interaction::Space { before, .. } => {
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
            Interaction::Pan { .. } => {}
            Interaction::Erase {
                page_id,
                layer_id,
                before,
                changed,
            } => {
                if changed {
                    self.push_history(HistoryEntry::LayerReordered {
                        page_id,
                        layer_id,
                        stored: before,
                    });
                    self.dirty = true;
                }
            }
        }
    }

    fn add_element(&mut self, element: Element) {
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

    fn toggle_checkable(&mut self, id: Uuid, world: Point) -> bool {
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if element.id() != id {
                    continue;
                }
                match element {
                    Element::Tag(tag)
                        if tag.kind == TagKind::ToDo && world.x <= tag.origin.x + 28.0 =>
                    {
                        tag.checked = !tag.checked;
                        self.dirty = true;
                        return true;
                    }
                    Element::Text(text)
                        if text.list == ListStyle::Checklist
                            && world.x <= text.origin.x + text.font_size =>
                    {
                        text.checked = !text.checked;
                        self.dirty = true;
                        return true;
                    }
                    _ => return false,
                }
            }
        }
        false
    }

    fn erase_at(&mut self, point: Point, record: bool) {
        let radius = (self.style.width * self.runtime.eraser_scale).max(10.0 / self.zoom);
        let mut changed = false;
        for layer_index in (0..self.page().layers.len()).rev() {
            if !self.page().layers[layer_index].visible || self.page().layers[layer_index].locked {
                continue;
            }
            let count = self.page().layers[layer_index].elements.len();
            for element_index in (0..count).rev() {
                let Element::Stroke(stroke) =
                    &self.page().layers[layer_index].elements[element_index]
                else {
                    continue;
                };
                let Some(pieces) = local::split_stroke(stroke, point, radius) else {
                    continue;
                };
                self.page_mut().layers[layer_index]
                    .elements
                    .remove(element_index);
                for piece in pieces.into_iter().rev() {
                    self.page_mut().layers[layer_index]
                        .elements
                        .insert(element_index, Element::Stroke(piece));
                }
                changed = true;
                break;
            }
            if changed {
                break;
            }
        }
        if !changed {
            if let Some(id) = self.hit_test(point)
                && !self
                    .page()
                    .elements()
                    .any(|element| element.id() == id && matches!(element, Element::Stroke(_)))
            {
                self.selection.clear();
                self.selection.insert(id);
                self.delete_selection();
                self.selection.clear();
            }
            return;
        }
        if let Some(Interaction::Erase { changed, .. }) = &mut self.interaction {
            *changed = true;
        }
        self.dirty = true;
        let _ = record;
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
                    self.notebook.trash.retain(|item| item.id != page.id);
                    self.notebook
                        .pages
                        .insert((*index).min(self.notebook.pages.len()), page);
                    self.active_page = (*index).min(self.notebook.pages.len() - 1);
                } else if *index < self.notebook.pages.len() && self.notebook.pages.len() > 1 {
                    let page = self.notebook.pages.remove(*index);
                    self.notebook.trash.push(page.clone());
                    *stored = Some(page);
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
            HistoryEntry::LayerReordered {
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
                std::mem::swap(&mut layer.elements, stored);
            }
        }
        self.selection.clear();
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

fn emit_replay(state: &Rc<RefCell<CanvasState>>) {
    let (status, listeners) = {
        let state = state.borrow();
        (state.replay_status(), state.replay_listeners.clone())
    };
    for listener in listeners {
        listener(status);
    }
}

fn emit_ink_prefs(state: &Rc<RefCell<CanvasState>>) {
    let listeners = state.borrow().ink_pref_listeners.clone();
    for listener in listeners {
        listener();
    }
}

fn apply_page_defaults_to(page: &mut NotebookPage, defaults: PageDefaults) {
    page.canvas.set_paper_size(defaults.paper);
    page.canvas.set_pattern(defaults.pattern);
    page.canvas.grid_spacing = mm_to_pt(defaults.grid_mm.clamp(1.0, 50.0));
    page.canvas.layout = defaults.layout;
    if defaults.night {
        local::apply_night_paper(page);
    }
    if defaults.template != PageTemplate::Blank {
        local::apply_template(page, defaults.template);
    }
}

fn start_replay_ticks(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let generation = {
        let state = state.borrow();
        state.replay.as_ref().map(|replay| replay.generation)
    };
    let Some(generation) = generation else {
        return;
    };
    let state = state.clone();
    area.add_tick_callback(move |area, _clock| {
        let keep = {
            let mut state = state.borrow_mut();
            let current = state.replay.as_ref().map(|replay| replay.generation);
            match current {
                None => false,
                Some(current) if current != generation => false,
                Some(_) => {
                    let duration = state.replay_duration();
                    if let Some(replay) = state.replay.as_mut() {
                        replay.commit();
                        if duration <= 0.0 || replay.elapsed_secs >= duration {
                            replay.elapsed_secs = duration.max(0.0);
                            replay.playing = false;
                        }
                        replay.playing
                    } else {
                        false
                    }
                }
            }
        };
        emit_replay(&state);
        area.queue_draw();
        if keep {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn emit_text_loaded(state: &Rc<RefCell<CanvasState>>) {
    let (text, listeners) = {
        let mut state = state.borrow_mut();
        if !state.text_loaded {
            return;
        }
        state.text_loaded = false;
        (state.pending_text.clone(), state.text_listeners.clone())
    };
    for listener in listeners {
        listener(text.clone());
    }
}

fn shift_held(controller: &impl gtk::prelude::EventControllerExt) -> bool {
    controller
        .current_event_state()
        .contains(gdk::ModifierType::SHIFT_MASK)
}

fn is_touch_event(controller: &impl gtk::prelude::EventControllerExt) -> bool {
    controller
        .current_event()
        .and_then(|event| event.device())
        .is_some_and(|device| device.source() == gdk::InputSource::Touchscreen)
}

fn constrain_shape_point(
    constrain: bool,
    iso_angle: bool,
    tool: Tool,
    shape_kind: ShapeKind,
    start: Point,
    current: Point,
) -> Point {
    if !constrain {
        return current;
    }
    let kind = if tool == Tool::Measure {
        ShapeKind::Dimension
    } else {
        shape_kind
    };
    if kind.uses_drag_bounds() {
        if iso_angle {
            local::snap_to_iso_angle(start, current)
        } else {
            current
        }
    } else {
        local::constrain_to_square(start, current)
    }
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

fn start_canvas_fx(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    emit_replay(state);
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
    let (generation, path, delay) = {
        let mut state = state.borrow_mut();
        if !state.dirty || state.path.is_none() || state.runtime.autosave_ms == 0 {
            return;
        }
        state.autosave_generation = state.autosave_generation.wrapping_add(1);
        (
            state.autosave_generation,
            state.path.clone(),
            state.runtime.autosave_ms,
        )
    };
    glib::timeout_add_local_once(Duration::from_millis(delay), {
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
                    || (state.ignore_touch
                        && is_touch_event(gesture)
                        && state.last_stylus.is_some_and(|at| {
                            at.elapsed() < Duration::from_millis(state.runtime.palm_reject_ms)
                        }))
                {
                    return;
                }
                state.begin_input(
                    Point::new(x as f32, y as f32),
                    1.0,
                    false,
                    gesture.current_button(),
                    shift_held(gesture),
                );
            }
            emit_busy(&state);
            emit_text_loaded(&state);
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
                shift_held(gesture),
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
                shift_held(gesture),
            );
            schedule_autosave(&state);
            emit_busy(&state);
            start_canvas_fx(&area, &state);
            area.queue_draw();
        }
    });
    area.add_controller(drag);
}

fn stylus_pressure(state: &CanvasState, gesture: &gtk::GestureStylus, include_tilt: bool) -> f32 {
    if !state.runtime.use_pressure {
        return 1.0;
    }
    let pressure = gesture.axis(gdk::AxisUse::Pressure).unwrap_or(1.0) as f32;
    let tilt = if include_tilt && state.runtime.use_tilt {
        gesture
            .axis(gdk::AxisUse::Xtilt)
            .and_then(|x| gesture.axis(gdk::AxisUse::Ytilt).map(|y| (x, y)))
            .map(|(x, y)| (x.abs() + y.abs()) as f32 * 0.35)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    (pressure + tilt).clamp(0.05, 1.0)
}

fn attach_stylus_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let stylus = gtk::GestureStylus::new();
    stylus.connect_down({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let (pressure, eraser) = {
                let state = state.borrow();
                let pressure = stylus_pressure(&state, gesture, true);
                let eraser = gesture
                    .device_tool()
                    .is_some_and(|tool| tool.tool_type() == gdk::DeviceToolType::Eraser)
                    || (state.runtime.barrel_eraser && gesture.current_button() == 2);
                (pressure, eraser)
            };
            {
                let mut state = state.borrow_mut();
                state.stylus_active = true;
                state.last_stylus = Some(Instant::now());
                state.begin_input(
                    Point::new(x as f32, y as f32),
                    pressure,
                    eraser,
                    1,
                    shift_held(gesture),
                );
            }
            emit_busy(&state);
            emit_text_loaded(&state);
            start_canvas_fx(&area, &state);
            area.grab_focus();
            area.queue_draw();
        }
    });
    stylus.connect_motion({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let pressure = stylus_pressure(&state.borrow(), gesture, false);
            state.borrow_mut().update_input(
                Point::new(x as f32, y as f32),
                pressure,
                shift_held(gesture),
            );
            area.queue_draw();
        }
    });
    stylus.connect_up({
        let state_ref = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let pressure = stylus_pressure(&state_ref.borrow(), gesture, false);
            let mut state = state_ref.borrow_mut();
            state.end_input(
                Point::new(x as f32, y as f32),
                pressure,
                shift_held(gesture),
            );
            state.stylus_active = false;
            state.last_stylus = Some(Instant::now());
            drop(state);
            schedule_autosave(&state_ref);
            emit_busy(&state_ref);
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

fn attach_keys(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let state = state.clone();
        let area = area.clone();
        move |_, key, _, _| {
            if key == gdk::Key::Shift_L || key == gdk::Key::Shift_R {
                state.borrow_mut().constrain = true;
                area.queue_draw();
                return glib::Propagation::Proceed;
            }
            if key == gdk::Key::Escape {
                state.borrow_mut().cancel_input();
                emit_busy(&state);
                emit_replay(&state);
                area.queue_draw();
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::space {
                if state.borrow().replay.is_none() {
                    return glib::Propagation::Proceed;
                }
                let should_tick = {
                    let mut state = state.borrow_mut();
                    let duration = state.replay_duration();
                    let Some(replay) = state.replay.as_mut() else {
                        return glib::Propagation::Proceed;
                    };
                    if replay.playing {
                        replay.commit();
                        replay.playing = false;
                        false
                    } else {
                        if duration > 0.0 && replay.elapsed_secs >= duration - 0.0005 {
                            replay.elapsed_secs = 0.0;
                        }
                        replay.last_tick = Instant::now();
                        replay.playing = true;
                        true
                    }
                };
                emit_replay(&state);
                if should_tick {
                    start_replay_ticks(&area, &state);
                }
                area.queue_draw();
                return glib::Propagation::Stop;
            }
            let tool = if !state.borrow().runtime.tool_shortcuts {
                None
            } else {
                match key {
                    gdk::Key::v | gdk::Key::V => Some(Tool::Select),
                    gdk::Key::p | gdk::Key::P => Some(Tool::Pen),
                    gdk::Key::h | gdk::Key::H => Some(Tool::Highlighter),
                    gdk::Key::e | gdk::Key::E => Some(Tool::Eraser),
                    gdk::Key::t | gdk::Key::T => Some(Tool::Text),
                    gdk::Key::s | gdk::Key::S => Some(Tool::Shape),
                    gdk::Key::m | gdk::Key::M => Some(Tool::Measure),
                    _ => None,
                }
            };
            if let Some(tool) = tool {
                let listeners = {
                    let mut state = state.borrow_mut();
                    state.tool = tool;
                    state.interaction = None;
                    state.tool_listeners.clone()
                };
                for listener in listeners {
                    listener(tool);
                }
                area.queue_draw();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    keys.connect_key_released({
        let state = state.clone();
        let area = area.clone();
        move |_, key, _, _| {
            if key == gdk::Key::Shift_L || key == gdk::Key::Shift_R {
                state.borrow_mut().constrain = false;
                area.queue_draw();
            }
        }
    });
    area.add_controller(keys);
}

fn attach_file_drop(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let drop = gtk::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
    drop.set_types(&[gio::File::static_type()]);
    drop.connect_drop({
        let state = state.clone();
        let area = area.clone();
        move |_, value, x, y| {
            let Ok(file) = value.get::<gio::File>() else {
                return false;
            };
            let Some(path) = file.path() else {
                return false;
            };
            let paths = vec![path];
            let world = state
                .borrow()
                .screen_to_world(Point::new(x as f32, y as f32));
            for path in paths {
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let result = if extension == "svg" {
                    fs::read_to_string(&path)
                        .map_err(DocumentError::from)
                        .and_then(|svg| import_svg_elements(&svg))
                        .and_then(|elements| {
                            let mut state = state.borrow_mut();
                            if state.active_layer().locked {
                                return Err(DocumentError::Invalid(
                                    "the active layer is locked".to_owned(),
                                ));
                            }
                            for element in elements {
                                state.add_element(element);
                            }
                            Ok(())
                        })
                } else {
                    import_media_into(&state, &area, &path, Some(world))
                };
                if result.is_err() {
                    return false;
                }
            }
            schedule_autosave(&state);
            area.queue_draw();
            true
        }
    });
    area.add_controller(drop);
}

fn import_media_into(
    state: &Rc<RefCell<CanvasState>>,
    area: &gtk::DrawingArea,
    path: &Path,
    at: Option<Point>,
) -> Result<(), DocumentError> {
    // Reuse Canvas::import_media by reconstructing from widget-less state is awkward;
    // duplicate the core import here with an optional drop point.
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
        "png" => (MediaKind::Image, "image/png".to_owned()),
        "jpg" | "jpeg" => (MediaKind::Image, "image/jpeg".to_owned()),
        "webp" => (MediaKind::Image, "image/webp".to_owned()),
        "gif" => (MediaKind::Image, "image/gif".to_owned()),
        "pdf" => (MediaKind::Pdf, "application/pdf".to_owned()),
        "mp3" => (MediaKind::Audio, "audio/mpeg".to_owned()),
        "wav" => (MediaKind::Audio, "audio/wav".to_owned()),
        "ogg" | "oga" => (MediaKind::Audio, "audio/ogg".to_owned()),
        "flac" => (MediaKind::Audio, "audio/flac".to_owned()),
        "m4a" => (MediaKind::Audio, "audio/mp4".to_owned()),
        other if !other.is_empty() => (MediaKind::File, format!("application/{other}")),
        _ => {
            return Err(DocumentError::Invalid(
                "supported imports are images, PDF, audio, SVG, and other files".to_owned(),
            ));
        }
    };
    let bytes = fs::read(path)?;
    let asset_id = Uuid::new_v4();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Embedded file")
        .to_owned();
    let mut state = state.borrow_mut();
    if state.active_layer().locked {
        return Err(DocumentError::Invalid(
            "the active layer is locked".to_owned(),
        ));
    }
    let center = at.unwrap_or_else(|| {
        state.screen_to_world(Point::new(
            area.width() as f32 / 2.0,
            area.height() as f32 / 2.0,
        ))
    });
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
        match kind {
            MediaKind::Audio => (280.0, 72.0),
            _ => (320.0, 88.0),
        }
    };
    state.notebook.assets.push(Asset {
        id: asset_id,
        name: name.clone(),
        media_type,
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
    Ok(())
}

fn draw_canvas(context: &Context, width: i32, height: i32, state: &CanvasState) {
    let canvas = &state.page().canvas;
    match canvas.layout {
        PageLayout::Infinite => {
            set_source(context, canvas.background);
            let _ = context.paint();
        }
        PageLayout::Fixed | PageLayout::ContinuousVertical => {
            context.set_source_rgb(0.82, 0.84, 0.87);
            let _ = context.paint();
        }
    }

    let _ = context.save();
    context.translate(state.pan.x as f64, state.pan.y as f64);
    context.scale(state.zoom as f64, state.zoom as f64);

    let viewport = Rect {
        x: -state.pan.x / state.zoom,
        y: -state.pan.y / state.zoom,
        width: width as f32 / state.zoom,
        height: height as f32 / state.zoom,
    };
    draw_page_sheets(context, state, viewport);
    match canvas.pattern() {
        BackgroundPattern::None => {}
        BackgroundPattern::Grid => draw_grid(context, state, viewport),
        BackgroundPattern::Dots => draw_dots(context, state, viewport),
        BackgroundPattern::Lines => draw_lines(context, state, viewport),
    }

    let replay_events = state
        .replay
        .as_ref()
        .map(|_| local::replay_timeline(state.page().visible_elements()));
    let replay_seconds = state.replay.as_ref().map(ReplayPlayback::seconds);
    for element in state.page().visible_elements() {
        if element
            .bounds()
            .intersects(viewport.expand(48.0 / state.zoom))
            && matches!(element, Element::Stroke(stroke) if stroke.kind == StrokeKind::Highlighter)
        {
            draw_replay_element(
                context,
                element,
                &state.image_cache,
                replay_events.as_deref(),
                replay_seconds,
            );
        }
    }
    for element in state.page().visible_elements() {
        if element
            .bounds()
            .intersects(viewport.expand(48.0 / state.zoom))
            && !matches!(element, Element::Stroke(stroke) if stroke.kind == StrokeKind::Highlighter)
        {
            draw_replay_element(
                context,
                element,
                &state.image_cache,
                replay_events.as_deref(),
                replay_seconds,
            );
        }
    }
    if state.replay.is_none() {
        draw_selection(context, state);
    }
    draw_interaction(context, state);
    let _ = context.restore();
    draw_page_fade(context, width, height, state);
    draw_empty_hint(context, width, height, state);
    draw_replay_progress(context, width, height, state);
}

fn draw_page_sheets(context: &Context, state: &CanvasState, viewport: Rect) {
    let canvas = &state.page().canvas;
    let page_width = canvas.page_width.max(1.0);
    let page_height = canvas.page_height.max(1.0);
    let sheets = match canvas.layout {
        PageLayout::Infinite => return,
        PageLayout::Fixed => 1,
        PageLayout::ContinuousVertical => {
            let bottom = viewport.y + viewport.height;
            ((bottom / page_height).ceil() as i32 + 1).max(1)
        }
    };
    for index in 0..sheets {
        let y = index as f32 * page_height;
        context.set_source_rgba(0.0, 0.0, 0.0, 0.08);
        rounded_rect(
            context,
            Rect {
                x: 6.0,
                y: y + 8.0,
                width: page_width,
                height: page_height,
            },
            4.0,
        );
        let _ = context.fill();
        set_source(context, canvas.background);
        context.rectangle(0.0, y as f64, page_width as f64, page_height as f64);
        let _ = context.fill();
        context.set_source_rgba(0.55, 0.58, 0.62, 0.55);
        context.set_line_width(1.0 / state.zoom as f64);
        context.rectangle(0.0, y as f64, page_width as f64, page_height as f64);
        let _ = context.stroke();
    }
}

fn draw_dots(context: &Context, state: &CanvasState, viewport: Rect) {
    let mut spacing = state.page().canvas.grid_spacing;
    while spacing * state.zoom < 12.0 {
        spacing *= 2.0;
    }
    let start_x = (viewport.x / spacing).floor() as i32 - 1;
    let end_x = ((viewport.x + viewport.width) / spacing).ceil() as i32 + 1;
    let start_y = (viewport.y / spacing).floor() as i32 - 1;
    let end_y = ((viewport.y + viewport.height) / spacing).ceil() as i32 + 1;
    context.set_source_rgba(0.42, 0.46, 0.54, 0.45);
    let radius = (1.3 / state.zoom as f64).max(0.6);
    for x in start_x..=end_x {
        for y in start_y..=end_y {
            context.arc(
                x as f64 * spacing as f64,
                y as f64 * spacing as f64,
                radius,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.fill();
        }
    }
}

fn draw_lines(context: &Context, state: &CanvasState, viewport: Rect) {
    let mut spacing = state.page().canvas.grid_spacing;
    while spacing * state.zoom < 16.0 {
        spacing *= 2.0;
    }
    let start_y = (viewport.y / spacing).floor() as i32 - 1;
    let end_y = ((viewport.y + viewport.height) / spacing).ceil() as i32 + 1;
    context.set_line_width(1.0 / state.zoom as f64);
    context.set_source_rgba(0.42, 0.46, 0.54, 0.22);
    for y in start_y..=end_y {
        let y = y as f64 * spacing as f64;
        context.move_to(viewport.x as f64, y);
        context.line_to((viewport.x + viewport.width) as f64, y);
    }
    let _ = context.stroke();
}

fn draw_grid(context: &Context, state: &CanvasState, viewport: Rect) {
    let mut spacing = state.page().canvas.grid_spacing;
    while spacing * state.zoom < 16.0 {
        spacing *= 2.0;
    }
    let major = spacing * 2.0;
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

fn draw_interaction(context: &Context, state: &CanvasState) {
    match &state.interaction {
        Some(Interaction::Stroke(stroke)) => draw_stroke(context, stroke),
        Some(Interaction::Shape { start, current }) => {
            let kind = if state.tool == Tool::Measure {
                ShapeKind::Dimension
            } else {
                state.shape_kind
            };
            let bounds = if kind.uses_drag_bounds() {
                Rect::from_drag(*start, *current)
            } else {
                Rect::from_points(*start, *current)
            };
            let label = if kind == ShapeKind::Dimension {
                local::dimension_label(*start, *current)
            } else {
                state.pending_label.clone()
            };
            draw_shape(
                context,
                &Shape {
                    id: Uuid::nil(),
                    kind,
                    bounds,
                    rotation_degrees: 0.0,
                    style: state.style.clone(),
                    fill: state.fill_enabled.then_some(Color {
                        alpha: 0.22,
                        ..state.style.color
                    }),
                    label,
                },
            );
        }
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
        | Some(Interaction::Resize { .. })
        | Some(Interaction::Rotate { .. })
        | Some(Interaction::Pan { .. })
        | Some(Interaction::Erase { .. })
        | None => {}
        Some(Interaction::Space {
            start_y, last_y, ..
        }) => {
            context.set_source_rgba(0.12, 0.38, 0.88, 0.12);
            let top = (*start_y).min(*last_y);
            let height = (*last_y - *start_y).abs().max(2.0 / state.zoom);
            context.rectangle(-10_000.0, top as f64, 20_000.0, height as f64);
            let _ = context.fill();
            context.set_source_rgba(0.12, 0.38, 0.88, 0.85);
            context.set_line_width(1.5 / state.zoom as f64);
            context.move_to(-10_000.0, *start_y as f64);
            context.line_to(10_000.0, *start_y as f64);
            let _ = context.stroke();
        }
    }
    if state.ruler {
        draw_ruler_guides(context, state);
    }
}

fn draw_ruler_guides(context: &Context, state: &CanvasState) {
    let origin = match &state.interaction {
        Some(Interaction::Stroke(stroke)) => stroke.points.first().map(|point| point.point()),
        Some(Interaction::Shape { start, .. }) => Some(*start),
        _ => None,
    };
    let Some(origin) = origin else {
        return;
    };
    context.set_source_rgba(0.12, 0.38, 0.88, 0.28);
    context.set_line_width(1.0 / state.zoom as f64);
    context.set_dash(&[6.0 / state.zoom as f64, 4.0 / state.zoom as f64], 0.0);
    context.move_to((origin.x - 2400.0) as f64, origin.y as f64);
    context.line_to((origin.x + 2400.0) as f64, origin.y as f64);
    context.move_to(origin.x as f64, (origin.y - 2400.0) as f64);
    context.line_to(origin.x as f64, (origin.y + 2400.0) as f64);
    let _ = context.stroke();
    context.set_dash(&[], 0.0);
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
    if let Some(bounds) = state.selection_bounds() {
        let bounds = bounds.expand(pad);
        let rotate = Point::new(bounds.center().x, bounds.y - 22.0 / state.zoom);
        context.set_source_rgba(0.18, 0.44, 0.92, appear as f64);
        context.set_line_width(1.2 / state.zoom as f64);
        context.move_to(bounds.center().x as f64, bounds.y as f64);
        context.line_to(rotate.x as f64, rotate.y as f64);
        let _ = context.stroke();
        context.arc(
            rotate.x as f64,
            rotate.y as f64,
            (5.0 / state.zoom) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.fill();
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
    if state.replay.is_some()
        || state.interaction.is_some()
        || state.page().visible_elements().next().is_some()
    {
        return;
    }
    let appear = fx_ease(state.empty_hint, EMPTY_HINT_SECS);
    let lift = 10.0 * (1.0 - appear) as f64;
    let title = "Start a page";
    let subtitle = "Draw, type, or import";
    context.set_font_size(22.0);
    context.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    let title_ext = context.text_extents(title).ok();
    context.set_font_size(13.0);
    let subtitle_ext = context.text_extents(subtitle).ok();
    let card_y = height as f64 / 2.0 - 28.0 - lift;
    context.set_font_size(22.0);
    context.set_source_rgba(0.22, 0.25, 0.30, (0.68 * appear) as f64);
    if let Some(ext) = title_ext {
        context.move_to(
            width as f64 / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
            card_y + 24.0,
        );
        let _ = context.show_text(title);
    }
    context.set_font_size(13.0);
    context.set_source_rgba(0.40, 0.44, 0.50, (0.56 * appear) as f64);
    if let Some(ext) = subtitle_ext {
        context.move_to(
            width as f64 / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
            card_y + 48.0,
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
        Element::Table(table) => draw_table(context, table),
        Element::Tag(tag) => draw_tag(context, tag),
    }
}

fn draw_replay_element(
    context: &Context,
    element: &Element,
    image_cache: &HashMap<Uuid, Pixbuf>,
    events: Option<&[local::ReplayEvent]>,
    seconds: Option<f32>,
) {
    let (Some(events), Some(seconds)) = (events, seconds) else {
        draw_element(context, element, image_cache);
        return;
    };
    let Some(event) = events.iter().find(|event| event.id == element.id()) else {
        return;
    };
    let is_stroke = matches!(element, Element::Stroke(_));
    let Some(progress) = local::replay_progress(seconds, event, is_stroke) else {
        return;
    };
    match element {
        Element::Stroke(stroke) if progress < 1.0 => {
            let prefix = local::stroke_prefix(stroke, progress);
            draw_stroke(context, &prefix);
        }
        _ => draw_element(context, element, image_cache),
    }
}

fn draw_replay_progress(context: &Context, width: i32, height: i32, state: &CanvasState) {
    if state.replay.is_none() {
        return;
    }
    let status = state.replay_status();
    context.set_source_rgba(0.12, 0.38, 0.88, 0.16);
    context.rectangle(0.0, height as f64 - 4.0, width as f64, 4.0);
    let _ = context.fill();
    context.set_source_rgba(0.12, 0.38, 0.88, 0.92);
    context.rectangle(
        0.0,
        height as f64 - 4.0,
        width as f64 * status.progress as f64,
        4.0,
    );
    let _ = context.fill();
}

fn draw_stroke(context: &Context, stroke: &Stroke) {
    if stroke.points.is_empty() {
        return;
    }
    set_source(context, stroke.style.color);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);
    apply_dash(context, stroke.style.dashed, stroke.style.width);
    if stroke.points.len() == 1 {
        let point = stroke.points[0];
        let radius = (stroke.style.width * point.pressure.max(0.35) * 0.5).max(0.4) as f64;
        context.arc(
            point.x as f64,
            point.y as f64,
            radius,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.fill();
        context.set_dash(&[], 0.0);
        return;
    }
    set_source(context, stroke.style.color);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);
    apply_dash(context, stroke.style.dashed, stroke.style.width);
    if stroke.style.dashed {
        context.set_line_width(stroke.style.width as f64);
        context.move_to(stroke.points[0].x as f64, stroke.points[0].y as f64);
        for point in &stroke.points[1..] {
            context.line_to(point.x as f64, point.y as f64);
        }
        let _ = context.stroke();
        context.set_dash(&[], 0.0);
        return;
    }
    for pair in stroke.points.windows(2) {
        let mut pressure = ((pair[0].pressure + pair[1].pressure) / 2.0).clamp(0.12, 1.0);
        if stroke.kind == StrokeKind::Brush {
            pressure = pressure.powf(0.65);
        }
        context.set_line_width((stroke.style.width * pressure) as f64);
        context.move_to(pair[0].x as f64, pair[0].y as f64);
        context.line_to(pair[1].x as f64, pair[1].y as f64);
        let _ = context.stroke();
    }
}

fn apply_dash(context: &Context, dashed: bool, width: f32) {
    if dashed {
        let dash = (width * 3.2).max(4.0) as f64;
        context.set_dash(&[dash, dash * 0.7], 0.0);
    } else {
        context.set_dash(&[], 0.0);
    }
}

fn draw_text(context: &Context, text: &TextNote) {
    let bounds = Element::Text(text.clone()).bounds();
    if let Some(highlight) = text.highlight {
        set_source(context, highlight);
        context.rectangle(
            bounds.x as f64,
            bounds.y as f64,
            bounds.width as f64,
            bounds.height as f64,
        );
        let _ = context.fill();
    }
    set_source(context, text.color);
    context.select_font_face(
        "Sans",
        if text.italic {
            gtk::cairo::FontSlant::Italic
        } else {
            gtk::cairo::FontSlant::Normal
        },
        if text.bold {
            gtk::cairo::FontWeight::Bold
        } else {
            gtk::cairo::FontWeight::Normal
        },
    );
    context.set_font_size(text.font_size as f64);
    let prefix = text.prefix();
    for (index, line) in text.text.lines().enumerate() {
        let y = (text.origin.y + index as f32 * text.font_size * 1.25) as f64;
        let shown = format!("{prefix}{line}");
        context.move_to(text.origin.x as f64, y);
        let _ = context.show_text(&shown);
        if text.underline || text.href.is_some() {
            let width = context
                .text_extents(&shown)
                .map(|value| value.width())
                .unwrap_or(bounds.width as f64);
            context.set_line_width((text.font_size * 0.08).max(1.0) as f64);
            context.move_to(text.origin.x as f64, y + 2.0);
            context.line_to(text.origin.x as f64 + width, y + 2.0);
            let _ = context.stroke();
        }
    }
}

fn draw_connector(context: &Context, connector: &Connector) {
    set_source(context, connector.style.color);
    context.set_line_width(connector.style.width as f64);
    apply_dash(context, connector.style.dashed, connector.style.width);
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
    context.set_font_size(16.0);
    context.move_to(
        (media.bounds.x + 16.0) as f64,
        (media.bounds.y + 32.0) as f64,
    );
    let kind = match media.kind {
        MediaKind::Pdf => "PDF",
        MediaKind::Audio => "Audio",
        MediaKind::File => "File",
        MediaKind::Image => "Image",
    };
    let title = format!("{kind} · {}", media.caption);
    let _ = context.show_text(&title);
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    context.set_font_size(12.0);
    context.set_source_rgba(0.35, 0.38, 0.44, 1.0);
    context.move_to(
        (media.bounds.x + 16.0) as f64,
        (media.bounds.y + 52.0) as f64,
    );
    let _ = context.show_text("Select, then Insert → Open attachment");
}

fn draw_table(context: &Context, table: &TableElement) {
    let bounds = table.bounds;
    context.set_source_rgba(1.0, 1.0, 1.0, 0.96);
    rounded_rect(context, bounds, 6.0);
    let _ = context.fill_preserve();
    context.set_source_rgba(0.32, 0.36, 0.42, 0.85);
    context.set_line_width(1.2);
    let _ = context.stroke();
    let columns = table.columns.max(1) as f32;
    let rows = table.rows.max(1) as f32;
    let cell_w = bounds.width / columns;
    let cell_h = bounds.height / rows;
    context.set_source_rgba(0.42, 0.46, 0.52, 0.45);
    context.set_line_width(1.0);
    for column in 1..table.columns {
        let x = bounds.x + cell_w * column as f32;
        context.move_to(x as f64, bounds.y as f64);
        context.line_to(x as f64, (bounds.y + bounds.height) as f64);
    }
    for row in 1..table.rows {
        let y = bounds.y + cell_h * row as f32;
        context.move_to(bounds.x as f64, y as f64);
        context.line_to((bounds.x + bounds.width) as f64, y as f64);
    }
    let _ = context.stroke();
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    context.set_font_size((cell_h * 0.42).clamp(10.0, 16.0) as f64);
    context.set_source_rgba(0.16, 0.18, 0.22, 1.0);
    for (index, cell) in table.cells.iter().enumerate() {
        if cell.is_empty() {
            continue;
        }
        let column = (index as u32) % table.columns;
        let row = (index as u32) / table.columns;
        context.move_to(
            (bounds.x + cell_w * column as f32 + 8.0) as f64,
            (bounds.y + cell_h * row as f32 + cell_h * 0.68) as f64,
        );
        let _ = context.show_text(cell);
    }
}

fn draw_tag(context: &Context, tag: &TagElement) {
    let (width, height) = tag.size();
    let bounds = Rect {
        x: tag.origin.x,
        y: tag.origin.y,
        width,
        height,
    };
    let color = tag.kind.color();
    context.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        0.14,
    );
    rounded_rect(context, bounds, 10.0);
    let _ = context.fill_preserve();
    context.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        0.95,
    );
    context.set_line_width(1.2);
    let _ = context.stroke();
    if tag.kind == TagKind::ToDo {
        let box_bounds = Rect {
            x: tag.origin.x + 8.0,
            y: tag.origin.y + 7.0,
            width: 14.0,
            height: 14.0,
        };
        rounded_rect(context, box_bounds, 3.0);
        let _ = context.stroke();
        if tag.checked {
            context.move_to((tag.origin.x + 11.0) as f64, (tag.origin.y + 14.0) as f64);
            context.line_to((tag.origin.x + 14.0) as f64, (tag.origin.y + 18.0) as f64);
            context.line_to((tag.origin.x + 20.0) as f64, (tag.origin.y + 10.0) as f64);
            let _ = context.stroke();
        }
    }
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Bold,
    );
    context.set_font_size(12.0);
    let label_x = if tag.kind == TagKind::ToDo {
        tag.origin.x + 28.0
    } else {
        tag.origin.x + 12.0
    };
    context.move_to(label_x as f64, (tag.origin.y + 18.0) as f64);
    let label = if tag.note == tag.kind.label() {
        tag.note.clone()
    } else {
        format!("{} · {}", tag.kind.label(), tag.note)
    };
    let _ = context.show_text(&label);
}

fn draw_shape(context: &Context, shape: &Shape) {
    let bounds = if shape.kind.uses_drag_bounds() {
        shape.bounds
    } else {
        shape.bounds.normalized()
    };
    if bounds.width.abs() < f32::EPSILON && bounds.height.abs() < f32::EPSILON {
        return;
    }
    let box_bounds = bounds.normalized();
    if !shape.kind.uses_drag_bounds()
        && (box_bounds.width < f32::EPSILON || box_bounds.height < f32::EPSILON)
    {
        return;
    }
    let _ = context.save();
    let center = box_bounds.center();
    context.translate(center.x as f64, center.y as f64);
    context.rotate(shape.rotation_degrees.to_radians() as f64);
    context.translate(-center.x as f64, -center.y as f64);
    set_source(context, shape.style.color);
    context.set_line_width(shape.style.width as f64);
    apply_dash(context, shape.style.dashed, shape.style.width);
    context.set_line_cap(LineCap::Round);
    context.set_line_join(LineJoin::Round);

    match shape.kind {
        ShapeKind::Rectangle => {
            context.rectangle(
                box_bounds.x as f64,
                box_bounds.y as f64,
                box_bounds.width as f64,
                box_bounds.height as f64,
            );
            fill_and_stroke(context, shape.fill, shape.style.color);
        }
        ShapeKind::Ellipse => {
            ellipse_path(context, box_bounds);
            fill_and_stroke(context, shape.fill, shape.style.color);
        }
        ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Dimension => {
            let start = bounds.start();
            let end = bounds.end();
            context.move_to(start.x as f64, start.y as f64);
            context.line_to(end.x as f64, end.y as f64);
            let _ = context.stroke();
            if shape.kind != ShapeKind::Line {
                draw_arrow_head(context, start, end, bounds);
            }
        }
        ShapeKind::Triangle => {
            context.move_to(box_bounds.center().x as f64, box_bounds.y as f64);
            context.line_to(
                (box_bounds.x + box_bounds.width) as f64,
                (box_bounds.y + box_bounds.height) as f64,
            );
            context.line_to(
                box_bounds.x as f64,
                (box_bounds.y + box_bounds.height) as f64,
            );
            context.close_path();
            fill_and_stroke(context, shape.fill, shape.style.color);
        }
        ShapeKind::Spring => draw_zigzag(context, box_bounds),
        ShapeKind::Resistor => draw_iec_resistor(context, box_bounds),
        ShapeKind::Capacitor => {
            let left = box_bounds.x + box_bounds.width * 0.42;
            let right = box_bounds.x + box_bounds.width * 0.58;
            context.move_to(box_bounds.x as f64, box_bounds.center().y as f64);
            context.line_to(left as f64, box_bounds.center().y as f64);
            context.move_to(right as f64, box_bounds.center().y as f64);
            context.line_to(
                (box_bounds.x + box_bounds.width) as f64,
                box_bounds.center().y as f64,
            );
            context.move_to(left as f64, box_bounds.y as f64);
            context.line_to(left as f64, (box_bounds.y + box_bounds.height) as f64);
            context.move_to(right as f64, box_bounds.y as f64);
            context.line_to(right as f64, (box_bounds.y + box_bounds.height) as f64);
            let _ = context.stroke();
        }
        ShapeKind::Diode => draw_iec_diode(context, box_bounds),
        ShapeKind::Inductor => draw_iec_inductor(context, box_bounds),
        ShapeKind::Switch => draw_iec_switch(context, box_bounds),
        ShapeKind::Fuse => draw_iec_fuse(context, box_bounds),
        ShapeKind::Battery => draw_iec_battery(context, box_bounds),
        ShapeKind::Ground => {
            let center_x = box_bounds.center().x;
            context.move_to(center_x as f64, box_bounds.y as f64);
            context.line_to(
                center_x as f64,
                (box_bounds.y + box_bounds.height * 0.45) as f64,
            );
            for (offset, y, width) in [(0.0, 0.45, 1.0), (0.18, 0.68, 0.64), (0.36, 0.9, 0.28)] {
                context.move_to(
                    (box_bounds.x + box_bounds.width * offset) as f64,
                    (box_bounds.y + box_bounds.height * y) as f64,
                );
                context.line_to(
                    (box_bounds.x + box_bounds.width * (offset + width)) as f64,
                    (box_bounds.y + box_bounds.height * y) as f64,
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Motor => {
            ellipse_path(context, box_bounds);
            let _ = context.stroke();
            context.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Bold,
            );
            context.set_font_size((box_bounds.height * 0.45).min(box_bounds.width * 0.45) as f64);
            context.move_to(
                (box_bounds.center().x - box_bounds.width * 0.16) as f64,
                (box_bounds.center().y + box_bounds.height * 0.16) as f64,
            );
            let _ = context.show_text("M");
        }
        ShapeKind::Gear => {
            ellipse_path(context, box_bounds);
            let _ = context.stroke();
            context.arc(
                box_bounds.center().x as f64,
                box_bounds.center().y as f64,
                box_bounds.width.min(box_bounds.height) as f64 * 0.16,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.stroke();
            for index in 0..8 {
                let angle = index as f64 * std::f64::consts::TAU / 8.0;
                let inner = box_bounds.width.min(box_bounds.height) as f64 * 0.36;
                let outer = box_bounds.width.min(box_bounds.height) as f64 * 0.54;
                context.move_to(
                    box_bounds.center().x as f64 + inner * angle.cos(),
                    box_bounds.center().y as f64 + inner * angle.sin(),
                );
                context.line_to(
                    box_bounds.center().x as f64 + outer * angle.cos(),
                    box_bounds.center().y as f64 + outer * angle.sin(),
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Bearing => {
            ellipse_path(context, box_bounds);
            let _ = context.stroke();
            context.arc(
                box_bounds.center().x as f64,
                box_bounds.center().y as f64,
                box_bounds.width.min(box_bounds.height) as f64 * 0.18,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.stroke();
            for index in 0..6 {
                let angle = index as f64 * std::f64::consts::TAU / 6.0;
                context.arc(
                    box_bounds.center().x as f64
                        + box_bounds.width.min(box_bounds.height) as f64 * 0.34 * angle.cos(),
                    box_bounds.center().y as f64
                        + box_bounds.width.min(box_bounds.height) as f64 * 0.34 * angle.sin(),
                    box_bounds.width.min(box_bounds.height) as f64 * 0.06,
                    0.0,
                    std::f64::consts::TAU,
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Beam => {
            context.rectangle(
                box_bounds.x as f64,
                box_bounds.y as f64,
                box_bounds.width as f64,
                box_bounds.height as f64,
            );
            let _ = context.stroke();
            let mut x = box_bounds.x - box_bounds.height;
            while x < box_bounds.x + box_bounds.width {
                context.move_to(
                    x.max(box_bounds.x) as f64,
                    (box_bounds.y + box_bounds.height) as f64,
                );
                context.line_to(
                    (x + box_bounds.height).min(box_bounds.x + box_bounds.width) as f64,
                    box_bounds.y as f64,
                );
                x += box_bounds.height * 0.65;
            }
            let _ = context.stroke();
        }
        ShapeKind::Lamp => draw_iec_lamp(context, box_bounds),
        ShapeKind::Transformer => draw_iec_transformer(context, box_bounds),
        ShapeKind::AndGate | ShapeKind::NandGate => {
            draw_logic_and(context, box_bounds, shape.kind == ShapeKind::NandGate);
        }
        ShapeKind::OrGate | ShapeKind::NorGate | ShapeKind::XorGate => {
            draw_logic_or(context, box_bounds, shape.kind);
        }
        ShapeKind::NotGate => draw_logic_not(context, box_bounds),
        ShapeKind::SurfaceFinish => {
            context.move_to(
                box_bounds.x as f64,
                (box_bounds.y + box_bounds.height * 0.62) as f64,
            );
            context.line_to(
                (box_bounds.x + box_bounds.width * 0.32) as f64,
                (box_bounds.y + box_bounds.height) as f64,
            );
            context.line_to(
                (box_bounds.x + box_bounds.width) as f64,
                box_bounds.y as f64,
            );
            let _ = context.stroke();
        }
        ShapeKind::ThirdAngle => draw_third_angle(context, box_bounds),
    }
    let _ = context.restore();

    if !shape.label.is_empty() {
        draw_label(
            context,
            Point::new(
                box_bounds.center().x,
                box_bounds.y.min(box_bounds.y + box_bounds.height) + box_bounds.height + 16.0,
            ),
            &shape.label,
            shape.style.color,
        );
    }
}

fn draw_arrow_head(context: &Context, start: Point, end: Point, bounds: Rect) {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let size = (bounds.width.abs().hypot(bounds.height.abs()) * 0.18).clamp(8.0, 22.0);
    context.move_to(end.x as f64, end.y as f64);
    context.line_to(
        (end.x - size * (angle - 0.45).cos()) as f64,
        (end.y - size * (angle - 0.45).sin()) as f64,
    );
    context.move_to(end.x as f64, end.y as f64);
    context.line_to(
        (end.x - size * (angle + 0.45).cos()) as f64,
        (end.y - size * (angle + 0.45).sin()) as f64,
    );
    let _ = context.stroke();
}

fn draw_zigzag(context: &Context, bounds: Rect) {
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

fn draw_iec_resistor(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.22;
    let x2 = bounds.x + bounds.width * 0.78;
    let y1 = bounds.y + bounds.height * 0.28;
    let y2 = bounds.y + bounds.height * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.rectangle(x1 as f64, y1 as f64, (x2 - x1) as f64, (y2 - y1) as f64);
    let _ = context.stroke();
}

fn draw_iec_diode(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.62;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, (bounds.y + bounds.height * 0.18) as f64);
    context.line_to(x2 as f64, cy as f64);
    context.line_to(x1 as f64, (bounds.y + bounds.height * 0.82) as f64);
    context.close_path();
    let _ = context.stroke();
    context.move_to(x2 as f64, (bounds.y + bounds.height * 0.18) as f64);
    context.line_to(x2 as f64, (bounds.y + bounds.height * 0.82) as f64);
    let _ = context.stroke();
}

fn draw_iec_inductor(context: &Context, bounds: Rect) {
    let cy = bounds.center().y as f64;
    let radius = (bounds.width.abs() / 10.0) as f64;
    context.move_to(bounds.x as f64, cy);
    context.line_to((bounds.x + bounds.width * 0.18) as f64, cy);
    for index in 0..4 {
        let cx = (bounds.x + bounds.width * (0.26 + index as f32 * 0.14)) as f64;
        context.arc(cx, cy, radius, std::f64::consts::PI, 0.0);
    }
    context.line_to((bounds.x + bounds.width) as f64, cy);
    let _ = context.stroke();
}

fn draw_iec_switch(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, cy as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.62) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    let _ = context.stroke();
    context.arc(x1 as f64, cy as f64, 2.4, 0.0, std::f64::consts::TAU);
    let _ = context.stroke();
    context.arc(x2 as f64, cy as f64, 2.4, 0.0, std::f64::consts::TAU);
    let _ = context.stroke();
}

fn draw_iec_fuse(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.rectangle(
        x1 as f64,
        (bounds.y + bounds.height * 0.32) as f64,
        (x2 - x1) as f64,
        (bounds.height * 0.36) as f64,
    );
    let _ = context.stroke();
}

fn draw_iec_battery(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.42;
    let x2 = bounds.x + bounds.width * 0.58;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, (bounds.y + bounds.height * 0.12) as f64);
    context.line_to(x1 as f64, (bounds.y + bounds.height * 0.88) as f64);
    context.move_to(x2 as f64, (bounds.y + bounds.height * 0.28) as f64);
    context.line_to(x2 as f64, (bounds.y + bounds.height * 0.72) as f64);
    let _ = context.stroke();
}

fn draw_iec_lamp(context: &Context, bounds: Rect) {
    let r = bounds.width.min(bounds.height) / 2.0;
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        r as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.move_to(
        (bounds.center().x - r * 0.62) as f64,
        (bounds.center().y - r * 0.62) as f64,
    );
    context.line_to(
        (bounds.center().x + r * 0.62) as f64,
        (bounds.center().y + r * 0.62) as f64,
    );
    context.move_to(
        (bounds.center().x - r * 0.62) as f64,
        (bounds.center().y + r * 0.62) as f64,
    );
    context.line_to(
        (bounds.center().x + r * 0.62) as f64,
        (bounds.center().y - r * 0.62) as f64,
    );
    let _ = context.stroke();
}

fn draw_iec_transformer(context: &Context, bounds: Rect) {
    let left = Rect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width * 0.42,
        height: bounds.height,
    };
    let right = Rect {
        x: bounds.x + bounds.width * 0.58,
        y: bounds.y,
        width: bounds.width * 0.42,
        height: bounds.height,
    };
    draw_iec_inductor(context, left);
    draw_iec_inductor(context, right);
    context.move_to(
        (bounds.x + bounds.width * 0.46) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.46) as f64,
        (bounds.y + bounds.height * 0.82) as f64,
    );
    context.move_to(
        (bounds.x + bounds.width * 0.54) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.54) as f64,
        (bounds.y + bounds.height * 0.82) as f64,
    );
    let _ = context.stroke();
}

fn draw_logic_and(context: &Context, bounds: Rect, nand: bool) {
    let mid = bounds.x + bounds.width * 0.55;
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.line_to(mid as f64, bounds.y as f64);
    context.arc(
        mid as f64,
        bounds.center().y as f64,
        (bounds.height / 2.0) as f64,
        -std::f64::consts::FRAC_PI_2,
        std::f64::consts::FRAC_PI_2,
    );
    context.line_to(bounds.x as f64, (bounds.y + bounds.height) as f64);
    context.close_path();
    let _ = context.stroke();
    if nand {
        context.arc(
            (bounds.x + bounds.width * 0.88) as f64,
            bounds.center().y as f64,
            (bounds.width * 0.07) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.stroke();
    }
}

fn draw_logic_or(context: &Context, bounds: Rect, kind: ShapeKind) {
    let cy = bounds.center().y;
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.curve_to(
        (bounds.x + bounds.width * 0.22) as f64,
        bounds.y as f64,
        (bounds.x + bounds.width * 0.55) as f64,
        (bounds.y + bounds.height * 0.08) as f64,
        (bounds.x + bounds.width * 0.82) as f64,
        cy as f64,
    );
    context.curve_to(
        (bounds.x + bounds.width * 0.55) as f64,
        (bounds.y + bounds.height * 0.92) as f64,
        (bounds.x + bounds.width * 0.22) as f64,
        (bounds.y + bounds.height) as f64,
        bounds.x as f64,
        (bounds.y + bounds.height) as f64,
    );
    context.curve_to(
        (bounds.x + bounds.width * 0.18) as f64,
        cy as f64,
        (bounds.x + bounds.width * 0.18) as f64,
        cy as f64,
        bounds.x as f64,
        bounds.y as f64,
    );
    let _ = context.stroke();
    if kind == ShapeKind::XorGate {
        context.move_to((bounds.x + bounds.width * 0.08) as f64, bounds.y as f64);
        context.curve_to(
            (bounds.x + bounds.width * 0.26) as f64,
            cy as f64,
            (bounds.x + bounds.width * 0.26) as f64,
            cy as f64,
            (bounds.x + bounds.width * 0.08) as f64,
            (bounds.y + bounds.height) as f64,
        );
        let _ = context.stroke();
    }
    if kind == ShapeKind::NorGate {
        context.arc(
            (bounds.x + bounds.width * 0.9) as f64,
            cy as f64,
            (bounds.width * 0.07) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.stroke();
    }
}

fn draw_logic_not(context: &Context, bounds: Rect) {
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.72) as f64,
        bounds.center().y as f64,
    );
    context.line_to(bounds.x as f64, (bounds.y + bounds.height) as f64);
    context.close_path();
    let _ = context.stroke();
    context.arc(
        (bounds.x + bounds.width * 0.84) as f64,
        bounds.center().y as f64,
        (bounds.width * 0.08) as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
}

fn draw_third_angle(context: &Context, bounds: Rect) {
    let r = bounds.width.min(bounds.height) * 0.38;
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        r as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        (r * 0.42) as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.move_to((bounds.x + bounds.width * 0.18) as f64, bounds.y as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.32) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.68) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to((bounds.x + bounds.width * 0.82) as f64, bounds.y as f64);
    context.close_path();
    let _ = context.stroke();
}

fn sanitize_export_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        "export.png".to_owned()
    } else {
        trimmed
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
