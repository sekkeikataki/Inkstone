use crate::canvas::{Canvas, PageDefaults, REPLAY_SPEEDS, ReplayStatus, RuntimePrefs, Tool};
use crate::document::{
    BackgroundPattern, Color, ISO_LINE_WIDTHS_MM, ListStyle, MAX_STROKE_WIDTH, MIN_STROKE_WIDTH,
    PageLayout, PaperSize, ShapeKind, TagKind, mm_to_pt, pt_to_mm,
};
use crate::library::{
    Library, Preferences, Session, ThemePref, config_dir, env_library_root, notebook_color_index,
};
use crate::local::{AlignMode, DateStamp, PageTemplate};
use adw::prelude::*;
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gio;
use gtk::glib;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use uuid::Uuid;

const APP_ID: &str = "dev.inkstone.Inkstone";
const INK_COLOR_CHOICES: [(&str, Color); 12] = [
    ("Ink", Color::INK),
    ("White", Color::rgb(0.97, 0.97, 0.96)),
    ("Gray", Color::rgb(0.42, 0.45, 0.50)),
    ("Blue", Color::BLUE),
    ("Cyan", Color::rgb(0.05, 0.60, 0.66)),
    ("Green", Color::rgb(0.05, 0.56, 0.32)),
    ("Amber", Color::rgb(0.95, 0.62, 0.05)),
    ("Orange", Color::rgb(0.89, 0.42, 0.07)),
    ("Red", Color::rgb(0.84, 0.16, 0.20)),
    ("Pink", Color::rgb(0.84, 0.24, 0.53)),
    ("Violet", Color::rgb(0.48, 0.20, 0.78)),
    ("Brown", Color::rgb(0.54, 0.35, 0.20)),
];
const CHROME_MARGIN: i32 = 16;
const TOOLS_GUTTER: i32 = 96;
const TRAY_Y: i32 = 16;
const WIDTH_MARGIN_END: i32 = 16;

#[derive(Clone)]
struct Navigator {
    sidebar: gtk::ScrolledWindow,
    search: gtk::SearchEntry,
    notebook_list: gtk::ListBox,
    library_filter: gtk::SearchEntry,
    current_name: gtk::EditableLabel,
    current_category: gtk::Button,
    add_to_library: gtk::Button,
    library: Rc<RefCell<Library>>,
    session: Rc<RefCell<Session>>,
    section_list: gtk::ListBox,
    page_list: gtk::ListBox,
    layer_list: gtk::ListBox,
    trash_list: gtk::ListBox,
    search_hits: gtk::ListBox,
    todo_list: gtk::ListBox,
    tabs: gtk::Box,
    open_paths: Rc<RefCell<Vec<PathBuf>>>,
    folder_monitor: Rc<RefCell<Option<gio::FileMonitor>>>,
    pattern: adw::ComboRow,
    paper: adw::ComboRow,
    grid: gtk::SpinButton,
    updating: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct Feedback {
    status: gtk::Label,
    zoom: gtk::Label,
    toasts: adw::ToastOverlay,
    title: adw::WindowTitle,
    toast_seconds: Rc<Cell<u32>>,
}

impl Feedback {
    fn show(&self, message: impl AsRef<str>) {
        let message = message.as_ref();
        self.status.set_text(message);
        let toast = adw::Toast::new(message);
        toast.set_timeout(self.toast_seconds.get());
        self.toasts.add_toast(toast);
    }

    fn whisper(&self, message: impl AsRef<str>) {
        self.status.set_text(message.as_ref());
    }

