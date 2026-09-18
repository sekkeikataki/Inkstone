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
