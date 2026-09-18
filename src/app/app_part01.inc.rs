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
        }
        .header-file {
            min-width: 34px;
            min-height: 34px;
        }
        .nav-row button {
            min-width: 28px;
            min-height: 28px;
            margin: 4px 2px;
        }
        .workspace-sidebar .boxed-list row {
            min-height: 44px;
        }
        ",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn icon_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/icons")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join("../share/icons"));
        paths.push(dir.join("../share/inkstone/icons"));
    }
    if let Some(data_home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(data_home).join(".local/share/icons"));
    }
    paths
}

fn build_window(application: &adw::Application) {
    install_css();
    let canvas = Canvas::new();
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Inkstone — Untitled note")
        .default_width(1380)
        .default_height(860)
        .width_request(760)
        .height_request(520)
        .icon_name("applications-graphics-symbolic")
        .build();

    let split_view = adw::OverlaySplitView::builder()
        .collapsed(false)
        .show_sidebar(true)
        .enable_hide_gesture(true)
        .enable_show_gesture(true)
        .min_sidebar_width(292.0)
        .max_sidebar_width(360.0)
        .sidebar_width_fraction(0.24)
        .build();
    let compact = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        860.0,
        adw::LengthUnit::Sp,
    ));
    let collapsed = true.to_value();
    compact.add_setter(&split_view, "collapsed", Some(&collapsed));
    window.add_breakpoint(compact);

    let toast_overlay = adw::ToastOverlay::new();
    let window_title = adw::WindowTitle::new("Inkstone", "Untitled notebook");
    let status = gtk::Label::builder()
        .label("Ready")
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(42)
        .build();
    status.add_css_class("dim-label");
    status.add_css_class("caption");
    let zoom = gtk::Label::builder()
        .label("100%")
        .tooltip_text("Current zoom")
        .build();
    zoom.add_css_class("zoom-value");
    let feedback = Feedback {
        status,
        zoom,
        toasts: toast_overlay.clone(),
        title: window_title,
        toast_seconds: Rc::new(Cell::new(2)),
    };
    canvas.connect_view_changed({
        let feedback = feedback.clone();
        move |percent| feedback.set_zoom(percent)
    });

    let navigator = Navigator::new(&canvas, &feedback);
    apply_session_to_canvas(&canvas, &navigator.session.borrow().preferences);
    apply_session_style(&canvas, &navigator.session.borrow().preferences);
    apply_theme(navigator.session.borrow().preferences.theme);
    feedback
        .toast_seconds
        .set(navigator.session.borrow().preferences.toast_seconds);
    if navigator.session.borrow().preferences.remember_window {
        let prefs = navigator.session.borrow().preferences.clone();
        window.set_default_width(prefs.window_width.max(760));
        window.set_default_height(prefs.window_height.max(520));
    }
    navigator
        .tabs
        .set_visible(navigator.session.borrow().preferences.show_open_tabs);
    let (workspace, chrome) = build_canvas_workspace(&canvas, &feedback, &navigator.session);
    split_view.set_sidebar(Some(&navigator.sidebar));
    split_view.set_content(Some(&workspace));
    toast_overlay.set_child(Some(&split_view));

    let toolbar_view = adw::ToolbarView::new();
    let (header, tools_toggle, colors_toggle, widths_toggle) =
        build_header(&window, &feedback.title);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&toast_overlay));
    window.set_content(Some(&toolbar_view));
    wire_chrome_toggle(
        &window,
        &toolbar_view,
        &chrome,
        &tools_toggle,
        &colors_toggle,
        &widths_toggle,
    );
    apply_session_chrome(
        &navigator.session.borrow().preferences,
        &split_view,
        &tools_toggle,
        &chrome,
    );
    persist_workspace_preferences(&navigator.session, &split_view, &tools_toggle, &chrome);

    install_actions(
        application,
        &window,
        &canvas,
        &feedback,
        &navigator,
        &split_view,
        &chrome,
        &tools_toggle,
    );
    install_shortcuts(application);
    protect_unsaved_close(&window, &canvas, &navigator.session);
    window.connect_notify_local(Some("is-active"), {
        let navigator = navigator.clone();
        let canvas = canvas.clone();
        let was_active = Rc::new(Cell::new(false));
        move |window, _| {
            let active = window.is_active();
            if active && !was_active.get() && !navigator.updating.get() {
                navigator.refresh_library(&canvas);
            }
            was_active.set(active);
        }
    });
    window.present();
    refresh_status_quiet(&window, &canvas, &feedback, "Ready");
}