    fn set_zoom(&self, percent: u32) {
        self.zoom.set_label(&format!("{percent}%"));
    }
}

pub fn run() -> glib::ExitCode {
    let application = adw::Application::builder().application_id(APP_ID).build();
    application.connect_activate(build_window);
    application.run()
}

fn install_css() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_icon_theme_name(Some("Adwaita"));
    }
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        theme.add_search_path("/usr/share/icons/Adwaita");
        for path in icon_search_paths() {
            theme.add_search_path(path);
        }
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .workspace-sidebar {
            background-color: @sidebar_bg_color;
        }
        .sidebar-search {
            border-radius: 14px;
            min-height: 36px;
        }
        .library-scroll {
            min-height: 72px;
        }
        .category-row {
            padding: 8px 8px 2px 8px;
        }
        .category-label {
            letter-spacing: 0.04em;
        }
        .library-current {
            margin-bottom: 2px;
        }
        .section-header {
            padding: 2px 4px 0 4px;
        }
        .nav-list {
            margin-top: 2px;
        }
        .workspace-sidebar list {
            background: transparent;
        }
        .nav-row {
            border-radius: 14px;
            margin: 2px 0;
        }
        .nav-row:hover {
            background-color: alpha(@accent_bg_color, 0.08);
        }
        .nav-row:selected {
            background-color: alpha(@accent_bg_color, 0.16);
        }
        .nav-title {
            font-weight: 600;
        }
        .search-hits, .todo-list {
            margin-top: 4px;
        }
        .open-tabs {
            padding: 0 4px 6px 4px;
        }
        .tab-chip {
            border-radius: 999px;
            padding: 2px 10px;
            font-size: 0.85em;
        }
        .tab-chip:checked {
            background-color: alpha(@accent_bg_color, 0.22);
        }
        .shape-grid {
            padding: 8px;
        }
        .shape-chip {
            min-width: 52px;
            min-height: 28px;
            font-size: 0.78em;
        }
        .page-thumb {
            border-radius: 4px;
            background-color: alpha(@window_fg_color, 0.06);
        }
        .floating-panel {
            background-color: alpha(@window_bg_color, 0.97);
            border: 1px solid alpha(@borders, 0.10);
            box-shadow: 0 1px 2px alpha(black, 0.04), 0 14px 36px alpha(black, 0.08);
        }
        .tool-palette {
            border-radius: 24px;
            padding: 10px 8px;
        }
        .tool-options, .floating-tray {
            border-radius: 20px;
            padding: 5px 6px 5px 2px;
            min-height: 44px;
        }
        .panel-grip {
            min-width: 6px;
            min-height: 18px;
            margin: 8px 8px 8px 6px;
            padding: 0;
            border-radius: 99px;
            background-color: alpha(@window_fg_color, 0.22);
        }
        .panel-close {
            min-width: 28px;
            min-height: 28px;
            margin-left: 4px;
            padding: 4px;
            opacity: 0.58;
        }
        .panel-close-visible {
            opacity: 0.92;
        }
        .panel-close:hover {
            opacity: 1;
            background-color: alpha(@window_fg_color, 0.08);
        }
        .tool-button {
            border-radius: 999px;
            min-width: 36px;
            min-height: 36px;
            padding: 6px;
            color: @window_fg_color;
        }
        .tool-button:hover {
            background-color: alpha(@window_fg_color, 0.06);
        }
        .tool-button:checked {
            color: @accent_fg_color;
            background-color: @accent_bg_color;
        }
        .tool-button:checked image {
            color: @accent_fg_color;
        }
        .tool-separator {
            margin: 5px 10px;
            opacity: 0.22;
        }
        .option-separator {
            margin: 6px 6px;
            opacity: 0.22;
        }
        .canvas-surface {
            background-color: @view_bg_color;
        }
        .status-chip, .zoom-chip, .replay-chip {
            border-radius: 999px;
            padding: 5px 12px;
        }
        .replay-chip {
            padding: 4px 10px 4px 6px;
        }
        .replay-chip button {
            min-width: 28px;
            min-height: 28px;
        }
        .replay-speed {
            min-width: 38px;
            min-height: 26px;
            font-size: 0.8em;
            border-radius: 999px;
        }
        .zoom-chip button {
            min-width: 28px;
            min-height: 28px;
        }
        .zoom-value {
            min-width: 3.4em;
            font-weight: 600;
        }
        .color-swatch {
            min-width: 18px;
            min-height: 18px;
            padding: 0;
            margin: 0 1px;
            border-radius: 999px;
            border: 1px solid alpha(@borders, 0.34);
        }
        .color-swatch:hover {
            box-shadow: 0 0 0 3px alpha(@accent_bg_color, 0.22);
        }
        .color-swatch:checked {
            box-shadow: 0 0 0 2px @window_bg_color, 0 0 0 4px @accent_bg_color;
        }
        .color-add {
            min-width: 22px;
            min-height: 22px;
            padding: 0;
            margin: 0 2px;
            border-radius: 999px;
            color: alpha(@window_fg_color, 0.62);
            border: 1px dashed alpha(@window_fg_color, 0.28);
            background-color: transparent;
        }
        .color-add:hover {
            color: @window_fg_color;
            background-color: alpha(@window_fg_color, 0.06);
        }
        .width-stepper {
            border-radius: 11px;
            background-color: alpha(@window_fg_color, 0.05);
            padding: 1px;
        }
        .width-stepper button {
            min-width: 22px;
            min-height: 22px;
            padding: 0;
            border-radius: 9px;
        }
        .width-spin {
            min-width: 2.2em;
            min-height: 26px;
            border-radius: 9px;
            padding: 0;
            background: none;
            box-shadow: none;
            border: none;
        }
        spinbutton.compact-spin {
            padding: 0;
        }
        spinbutton.compact-spin button {
            min-width: 1px;
            min-height: 1px;
            padding: 0;
            margin: 0;
            border: none;
            box-shadow: none;
            background: none;
            opacity: 0;
            -gtk-icon-size: 1px;
        }
        spinbutton.compact-spin text {
            min-width: 2.2em;
            padding: 0 4px;
            background: none;
        }
        .width-presets {
            background: transparent;
            padding: 0;
        }
        .width-presets scrollbar {
            opacity: 0.28;
        }
        .width-chip, .style-chip {
            min-width: 22px;
            min-height: 26px;
            padding: 0 5px;
            border-radius: 999px;
            font-variant-numeric: tabular-nums;
        }
        .width-chip:checked, .style-chip:checked, .style-icon:checked {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
        }
        .width-add, .style-icon {
            min-width: 26px;
            min-height: 26px;
            padding: 0;
            border-radius: 999px;
        }
        .option-icon {
            min-width: 32px;
            min-height: 32px;
            padding: 6px;
        }
        .swatch-ink { background-color: #1a1f29; }
        .swatch-white { background-color: #f7f7f4; border-color: alpha(@borders, 0.9); }
        .swatch-gray { background-color: #6b7280; }
        .swatch-blue { background-color: #1f61e0; }
        .swatch-cyan { background-color: #0d9aa8; }
        .swatch-green { background-color: #0d8f52; }
        .swatch-amber { background-color: #f29e0d; }
        .swatch-orange { background-color: #e36b12; }
        .swatch-red { background-color: #d62933; }
        .swatch-pink { background-color: #d63d88; }
        .swatch-violet { background-color: #7a33c7; }
        .swatch-brown { background-color: #8a5a32; }
        .restore-tools, .restore-island {
            min-height: 36px;
            padding: 4px 12px;
            margin: 14px;
        }
        .restore-island {
            margin: 0;
        }
        .section-dot {
            min-width: 10px;
            min-height: 10px;
            border-radius: 99px;
            margin: 0 4px 0 2px;
        }
        .subpage-row {
            margin-left: 14px;
        }
        .format-chip {
            min-width: 28px;
            min-height: 28px;
            padding: 0;
            border-radius: 9px;
            font-weight: 700;
        }
        .insert-button {
            min-width: 34px;
            min-height: 34px;
        }
        .trash-empty {
            opacity: 0.72;
        }
        .circular-icon {
            min-width: 32px;
            min-height: 32px;
            padding: 0;
        }