impl Navigator {
    fn new(canvas: &Canvas, feedback: &Feedback) -> Self {
        let page_list = boxed_list("page-list");
        let layer_list = boxed_list("layer-list");
        let section_list = boxed_list("section-list");
        let trash_list = boxed_list("trash-list");
        let updating = Rc::new(Cell::new(false));

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_start(16)
            .margin_end(16)
            .margin_top(18)
            .margin_bottom(16)
            .build();
        content.add_css_class("workspace-sidebar");

        let library = Rc::new(RefCell::new(match Library::open_default() {
            Ok(library) => library,
            Err(_) => Library {
                root: crate::library::fallback_library_root(),
                notebooks: Vec::new(),
                categories: Vec::new(),
            },
        }));
        let session = Rc::new(RefCell::new(Session::load()));

        let current_name = gtk::EditableLabel::builder()
            .text("My Notebook")
            .xalign(0.0)
            .hexpand(true)
            .tooltip_text("Click to rename this notebook. The file in Files is renamed to match.")
            .build();
        current_name.add_css_class("heading");
        current_name.add_css_class("library-current");
        let current_category = gtk::Button::builder()
            .label("Personal")
            .halign(gtk::Align::Start)
            .tooltip_text("Move this notebook into another category folder")
            .action_name("win.move-notebook")
            .build();
        current_category.add_css_class("dim-label");
        current_category.add_css_class("flat");
        current_category.add_css_class("caption");
        let add_to_library = gtk::Button::builder()
            .label("Keep in library")
            .tooltip_text("Move this file into the Inkstone folder so it appears here")
            .halign(gtk::Align::Start)
            .action_name("win.add-to-library")
            .build();
        add_to_library.add_css_class("pill");
        add_to_library.add_css_class("flat");
        add_to_library.set_visible(false);
        let brand_text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        brand_text.append(&current_name);
        brand_text.append(&current_category);
        brand_text.append(&add_to_library);
        let tabs = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();
        tabs.add_css_class("open-tabs");
        brand_text.append(&tabs);
        content.append(&brand_text);

        let add_notebook = circular_icon_button("list-add-symbolic", "New notebook");
        add_notebook.set_action_name(Some("win.new"));
        let add_category = circular_icon_button("folder-new-symbolic", "New category folder");
        add_category.set_action_name(Some("win.new-category"));
        let show_files = circular_icon_button("folder-symbolic", "Show library in Files");
        show_files.set_action_name(Some("win.show-library"));
        let notebook_actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .build();
        notebook_actions.append(&add_notebook);
        notebook_actions.append(&add_category);
        notebook_actions.append(&show_files);
        let notebooks_header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        notebooks_header.add_css_class("section-header");
        let notebooks_label = gtk::Label::builder()
            .label("Notebooks")
            .xalign(0.0)
            .hexpand(true)
            .build();
        notebooks_label.add_css_class("caption-heading");
        notebooks_header.append(&notebooks_label);
        notebooks_header.append(&notebook_actions);
        content.append(&notebooks_header);

        let library_filter = gtk::SearchEntry::builder()
            .placeholder_text("Find a notebook")
            .tooltip_text("Filter the library. Categories are the same folders you see in Files.")
            .build();
        library_filter.add_css_class("sidebar-search");
        content.append(&library_filter);

        let notebook_list = boxed_list("notebook-list");
        content.append(&notebook_list);

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search this notebook and the library")
            .tooltip_text(
                "Press Enter to jump. Matches notes, tables, and other notebooks in Files.",
            )
            .build();
        search.add_css_class("sidebar-search");
        content.append(&search);
        let search_hits = boxed_list("search-hits");
        search_hits.add_css_class("search-hits");
        content.append(&search_hits);

        let add_section = circular_icon_button("list-add-symbolic", "Add section");
        content.append(&section_header("Sections", Some(&add_section)));
        content.append(&section_list);

        let add_page = circular_icon_button("list-add-symbolic", "Add page");
        content.append(&section_header("Pages", Some(&add_page)));
        content.append(&page_list);
        let previous_page = circular_icon_button("go-previous-symbolic", "Previous page");
        let next_page = circular_icon_button("go-next-symbolic", "Next page");
        let add_subpage = circular_icon_button("go-down-symbolic", "Add subpage");
        let remove_page = circular_icon_button("user-trash-symbolic", "Move page to recycle bin");
        let page_actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let page_nav = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .build();
        page_nav.add_css_class("linked");
        page_nav.append(&previous_page);
        page_nav.append(&next_page);
        page_actions.append(&page_nav);
        page_actions.append(&add_subpage);
        let page_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        page_spacer.set_hexpand(true);
        page_actions.append(&page_spacer);
        page_actions.append(&remove_page);
        content.append(&page_actions);

        let todo_list = boxed_list("todo-list");
        todo_list.add_css_class("todo-list");
        content.append(&section_header("To-do", None));
        content.append(&todo_list);

        let add_layer = circular_icon_button("list-add-symbolic", "Add layer");
        content.append(&section_header("Layers", Some(&add_layer)));
        content.append(&layer_list);
        let remove_layer = circular_icon_button("list-remove-symbolic", "Remove layer");
        remove_layer.set_halign(gtk::Align::End);
        content.append(&remove_layer);

        let empty_trash = circular_icon_button("edit-clear-all-symbolic", "Empty recycle bin");
        empty_trash.add_css_class("trash-empty");
        content.append(&section_header("Recycle bin", Some(&empty_trash)));
        content.append(&trash_list);

        let pattern = adw::ComboRow::builder()
            .title("Background")
            .subtitle("5 mm")
            .model(&gtk::StringList::new(&BackgroundPattern::NAMES))
            .build();
        let grid = gtk::SpinButton::with_range(1.0, 50.0, 1.0);
        grid.set_digits(0);
        grid.set_value(canvas.grid_spacing_mm() as f64);
        grid.set_tooltip_text(Some("Grid spacing in millimetres"));
        grid.add_css_class("width-spin");
        grid.set_valign(gtk::Align::Center);
        let mm_label = gtk::Label::new(Some("mm"));
        mm_label.add_css_class("dim-label");
        mm_label.add_css_class("caption");
        pattern.add_suffix(&grid);
        pattern.add_suffix(&mm_label);
        let paper_size = adw::ComboRow::builder()
            .title("Paper")
            .subtitle("ISO 216")
            .model(&gtk::StringList::new(&PaperSize::NAMES))
            .build();
        let paper_button = gtk::Button::builder()
            .icon_name("color-select-symbolic")
            .tooltip_text("Choose a page background color")
            .valign(gtk::Align::Center)
            .build();
        paper_button.add_css_class("flat");
        paper_size.add_suffix(&paper_button);
        let night = gtk::Button::builder()
            .label("Night")
            .tooltip_text("Dark paper for low-light sketching")
            .valign(gtk::Align::Center)
            .action_name("win.night-paper")
            .build();
        night.add_css_class("flat");
        night.add_css_class("pill");
        paper_size.add_suffix(&night);
        let view_list = boxed_list("view-list");
        view_list.set_selection_mode(gtk::SelectionMode::None);
        view_list.append(&pattern);
        view_list.append(&paper_size);
        content.append(&view_list);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        content.append(&spacer);
        let hint = gtk::Label::builder()
            .label("Folders in Files are categories. Each .inkstone file is a notebook. F9 hides this sidebar.")
            .xalign(0.0)
            .wrap(true)
            .build();
        hint.add_css_class("dim-label");
        hint.add_css_class("caption");
        content.append(&hint);

        let sidebar = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_width(true)
            .child(&content)
            .build();
        sidebar.add_css_class("workspace-sidebar");

        let navigator = Self {
            sidebar,
            search: search.clone(),
            notebook_list,
            library_filter: library_filter.clone(),
            current_name: current_name.clone(),
            current_category: current_category.clone(),
            add_to_library: add_to_library.clone(),
            library,
            session,
            section_list,
            page_list,
            layer_list,
            trash_list,
            search_hits: search_hits.clone(),
            todo_list: todo_list.clone(),
            tabs: tabs.clone(),
            open_paths: Rc::new(RefCell::new(Vec::new())),
            folder_monitor: Rc::new(RefCell::new(None)),
            pattern: pattern.clone(),
            paper: paper_size.clone(),
            grid: grid.clone(),
            updating,
        };

        navigator.section_list.connect_row_selected({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            move |_, row| {
                if navigator.updating.get() {
                    return;
                }
                if let Some(id) = row.and_then(|row| Uuid::parse_str(&row.widget_name()).ok()) {
                    canvas.set_active_section(id);
                    navigator.refresh(&canvas);
                }
            }
        });
        navigator.page_list.connect_row_selected({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            move |_, row| {
                if navigator.updating.get() {
                    return;
                }
                if let Some(index) = row.and_then(|row| row.widget_name().parse().ok()) {
                    canvas.set_active_page(index);
                    navigator.refresh_after_page_change(&canvas);
                }
            }
        });
        navigator.layer_list.connect_row_selected({
            let canvas = canvas.clone();
            move |_, row| {
                if let Some(index) = row.map(|row| row.index() as usize) {
                    canvas.set_active_layer(index);
                }
            }
        });
        navigator.trash_list.connect_row_activated({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_, row| {
                if let Ok(index) = row.widget_name().parse()
                    && canvas.restore_trashed_page(index)
                {
                    navigator.refresh(&canvas);
                    feedback.show("Page restored");
                }
            }
        });
        previous_page.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                if canvas.cycle_page(-1) {
                    navigator.refresh(&canvas);
                    feedback.whisper("Previous page");
                }
            }
        });
        next_page.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                if canvas.cycle_page(1) {
                    navigator.refresh(&canvas);
                    feedback.whisper("Next page");
                }
            }
        });
        add_section.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                canvas.add_section(format!("Section {}", canvas.sections().len() + 1));
                navigator.refresh(&canvas);
                feedback.show("Section added");
            }
        });
        add_page.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                canvas.add_page();
                navigator.refresh(&canvas);
                feedback.show("Page added");
            }
        });
        add_subpage.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                canvas.add_subpage();
                navigator.refresh(&canvas);
                feedback.show("Subpage added");
            }
        });
        empty_trash.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                canvas.empty_trash();
                navigator.refresh(&canvas);
                feedback.show("Recycle bin emptied");
            }
        });
        remove_page.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                if canvas.remove_active_page() {
                    navigator.refresh(&canvas);
                    feedback.show("Page moved to Recycle bin");
                } else {
                    feedback.show("A notebook needs at least one page");
                }
            }
        });
        add_layer.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                canvas.add_layer();
                navigator.refresh(&canvas);
                feedback.show("Layer added");
            }
        });
        remove_layer.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                if canvas.remove_active_layer() {
                    navigator.refresh(&canvas);
                    feedback.show("Layer removed");
                } else {
                    feedback.show("A page needs at least one layer");
                }
            }
        });
        navigator.pattern.connect_selected_notify({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |row| {
                if navigator.updating.get() {
                    return;
                }
                if let Some(pattern) = BackgroundPattern::ALL.get(row.selected() as usize).copied()
                {
                    canvas.set_pattern(pattern);
                    feedback.whisper(format!(
                        "{} background",
                        BackgroundPattern::NAMES[row.selected() as usize]
                    ));
                }
            }
        });
        navigator.paper.connect_selected_notify({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |row| {
                if navigator.updating.get() {
                    return;
                }
                if let Some(size) = PaperSize::ALL.get(row.selected() as usize).copied() {
                    canvas.set_paper_size(size);
                    feedback.whisper(PaperSize::NAMES[row.selected() as usize]);
                }
            }
        });
        grid.connect_value_changed({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let pattern = pattern.clone();
            move |spin| {
                if navigator.updating.get() {
                    return;
                }
                let mm = spin.value() as f32;
                canvas.set_grid_spacing_mm(mm);
                pattern.set_subtitle(&format!("{mm:.0} mm"));
            }
        });
        paper_button.connect_clicked({
            let canvas = canvas.clone();
            let feedback = feedback.clone();
            move |button| {
                let Some(parent) = button.root().and_downcast::<gtk::Window>() else {
                    return;
                };
                let dialog = gtk::ColorChooserDialog::builder()
                    .title("Page background")
                    .transient_for(&parent)
                    .modal(true)
                    .build();
                dialog.set_use_alpha(false);
                let color = canvas.background_color();
                dialog.set_rgba(&gtk::gdk::RGBA::new(
                    color.red,
                    color.green,
                    color.blue,
                    1.0,
                ));
                dialog.connect_response({
                    let canvas = canvas.clone();
                    let feedback = feedback.clone();
                    move |dialog, response| {
                        if response == gtk::ResponseType::Ok {
                            let rgba = dialog.rgba();
                            canvas.set_background_color(Color::rgb(
                                rgba.red(),
                                rgba.green(),
                                rgba.blue(),
                            ));
                            feedback.whisper("Page background updated");
                        }
                        dialog.close();
                    }
                });
                dialog.present();
            }
        });
        search.connect_activate({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |search| {
                let query = search.text().to_string();
                if let Some(hit) = canvas.find_next(&query) {
                    navigator.refresh(&canvas);
                    navigator.fill_search_hits(&canvas, &query);
                    feedback.show(format!("{} · {}", hit.page_title, hit.snippet));
                    return;
                }
                navigator.fill_search_hits(&canvas, &query);
                if navigator.search_hits.first_child().is_some() {
                    feedback.show("Matches in other notebooks");
                } else {
                    feedback.show("No matching note or label");
                }
            }
        });
        navigator.library_filter.connect_search_changed({
            let navigator = navigator.clone();
            let canvas = canvas.clone();
            move |_| {
                navigator.refresh_library(&canvas);
            }
        });
        navigator.current_name.connect_changed({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |editable| {
                if navigator.updating.get() {
                    return;
                }
                match canvas.rename_current_notebook(&editable.text()) {
                    Ok(path) => {
                        if !path.as_os_str().is_empty() {
                            let _ = navigator.session.borrow_mut().remember(&path);
                        }
                        navigator.refresh_library(&canvas);
                        navigator.refresh_current_notebook(&canvas);
                        feedback.whisper("Notebook renamed");
                    }
                    Err(error) => feedback.show(error.to_string()),
                }
            }
        });
        navigator.notebook_list.connect_row_selected({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_, row| {
                if navigator.updating.get() {
                    return;
                }
                let Some(path) = row.map(|row| PathBuf::from(row.widget_name().as_str())) else {
                    return;
                };
                if path.as_os_str().is_empty() || !path.exists() {
                    return;
                }
                if canvas.current_path().as_deref() == Some(path.as_path()) {
                    return;
                }
                if canvas.is_dirty() && navigator.session.borrow().preferences.save_on_switch {
                    let _ = canvas.save_current();
                }
                match canvas.load(&path) {
                    Ok(()) => {
                        let _ = navigator.session.borrow_mut().remember(&path);
                        navigator.refresh(&canvas);
                        navigator.refresh_library(&canvas);
                        feedback.whisper("Opened notebook");
                    }
                    Err(error) => feedback.show(error.to_string()),
                }
            }
        });
        open_startup_notebook(canvas, &navigator, feedback);
        navigator.refresh_library(canvas);
        navigator.refresh(canvas);
        navigator.watch_library(canvas);
        navigator
    }

    fn refresh(&self, canvas: &Canvas) {
        self.updating.set(true);
        self.refresh_lists(canvas);
        self.sync_page_settings(canvas);
        self.refresh_current_notebook(canvas);
        self.refresh_todos(canvas);
        self.refresh_tabs(canvas);
        self.updating.set(false);
    }

    fn refresh_current_notebook(&self, canvas: &Canvas) {
        let title = canvas
            .current_path()
            .as_deref()
            .map(crate::library::file_stem_title)
            .unwrap_or_else(|| canvas.document_title());
        if self.current_name.text() != title {
            self.current_name.set_text(&title);
        }
        let (category, external) = {
            let library = self.library.borrow();
            let category = canvas
                .current_path()
                .as_deref()
                .map(|path| {
                    if library.contains(path) {
                        library.category_label_for(path)
                    } else {
                        "Opened from Files".to_owned()
                    }
                })
                .unwrap_or_else(|| "Not saved yet".to_owned());
            let external = canvas
                .current_path()
                .as_deref()
                .is_some_and(|path| !library.contains(path));
            (category, external)
        };
        self.current_category.set_label(&category);
        self.add_to_library.set_visible(external);
        self.remember_open(canvas);
    }

    fn remember_open(&self, canvas: &Canvas) {
        let Some(path) = canvas.current_path() else {
            self.refresh_tabs(canvas);
            return;
        };
        {
            let mut open = self.open_paths.borrow_mut();
            if !open.iter().any(|item| item == &path) {
                open.push(path);
            }
        }
        self.refresh_tabs(canvas);
    }

    fn refresh_tabs(&self, canvas: &Canvas) {
        while let Some(child) = self.tabs.first_child() {
            self.tabs.remove(&child);
        }
        let current = canvas.current_path();
        let paths = self.open_paths.borrow().clone();
        for path in paths {
            let title = crate::library::file_stem_title(&path);
            let button = gtk::ToggleButton::builder()
                .label(&title)
                .tooltip_text(path.to_string_lossy().as_ref())
                .active(current.as_deref() == Some(path.as_path()))
                .build();
            button.add_css_class("flat");
            button.add_css_class("tab-chip");
            button.connect_clicked({
                let canvas = canvas.clone();
                let navigator = self.clone();
                let path = path.clone();
                move |button| {
                    if !button.is_active() {
                        return;
                    }
                    if canvas.current_path().as_deref() == Some(path.as_path()) {
                        return;
                    }
                    if canvas.is_dirty() && navigator.session.borrow().preferences.save_on_switch {
                        let _ = canvas.save_current();
                    }
                    if canvas.load(&path).is_ok() {
                        let _ = navigator.session.borrow_mut().remember(&path);
                        navigator.refresh(&canvas);
                        navigator.refresh_library(&canvas);
                    }
                }
            });
            self.tabs.append(&button);
        }
    }

    fn refresh_todos(&self, canvas: &Canvas) {
        while let Some(child) = self.todo_list.first_child() {
            self.todo_list.remove(&child);
        }
        for item in canvas.todos() {
            let mark = if item.checked { "☑" } else { "☐" };
            let (row, _) =
                editable_nav_row(&format!("{mark} {}", item.text), Some(&item.page_title));
            row.set_widget_name(&item.element_id.to_string());
            let page_index = item.page_index;
            row.set_activatable(true);
            self.todo_list.append(&row);
            row.connect_activated({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |_| {
                    canvas.set_active_page(page_index);
                    navigator.refresh(&canvas);
                }
            });
        }
    }

    fn fill_search_hits(&self, canvas: &Canvas, query: &str) {
        while let Some(child) = self.search_hits.first_child() {
            self.search_hits.remove(&child);
        }
        let hits = self.library.borrow().search_limited(
            query,
            self.session.borrow().preferences.search_limit as usize,
        );
        let current = canvas.current_path();
        for hit in hits {
            if current.as_deref() == Some(hit.path.as_path()) {
                continue;
            }
            let (row, _) = editable_nav_row(
                &format!("{} · {}", hit.notebook_title, hit.page_title),
                Some(&hit.snippet),
            );
            row.set_activatable(true);
            let path = hit.path.clone();
            let page_index = hit.page_index;
            row.connect_activated({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |_| {
                    if canvas.is_dirty() && navigator.session.borrow().preferences.save_on_switch {
                        let _ = canvas.save_current();
                    }
                    if canvas.load(&path).is_ok() {
                        canvas.set_active_page(page_index);
                        let _ = navigator.session.borrow_mut().remember(&path);
                        navigator.refresh(&canvas);
                        navigator.refresh_library(&canvas);
                    }
                }
            });
            self.search_hits.append(&row);
        }
    }

    fn watch_library(&self, canvas: &Canvas) {
        if !self.session.borrow().preferences.watch_library {
            *self.folder_monitor.borrow_mut() = None;
            return;
        }
        let root = self.library.borrow().root.clone();
        let file = gio::File::for_path(&root);
        let Ok(monitor) =
            file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        else {
            return;
        };
        monitor.connect_changed({
            let navigator = self.clone();
            let canvas = canvas.clone();
            move |_, _, _, _| {
                if navigator.updating.get() {
                    return;
                }
                navigator.refresh_library(&canvas);
            }
        });
        *self.folder_monitor.borrow_mut() = Some(monitor);
    }

    fn refresh_library(&self, canvas: &Canvas) {
        if let Err(error) = self.library.borrow_mut().rescan() {
            self.current_category.set_label(&error.to_string());
            return;
        }
        self.updating.set(true);
        while let Some(child) = self.notebook_list.first_child() {
            self.notebook_list.remove(&child);
        }
        let filter = self.library_filter.text().to_ascii_lowercase();
        let current = canvas.current_path();
        let library = self.library.borrow();
        append_root_notebooks(
            &self.notebook_list,
            &library.notebooks,
            &filter,
            current.as_deref(),
            0,
            self,
            canvas,
        );
        append_categories(
