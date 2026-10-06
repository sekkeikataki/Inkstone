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

#[derive(Clone)]
struct AppState {
    window: adw::ApplicationWindow,
    canvas: Canvas,
    navigator: Navigator,
    feedback: Feedback,
}

pub fn run() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    run_with_args(&args)
}

pub fn run_with_args<S: AsRef<str>>(args: &[S]) -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let app_state: Rc<RefCell<Option<AppState>>> = Rc::new(RefCell::new(None));

    application.connect_activate({
        let app_state = app_state.clone();
        move |application| {
            if let Some(state) = app_state.borrow().as_ref() {
                state.window.present();
            } else {
                let state = build_window(application, None);
                *app_state.borrow_mut() = Some(state);
            }
        }
    });

    application.connect_open({
        let app_state = app_state.clone();
        move |application, files, _hint| {
            let first_file = files.first().and_then(|f| f.path());
            if app_state.borrow().is_none() {
                let state = build_window(application, first_file.as_deref());
                for file in files.iter().skip(1) {
                    if let Some(p) = file.path() {
                        let mut open = state.navigator.open_paths.borrow_mut();
                        if !open.iter().any(|item| item == &p) {
                            open.push(p);
                        }
                    }
                }
                state.navigator.refresh_tabs(&state.canvas);
                *app_state.borrow_mut() = Some(state);
            } else if let Some(state) = app_state.borrow().as_ref() {
                for file in files {
                    if let Some(path) = file.path() {
                        if state.canvas.is_dirty() && state.navigator.session.borrow().preferences.save_on_switch {
                            let _ = state.canvas.save_current();
                        }
                        if let Err(error) = state.canvas.load(&path) {
                            state.feedback.show(error.to_string());
                        } else {
                            let _ = state.navigator.session.borrow_mut().remember(&path);
                            state.navigator.refresh(&state.canvas);
                            state.navigator.refresh_library(&state.canvas);
                            state.feedback.whisper("Opened notebook");
                        }
                    }
                }
                state.window.present();
            }
        }
    });

    application.run_with_args(args)
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
        .tool-options {
            border-radius: 22px;
            padding: 4px 10px;
            min-height: 40px;
        }
        .tool-options button.pill {
            min-height: 26px;
            padding: 0 9px;
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
            background-color: alpha(@window_bg_color, 0.94);
            border-radius: 14px;
            box-shadow: 0 4px 18px alpha(#000, 0.16);
            border: 1px solid alpha(@window_fg_color, 0.12);
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
        .sheet-address {
            min-width: 4.2em;
            font-weight: 700;
            font-family: monospace;
        }
        .sheet-formula {
            min-width: 10em;
        }
        .sheet-chip {
            min-width: 28px;
            min-height: 28px;
            padding: 0 4px;
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
            border-color: @window_fg_color;
            color: @window_fg_color;
        }
        .section-dot {
            border-radius: 999px;
            margin-right: 6px;
        }
        .section-tab {
            padding: 4px 10px;
            border-radius: 999px;
            font-weight: 600;
        }
        .category-row {
            padding: 6px 10px 2px;
        }
        .category-label {
            letter-spacing: 0.08em;
            font-size: 0.76em;
        }
        .todo-item {
            padding: 2px 6px;
            border-radius: 8px;
        }
        .todo-list {
            margin-bottom: 8px;
        }
        .active-row {
            font-weight: 700;
        }
        .measure-chip {
            font-weight: 600;
        }
        .option-icon {
            min-width: 28px;
            min-height: 28px;
            padding: 0;
        }
        .restore-island {
            margin: 6px;
        }
        .restore-island image {
            margin-right: 4px;
        }
        .island-drag {
            padding: 2px 4px;
            color: alpha(@window_fg_color, 0.45);
        }
        .island-drag:hover {
            color: @window_fg_color;
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

fn build_window(application: &adw::Application, initial_path: Option<&Path>) -> AppState {
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

    let navigator = Navigator::new(&canvas, &feedback, initial_path);
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
    AppState {
        window,
        canvas,
        navigator,
        feedback,
    }
}

impl Navigator {
    fn new(canvas: &Canvas, feedback: &Feedback, initial_path: Option<&Path>) -> Self {
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
            .placeholder_text("Search notes, labels, cells, and library")
            .tooltip_text(
                "Press Enter to jump. Matches notes, labels, spreadsheet cells, and other notebooks in Files.",
            )
            .build();
        search.add_css_class("sidebar-search");
        content.append(&search);
        let search_hits = boxed_list("search-hits");
        search_hits.add_css_class("search-hits");
        content.append(&search_hits);

        let add_section = circular_icon_button("list-add-symbolic", "Add section");
        content.append(&section_header("Sections", Some(add_section.upcast_ref())));
        content.append(&section_list);

        let add_page = circular_icon_button("list-add-symbolic", "Add page");
        content.append(&section_header("Pages", Some(add_page.upcast_ref())));
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

        let add_layer = gtk::MenuButton::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add a notes or spreadsheet layer")
            .build();
        add_layer.add_css_class("flat");
        add_layer.add_css_class("circular");
        add_layer.add_css_class("circular-icon");
        let layer_menu = gio::Menu::new();
        layer_menu.append(Some("Notes layer"), Some("win.add-notes-layer"));
        layer_menu.append(Some("Spreadsheet layer"), Some("win.add-spreadsheet-layer"));
        add_layer.set_menu_model(Some(&layer_menu));
        content.append(&section_header("Layers", Some(add_layer.upcast_ref())));
        content.append(&layer_list);
        let remove_layer = circular_icon_button("list-remove-symbolic", "Remove layer");
        remove_layer.set_halign(gtk::Align::End);
        content.append(&remove_layer);

        let empty_trash = circular_icon_button("edit-clear-all-symbolic", "Empty recycle bin");
        empty_trash.add_css_class("trash-empty");
        content.append(&section_header("Recycle bin", Some(empty_trash.upcast_ref())));
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
        let add_notes = gtk::Button::builder().label("Notes layer").build();
        add_notes.add_css_class("flat");
        let add_sheet = gtk::Button::builder().label("Spreadsheet layer").build();
        add_sheet.add_css_class("flat");
        let add_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        add_box.append(&add_notes);
        add_box.append(&add_sheet);
        let add_popover = gtk::Popover::builder().child(&add_box).build();
        add_layer.set_popover(Some(&add_popover));
        add_notes.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            let add_popover = add_popover.clone();
            move |_| {
                add_popover.popdown();
                canvas.add_layer();
                navigator.refresh(&canvas);
                feedback.show("Notes layer added");
            }
        });
        add_sheet.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            let add_popover = add_popover.clone();
            move |_| {
                add_popover.popdown();
                canvas.add_spreadsheet_layer();
                navigator.refresh(&canvas);
                feedback.show("Spreadsheet layer added");
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
                if path.as_os_str().is_empty() || !path.is_file() {
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
        if let Some(path) = initial_path {
            if let Err(error) = canvas.load(path) {
                feedback.whisper(error.to_string());
            } else {
                let _ = navigator.session.borrow_mut().remember(path);
            }
        } else {
            open_startup_notebook(canvas, &navigator, feedback);
        }
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
            &self.notebook_list,
            &library.categories,
            &filter,
            current.as_deref(),
            self,
            canvas,
        );
        drop(library);
        self.refresh_current_notebook(canvas);
        self.updating.set(false);
    }

    fn refresh_after_page_change(&self, canvas: &Canvas) {
        self.updating.set(true);
        self.refresh_layer_list(canvas);
        self.sync_page_settings(canvas);
        self.updating.set(false);
    }

    fn sync_page_settings(&self, canvas: &Canvas) {
        let pattern = canvas.pattern();
        let pattern_index = BackgroundPattern::ALL
            .iter()
            .position(|value| *value == pattern)
            .unwrap_or(1) as u32;
        self.pattern.set_selected(pattern_index);
        let paper = canvas.paper_size();
        let paper_index = PaperSize::ALL
            .iter()
            .position(|value| *value == paper)
            .unwrap_or(2) as u32;
        self.paper.set_selected(paper_index);
        let mm = canvas.grid_spacing_mm();
        self.grid.set_value(mm as f64);
        self.pattern.set_subtitle(&format!("{mm:.0} mm"));
    }

    fn refresh_lists(&self, canvas: &Canvas) {
        let updating = self.updating.get();
        self.updating.set(true);
        self.refresh_section_list(canvas);
        self.refresh_page_list(canvas);
        self.refresh_layer_list(canvas);
        self.refresh_trash_list(canvas);
        self.updating.set(updating);
    }

    fn refresh_section_list(&self, canvas: &Canvas) {
        while let Some(child) = self.section_list.first_child() {
            self.section_list.remove(&child);
        }
        let active = canvas.active_section_id();
        for (id, name, color) in canvas.sections() {
            let (row, title) = editable_nav_row(&name, None);
            row.set_widget_name(&id.to_string());
            let dot = gtk::DrawingArea::builder()
                .content_width(10)
                .content_height(10)
                .valign(gtk::Align::Center)
                .build();
            dot.add_css_class("section-dot");
            dot.set_draw_func(move |_, context, width, height| {
                context.set_source_rgb(color.red as f64, color.green as f64, color.blue as f64);
                context.arc(
                    f64::from(width) / 2.0,
                    f64::from(height) / 2.0,
                    4.0,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = context.fill();
            });
            row.add_prefix(&dot);
            title.connect_changed({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |editable| {
                    if navigator.updating.get() {
                        return;
                    }
                    canvas.set_active_section(id);
                    canvas.rename_active_section(editable.text().to_string());
                }
            });
            self.section_list.append(&row);
            if id == active {
                self.section_list.select_row(Some(&row));
            }
        }
    }

    fn refresh_page_list(&self, canvas: &Canvas) {
        while let Some(child) = self.page_list.first_child() {
            self.page_list.remove(&child);
        }
        let active_page = canvas.active_page_index();
        let active_section = canvas.active_section_id();
        for (index, title, count, level, section_id) in canvas.page_summaries() {
            if section_id != active_section {
                continue;
            }
            let subtitle = if count == 0 {
                "Empty page".to_owned()
            } else {
                format!("{count} objects")
            };
            let (row, name) = editable_nav_row(&title, Some(subtitle.as_str()));
            row.set_widget_name(&index.to_string());
            if level > 0 {
                row.add_css_class("subpage-row");
            }
            let thumb = gtk::DrawingArea::builder()
                .content_width(36)
                .content_height(48)
                .valign(gtk::Align::Center)
                .build();
            thumb.add_css_class("page-thumb");
            if let Some(pixbuf) = canvas.render_page_thumbnail(index, 36, 48) {
                thumb.set_draw_func(move |_, context, width, height| {
                    context.set_source_pixbuf(
                        &pixbuf,
                        (f64::from(width) - f64::from(pixbuf.width())) / 2.0,
                        (f64::from(height) - f64::from(pixbuf.height())) / 2.0,
                    );
                    let _ = context.paint();
                });
            }
            row.add_prefix(&thumb);
            name.connect_changed({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |editable| {
                    if navigator.updating.get() {
                        return;
                    }
                    canvas.set_active_page(index);
                    canvas.rename_active_page(editable.text().to_string());
                }
            });
            self.page_list.append(&row);
            if index == active_page {
                self.page_list.select_row(Some(&row));
            }
        }
    }

    fn refresh_trash_list(&self, canvas: &Canvas) {
        while let Some(child) = self.trash_list.first_child() {
            self.trash_list.remove(&child);
        }
        for (index, title) in canvas.trash_titles().into_iter().enumerate() {
            let (row, _) = editable_nav_row(&title, Some("Click to restore"));
            row.set_widget_name(&index.to_string());
            row.set_activatable(true);
            self.trash_list.append(&row);
        }
    }

    fn refresh_layer_list(&self, canvas: &Canvas) {
        while let Some(child) = self.layer_list.first_child() {
            self.layer_list.remove(&child);
        }
        let active_layer = canvas.active_layer_index();
        for (index, (title, visible, locked, kind)) in
            canvas.layer_summaries().into_iter().enumerate()
        {
            let (row, name) = editable_nav_row(&title, None);
            row.set_tooltip_text(Some(kind.label()));
            let vis = layer_icon_toggle("view-reveal-symbolic", "Show or hide layer", visible);
            let lock = layer_icon_toggle("changes-prevent-symbolic", "Lock layer", locked);
            row.add_suffix(&lock);
            row.add_suffix(&vis);
            name.connect_changed({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |editable| {
                    if navigator.updating.get() {
                        return;
                    }
                    canvas.set_active_layer(index);
                    canvas.rename_active_layer(editable.text().to_string());
                }
            });
            vis.connect_clicked({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |button| {
                    if navigator.updating.get() {
                        return;
                    }
                    canvas.set_active_layer(index);
                    if canvas.active_layer_visible() != button.is_active() {
                        canvas.toggle_active_layer_visibility();
                    }
                    navigator.updating.set(true);
                    navigator.refresh_layer_list(&canvas);
                    navigator.updating.set(false);
                }
            });
            lock.connect_clicked({
                let canvas = canvas.clone();
                let navigator = self.clone();
                move |button| {
                    if navigator.updating.get() {
                        return;
                    }
                    canvas.set_active_layer(index);
                    if canvas.active_layer_locked() != button.is_active() {
                        canvas.toggle_active_layer_lock();
                    }
                    navigator.updating.set(true);
                    navigator.refresh_layer_list(&canvas);
                    navigator.updating.set(false);
                }
            });
            self.layer_list.append(&row);
            if index == active_layer {
                self.layer_list.select_row(Some(&row));
            }
        }
    }
}

fn open_startup_notebook(canvas: &Canvas, navigator: &Navigator, feedback: &Feedback) {
    let restore = navigator.session.borrow().preferences.restore_last_notebook;
    let session_path = restore
        .then(|| navigator.session.borrow().last_notebook.clone())
        .flatten()
        .filter(|path| path.exists());
    let first = navigator
        .library
        .borrow()
        .first_notebook()
        .map(|notebook| notebook.path.clone());
    let path = session_path.or(first);
    let Some(path) = path else {
        return;
    };
    if let Err(error) = canvas.load(&path) {
        feedback.whisper(error.to_string());
        return;
    }
    let _ = navigator.session.borrow_mut().remember(&path);
}

fn library_filter_match(title: &str, filter: &str) -> bool {
    filter.is_empty() || title.to_ascii_lowercase().contains(filter)
}

fn category_has_match(category: &crate::library::Category, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    if category.name.to_ascii_lowercase().contains(filter) {
        return true;
    }
    category
        .notebooks
        .iter()
        .any(|notebook| library_filter_match(&notebook.title, filter))
        || category
            .categories
            .iter()
            .any(|child| category_has_match(child, filter))
}

fn append_categories(
    list: &gtk::ListBox,
    categories: &[crate::library::Category],
    filter: &str,
    current: Option<&Path>,
    navigator: &Navigator,
    canvas: &Canvas,
) {
    for category in categories {
        if !category_has_match(category, filter) {
            continue;
        }
        let header = gtk::ListBoxRow::builder()
            .selectable(false)
            .activatable(false)
            .build();
        header.add_css_class("category-row");
        header.set_widget_name(&category.path.to_string_lossy());
        let label = gtk::Label::builder()
            .label(
                category
                    .relative
                    .iter()
                    .filter_map(|part| part.to_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
                    .to_uppercase(),
            )
            .xalign(0.0)
            .build();
        label.add_css_class("dim-label");
        label.add_css_class("caption-heading");
        label.add_css_class("category-label");
        header.set_child(Some(&label));
        let drop = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::MOVE);
        drop.connect_drop({
            let navigator = navigator.clone();
            let canvas = canvas.clone();
            let folder = category.path.clone();
            move |_, value, _, _| {
                let Ok(text) = value.get::<String>() else {
                    return false;
                };
                let from = PathBuf::from(text);
                match navigator.library.borrow().move_notebook(&from, &folder) {
                    Ok(destination) => {
                        if canvas.current_path().as_deref() == Some(from.as_path()) {
                            let _ = canvas.load(&destination);
                            let _ = navigator.session.borrow_mut().remember(&destination);
                        }
                        navigator.refresh_library(&canvas);
                        navigator.refresh(&canvas);
                        true
                    }
                    Err(_) => false,
                }
            }
        });
        header.add_controller(drop);
        list.append(&header);
        append_root_notebooks(
            list,
            &category.notebooks,
            filter,
            current,
            1,
            navigator,
            canvas,
        );
        append_categories(
            list,
            &category.categories,
            filter,
            current,
            navigator,
            canvas,
        );
    }
}

fn append_root_notebooks(
    list: &gtk::ListBox,
    notebooks: &[crate::library::NotebookFile],
    filter: &str,
    current: Option<&Path>,
    indent: i32,
    navigator: &Navigator,
    canvas: &Canvas,
) {
    for notebook in notebooks {
        if !library_filter_match(&notebook.title, filter) {
            continue;
        }
        let (row, title) = editable_nav_row(&notebook.title, None);
        title.set_editable(false);
        let path_name = notebook.path.to_string_lossy();
        row.set_widget_name(path_name.as_ref());
        row.set_margin_start(indent * 8);
        let is_current = current == Some(notebook.path.as_path());
        let color = if is_current {
            canvas
                .notebook_color()
                .unwrap_or(crate::library::NOTEBOOK_SWATCHES[notebook_color_index(&notebook.path)])
        } else {
            crate::library::NOTEBOOK_SWATCHES[notebook_color_index(&notebook.path)]
        };
        let dot = gtk::DrawingArea::builder()
            .content_width(10)
            .content_height(10)
            .valign(gtk::Align::Center)
            .tooltip_text("Notebook colour. Click the current notebook’s dot to cycle colours.")
            .build();
        dot.add_css_class("section-dot");
        dot.set_draw_func(move |_, context, width, height| {
            context.set_source_rgb(color.red as f64, color.green as f64, color.blue as f64);
            context.arc(
                f64::from(width) / 2.0,
                f64::from(height) / 2.0,
                4.0,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.fill();
        });
        if is_current {
            let click = gtk::GestureClick::new();
            click.connect_released({
                let canvas = canvas.clone();
                let navigator = navigator.clone();
                move |_, _, _, _| {
                    canvas.cycle_notebook_color();
                    navigator.refresh_library(&canvas);
                }
            });
            dot.add_controller(click);
        }
        row.add_prefix(&dot);
        let drag = gtk::DragSource::new();
        drag.set_actions(gtk::gdk::DragAction::MOVE);
        let path = notebook.path.clone();
        drag.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(&glib::Value::from(
                path.to_string_lossy().to_string(),
            )))
        });
        row.add_controller(drag);
        list.append(&row);
        if is_current {
            list.select_row(Some(&row));
        }
    }
}

fn section_header(text: &str, action: Option<&gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    row.add_css_class("section-header");
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .hexpand(true)
        .build();
    label.add_css_class("caption-heading");
    row.append(&label);
    if let Some(action) = action {
        row.append(action);
    }
    row
}

fn circular_icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("circular-icon");
    button
}

fn layer_icon_toggle(icon: &str, tooltip: &str, active: bool) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .active(active)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button
}

fn boxed_list(class: &str) -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .show_separators(false)
        .hexpand(true)
        .build();
    list.add_css_class("boxed-list");
    list.add_css_class("nav-list");
    list.add_css_class(class);
    list
}

fn editable_nav_row(title: &str, subtitle: Option<&str>) -> (adw::ActionRow, gtk::EditableLabel) {
    let row = adw::ActionRow::builder().activatable(true).build();
    if let Some(subtitle) = subtitle {
        row.set_subtitle(subtitle);
    }
    row.add_css_class("nav-row");
    let name = gtk::EditableLabel::builder()
        .text(title)
        .xalign(0.0)
        .hexpand(true)
        .tooltip_text("Click the name to rename")
        .build();
    name.add_css_class("nav-title");
    row.add_prefix(&name);
    (row, name)
}

fn build_header(
    window: &adw::ApplicationWindow,
    title: &adw::WindowTitle,
) -> (
    adw::HeaderBar,
    gtk::ToggleButton,
    gtk::ToggleButton,
    gtk::ToggleButton,
) {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(title));

    let sidebar_content = adw::ButtonContent::builder()
        .icon_name("sidebar-show-symbolic")
        .label("Notebook")
        .build();
    let sidebar = gtk::ToggleButton::builder()
        .child(&sidebar_content)
        .tooltip_text("Sidebar (F9)")
        .action_name("win.toggle-sidebar")
        .active(true)
        .build();
    sidebar.add_css_class("flat");
    sidebar.add_css_class("pill");
    let tools_content = adw::ButtonContent::builder()
        .icon_name("view-conceal-symbolic")
        .label("Tools")
        .build();
    let tools = gtk::ToggleButton::builder()
        .child(&tools_content)
        .tooltip_text("Tools (F10)")
        .active(true)
        .build();
    tools.add_css_class("flat");
    tools.add_css_class("pill");
    let open = gtk::Button::builder()
        .icon_name("document-open-symbolic")
        .tooltip_text("Open document (Ctrl+O)")
        .action_name("win.open")
        .build();
    let save = gtk::Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save document (Ctrl+S)")
        .action_name("win.save")
        .build();
    let nav = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .valign(gtk::Align::Center)
        .build();
    nav.append(&sidebar);
    nav.append(&tools);
    let palettes = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .valign(gtk::Align::Center)
        .build();
    palettes.add_css_class("linked");
    let colors = header_palette_toggle(
        "Colors",
        "color-select-symbolic",
        "Show or hide the color palette",
    );
    let widths = header_palette_toggle(
        "Widths",
        "format-text-size-symbolic",
        "Show or hide stroke widths",
    );
    palettes.append(&colors);
    palettes.append(&widths);
    nav.append(&palettes);
    let files = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .valign(gtk::Align::Center)
        .build();
    files.add_css_class("linked");
    for button in [&open, &save] {
        button.add_css_class("flat");
        button.add_css_class("header-file");
        files.append(button);
    }
    header.pack_start(&nav);
    header.pack_start(&files);

    let insert = gtk::MenuButton::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Insert")
        .menu_model(&build_insert_menu())
        .build();
    insert.add_css_class("flat");
    insert.add_css_class("insert-button");
    header.pack_start(&insert);

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .menu_model(&build_menu())
        .build();
    let undo = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .tooltip_text("Undo (Ctrl+Z)")
        .action_name("win.undo")
        .build();
    let redo = gtk::Button::builder()
        .icon_name("edit-redo-symbolic")
        .tooltip_text("Redo (Ctrl+Shift+Z)")
        .action_name("win.redo")
        .build();
    header.pack_end(&menu_button);
    header.pack_end(&redo);
    header.pack_end(&undo);
    let settings = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings (Ctrl+,)")
        .action_name("win.settings")
        .build();
    settings.add_css_class("flat");
    settings.add_css_class("header-file");
    header.pack_end(&settings);
    undo.add_css_class("flat");
    undo.add_css_class("header-file");
    redo.add_css_class("flat");
    redo.add_css_class("header-file");
    menu_button.add_css_class("flat");
    menu_button.add_css_class("header-file");
    window.set_title(Some("Inkstone — Untitled note"));
    (header, tools, colors, widths)
}

fn header_palette_toggle(label: &str, icon: &str, tooltip: &str) -> gtk::ToggleButton {
    let content = adw::ButtonContent::builder()
        .icon_name(icon)
        .label(label)
        .build();
    let button = gtk::ToggleButton::builder()
        .child(&content)
        .tooltip_text(tooltip)
        .active(true)
        .build();
    button.add_css_class("flat");
    button.add_css_class("pill");
    button
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("New Notebook"), Some("win.new"));
    file.append(Some("New Category Folder"), Some("win.new-category"));
    file.append(Some("Open from Files…"), Some("win.open"));
    file.append(Some("Show Library in Files"), Some("win.show-library"));
    file.append(Some("Keep in Library"), Some("win.add-to-library"));
    file.append(Some("Move to Category…"), Some("win.move-notebook"));
    file.append(Some("Save"), Some("win.save"));
    file.append(Some("Save As…"), Some("win.save-as"));
    file.append(
        Some("Import Image, PDF, SVG, or File…"),
        Some("win.import-media"),
    );
    file.append(Some("Export SVG…"), Some("win.export-svg"));
    file.append(Some("Export PDF…"), Some("win.export-pdf"));
    file.append(Some("Export PNG…"), Some("win.export-png"));
    file.append(Some("Export JPEG…"), Some("win.export-jpeg"));
    file.append(Some("Export notebook folder…"), Some("win.export-folder"));
    file.append(Some("Export layers…"), Some("win.export-layers"));
    file.append(Some("Print…"), Some("win.print"));
    menu.append_section(None, &file);

    let edit = gio::Menu::new();
    edit.append(Some("Undo"), Some("win.undo"));
    edit.append(Some("Redo"), Some("win.redo"));
    edit.append(Some("Cut"), Some("win.cut"));
    edit.append(Some("Copy"), Some("win.copy"));
    edit.append(Some("Copy Selection as SVG"), Some("win.copy-svg"));
    edit.append(Some("Copy Selection as PNG"), Some("win.copy-png"));
    edit.append(Some("Paste"), Some("win.paste"));
    edit.append(Some("Duplicate Selection"), Some("win.duplicate"));
    edit.append(Some("Align Left"), Some("win.align-left"));
    edit.append(Some("Align Centre"), Some("win.align-center"));
    edit.append(Some("Align Right"), Some("win.align-right"));
    edit.append(Some("Distribute Horizontally"), Some("win.distribute-x"));
    edit.append(Some("Same Width"), Some("win.same-width"));
    edit.append(Some("Bring to Front"), Some("win.bring-front"));
    edit.append(Some("Send to Back"), Some("win.send-back"));
    edit.append(Some("Rotate 90°"), Some("win.rotate"));
    edit.append(Some("Delete Selection"), Some("win.delete"));
    menu.append_section(None, &edit);

    let view = gio::Menu::new();
    view.append(Some("Fullscreen"), Some("win.fullscreen"));
    view.append(Some("Find in Notebook"), Some("win.focus-search"));
    view.append(Some("Night Paper"), Some("win.night-paper"));
    view.append(Some("Replay Ink on This Page"), Some("win.replay"));
    view.append(Some("Notebook Sidebar"), Some("win.toggle-sidebar"));
    view.append(Some("Drawing Tools"), Some("win.toggle-chrome"));
    view.append(Some("Colors"), Some("win.show-colors"));
    view.append(Some("Widths"), Some("win.show-widths"));
    view.append(Some("Previous Page"), Some("win.previous-page"));
    view.append(Some("Next Page"), Some("win.next-page"));
    view.append(Some("Zoom In"), Some("win.zoom-in"));
    view.append(Some("Zoom Out"), Some("win.zoom-out"));
    view.append(Some("Reset View"), Some("win.reset-view"));
    menu.append_section(None, &view);

    let insert = gio::Menu::new();
    insert.append(Some("Table"), Some("win.insert-table"));
    insert.append(Some("Date and Time"), Some("win.insert-date"));
    insert.append(Some("Copy Link to Page"), Some("win.copy-page-link"));
    insert.append(Some("Insert [[Page]] Link"), Some("win.insert-page-link"));
    insert.append(Some("File or Audio…"), Some("win.import-media"));
    insert.append(Some("PDF as ink pages…"), Some("win.import-pdf-pages"));
    insert.append(Some("Calculate 2+2="), Some("win.calculate"));
    insert.append(Some("Open Attachment"), Some("win.open-attachment"));
    insert.append(Some("Delete Section"), Some("win.delete-section"));
    menu.append_section(Some("Insert"), &insert);

    let about = gio::Menu::new();
    about.append(Some("Settings"), Some("win.settings"));
    about.append(Some("About Inkstone"), Some("win.about"));
    menu.append_section(None, &about);
    menu
}

fn build_insert_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Table"), Some("win.insert-table"));
    menu.append(Some("Date and Time"), Some("win.insert-date"));
    menu.append(Some("Copy link to this page"), Some("win.copy-page-link"));
    menu.append(Some("Insert [[Page]] link"), Some("win.insert-page-link"));
    menu.append(Some("File or Audio…"), Some("win.import-media"));
    menu.append(Some("Calculate selected 2+2="), Some("win.calculate"));
    menu.append(Some("Open Attachment"), Some("win.open-attachment"));

    let tags = gio::Menu::new();
    for (index, name) in TagKind::NAMES.iter().enumerate() {
        let item = gio::MenuItem::new(Some(*name), None);
        item.set_action_and_target_value(
            Some("win.insert-tag"),
            Some(&(index as u32).to_variant()),
        );
        tags.append_item(&item);
    }
    menu.append_submenu(Some("Tag"), &tags);

    let templates = gio::Menu::new();
    for (index, name) in PageTemplate::NAMES.iter().enumerate() {
        let item = gio::MenuItem::new(Some(*name), None);
        item.set_action_and_target_value(
            Some("win.apply-template"),
            Some(&(index as u32).to_variant()),
        );
        templates.append_item(&item);
    }
    menu.append_submenu(Some("Page Template"), &templates);
    menu
}

fn chrome_revealer(
    child: &impl IsA<gtk::Widget>,
    transition: gtk::RevealerTransitionType,
    halign: gtk::Align,
    valign: gtk::Align,
    revealed: bool,
) -> gtk::Revealer {
    let revealer = gtk::Revealer::builder()
        .transition_type(transition)
        .transition_duration(0)
        .reveal_child(revealed)
        .can_target(revealed)
        .halign(halign)
        .valign(valign)
        .child(child)
        .build();
    revealer.connect_reveal_child_notify(|revealer| {
        revealer.set_can_target(revealer.reveals_child());
    });
    revealer
}

fn wire_chrome_toggle(
    window: &adw::ApplicationWindow,
    toolbar: &adw::ToolbarView,
    chrome: &WorkspaceChrome,
    header_button: &gtk::ToggleButton,
    colors_toggle: &gtk::ToggleButton,
    widths_toggle: &gtk::ToggleButton,
) {
    let updating = Rc::new(Cell::new(false));
    let apply = {
        let toolbar = toolbar.clone();
        let chrome = chrome.clone();
        let header_button = header_button.clone();
        let updating = updating.clone();
        move |show: bool| {
            if updating.get() {
                return;
            }
            updating.set(true);
            header_button.set_active(show);
            toolbar.set_reveal_top_bars(show);
            chrome.tools.set_reveal_child(show);
            chrome.options.set_reveal_child(show);
            chrome
                .colors
                .set_reveal_child(show && chrome.colors_wanted.get());
            chrome
                .widths
                .set_reveal_child(show && chrome.widths_wanted.get());
            chrome
                .restore_colors
                .set_reveal_child(show && !chrome.colors_wanted.get());
            chrome
                .restore_widths
                .set_reveal_child(show && !chrome.widths_wanted.get());
            chrome.status.set_reveal_child(show);
            chrome.zoom.set_reveal_child(show);
            chrome.restore.set_reveal_child(!show);
            chrome.chrome_visible.set(show);
            updating.set(false);
        }
    };

    header_button.connect_toggled({
        let apply = apply.clone();
        move |button| apply(button.is_active())
    });
    chrome.restore_button.connect_clicked({
        let apply = apply.clone();
        move |_| apply(true)
    });
    add_action(window, "toggle-chrome", {
        let header_button = header_button.clone();
        move || header_button.set_active(!header_button.is_active())
    });
    window.add_action(&chrome.show_colors);
    window.add_action(&chrome.show_widths);
    bind_palette_toggle(colors_toggle, &chrome.show_colors);
    bind_palette_toggle(widths_toggle, &chrome.show_widths);
}

fn bind_palette_toggle(button: &gtk::ToggleButton, action: &gio::SimpleAction) {
    let updating = Rc::new(Cell::new(false));
    button.connect_toggled({
        let action = action.clone();
        let updating = updating.clone();
        move |button| {
            if updating.get() {
                return;
            }
            action.change_state(&button.is_active().to_variant());
        }
    });
    action.connect_notify_local(Some("state"), {
        let button = button.clone();
        let action = action.clone();
        let updating = updating.clone();
        move |_, _| {
            let show = action
                .state()
                .and_then(|value| value.get::<bool>())
                .unwrap_or(true);
            if button.is_active() == show {
                return;
            }
            updating.set(true);
            button.set_active(show);
            updating.set(false);
        }
    });
}

#[derive(Clone)]
struct WorkspaceChrome {
    tools: gtk::Revealer,
    options: gtk::Revealer,
    colors: gtk::Revealer,
    widths: gtk::Revealer,
    colors_wanted: Rc<Cell<bool>>,
    widths_wanted: Rc<Cell<bool>>,
    show_colors: gio::SimpleAction,
    show_widths: gio::SimpleAction,
    chrome_visible: Rc<Cell<bool>>,
    status: gtk::Revealer,
    zoom: gtk::Revealer,
    restore: gtk::Revealer,
    restore_button: gtk::Button,
    restore_colors: gtk::Revealer,
    restore_widths: gtk::Revealer,
}

fn build_canvas_workspace(
    canvas: &Canvas,
    feedback: &Feedback,
    session: &Rc<RefCell<Session>>,
) -> (gtk::Overlay, WorkspaceChrome) {
    canvas.widget().add_css_class("canvas-surface");
    let workspace = gtk::Overlay::new();
    workspace.set_child(Some(canvas.widget()));

    let options_page = Rc::new(RefCell::new(String::from("ink")));
    let chrome_visible = Rc::new(Cell::new(true));
    let colors_wanted = Rc::new(Cell::new(true));
    let widths_wanted = Rc::new(Cell::new(true));

    let options = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::None)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .hexpand(false)
        .build();
    options.add_named(&ink_options(canvas, session), Some("ink"));
    options.add_named(&select_options(), Some("select"));
    options.add_named(&text_options(canvas), Some("text"));
    options.add_named(&diagram_options(canvas), Some("diagram"));
    options.add_named(
        &hint_options("Drag over ink to split the stroke. Other objects erase whole."),
        Some("eraser"),
    );
    options.add_named(
        &hint_options("Drag the canvas · middle mouse always pans"),
        Some("pan"),
    );
    let (sheet_options, sheet_addr, sheet_formula) = spreadsheet_options(canvas);
    options.add_named(&sheet_options, Some("spreadsheet"));
    options.add_named(
        &hint_options("Drag vertically to insert or remove space"),
        Some("space"),
    );
    options.add_named(
        &hint_options("Drag to measure millimetres · Shift snaps to 15°"),
        Some("measure"),
    );
    options.set_visible_child_name("ink");

    let options_frame = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .build();
    options_frame.add_css_class("floating-panel");
    options_frame.add_css_class("floating-tray");
    options_frame.append(&options);
    let options_revealer = chrome_revealer(
        &options_frame,
        gtk::RevealerTransitionType::None,
        gtk::Align::Start,
        gtk::Align::Start,
        true,
    );
    options_frame.set_hexpand(false);
    options.set_hexpand(false);
    options_revealer.set_hexpand(false);
    options_revealer.set_margin_start(TOOLS_GUTTER);
    options_revealer.set_margin_top(TRAY_Y + 64);

    let tools = tool_strip(canvas, &options, &options_revealer, &options_page);
    let tools_revealer = chrome_revealer(
        &tools,
        gtk::RevealerTransitionType::None,
        gtk::Align::Start,
        gtk::Align::Start,
        true,
    );
    tools_revealer.set_margin_start(CHROME_MARGIN);
    tools_revealer.set_margin_top(TRAY_Y);

    let (colors, close_colors, color_handle) = floating_tray("Colors", &color_picker(canvas));
    let (widths, close_widths, width_handle) = floating_tray("Widths", &width_control(canvas));
    let colors_dragged = Rc::new(Cell::new(false));
    let widths_dragged = Rc::new(Cell::new(false));

    colors.set_hexpand(false);
    colors.set_halign(gtk::Align::Start);
    colors.set_margin_start(TOOLS_GUTTER);
    colors.set_margin_top(TRAY_Y);
    widths.set_hexpand(false);
    widths.set_halign(gtk::Align::Start);
    widths.set_margin_top(TRAY_Y);

    let restore_colors = island_restore_chip("Colors", "win.show-colors");
    restore_colors.set_margin_start(TOOLS_GUTTER);
    restore_colors.set_margin_top(TRAY_Y);
    let restore_widths = island_restore_chip("Widths", "win.show-widths");
    restore_widths.set_halign(gtk::Align::Start);
    restore_widths.set_margin_top(TRAY_Y);

    let show_colors = panel_action(
        "show-colors",
        &colors,
        &restore_colors,
        &colors_wanted,
        &chrome_visible,
    );
    let show_widths = panel_action(
        "show-widths",
        &widths,
        &restore_widths,
        &widths_wanted,
        &chrome_visible,
    );
    close_colors.connect_clicked({
        let show_colors = show_colors.clone();
        move |_| {
            show_colors.change_state(&false.to_variant());
        }
    });
    close_widths.connect_clicked({
        let show_widths = show_widths.clone();
        move |_| {
            show_widths.change_state(&false.to_variant());
        }
    });

    workspace.add_overlay(&tools_revealer);
    workspace.add_overlay(&colors);
    workspace.add_overlay(&widths);
    workspace.add_overlay(&restore_colors);
    workspace.add_overlay(&restore_widths);
    workspace.add_overlay(&options_revealer);
    workspace.set_clip_overlay(&colors, false);
    workspace.set_clip_overlay(&widths, false);
    workspace.set_clip_overlay(&restore_colors, false);
    workspace.set_clip_overlay(&restore_widths, false);
    workspace.set_measure_overlay(&colors, false);
    workspace.set_measure_overlay(&widths, false);
    workspace.set_measure_overlay(&restore_colors, false);
    workspace.set_measure_overlay(&restore_widths, false);
    pin_end_overlay(
        &workspace,
        &widths,
        WIDTH_MARGIN_END,
        TRAY_Y,
        &widths_dragged,
    );
    pin_end_overlay(
        &workspace,
        &restore_widths,
        WIDTH_MARGIN_END,
        TRAY_Y,
        &Rc::new(Cell::new(false)),
    );
    make_draggable(&color_handle, &colors, &workspace, &colors_dragged);
    make_draggable(&width_handle, &widths, &workspace, &widths_dragged);

    let formula_updating = Rc::new(Cell::new(false));
    canvas.connect_sheet_changed({
        let options = options.clone();
        let canvas = canvas.clone();
        let sheet_addr = sheet_addr.clone();
        let sheet_formula = sheet_formula.clone();
        let formula_updating = formula_updating.clone();
        move || {
            sync_options_stack(&canvas, &options);
            if canvas.active_layer_kind().is_spreadsheet() {
                formula_updating.set(true);
                if !sheet_addr.has_focus() {
                    sheet_addr.set_text(&canvas.sheet_address());
                }
                if !sheet_formula.has_focus() {
                    sheet_formula.set_text(&canvas.sheet_formula());
                }
                formula_updating.set(false);
            }
        }
    });

    let status_chip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(CHROME_MARGIN)
        .margin_bottom(CHROME_MARGIN)
        .build();
    status_chip.add_css_class("floating-panel");
    status_chip.add_css_class("status-chip");
    status_chip.append(&feedback.status);
    let status_revealer = chrome_revealer(
        &status_chip,
        gtk::RevealerTransitionType::None,
        gtk::Align::Start,
        gtk::Align::End,
        true,
    );
    workspace.add_overlay(&status_revealer);

    let zoom = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .margin_end(CHROME_MARGIN)
        .margin_bottom(CHROME_MARGIN)
        .build();
    zoom.add_css_class("floating-panel");
    zoom.add_css_class("zoom-chip");
    zoom.add_css_class("linked");
    let zoom_out = gtk::Button::builder()
        .icon_name("zoom-out-symbolic")
        .tooltip_text("Zoom out (Ctrl+-)")
        .action_name("win.zoom-out")
        .build();
    let reset = gtk::Button::builder()
        .child(&feedback.zoom)
        .tooltip_text("Reset view (Ctrl+0)")
        .action_name("win.reset-view")
        .build();
    reset.add_css_class("flat");
    let zoom_in = gtk::Button::builder()
        .icon_name("zoom-in-symbolic")
        .tooltip_text("Zoom in (Ctrl++)")
        .action_name("win.zoom-in")
        .build();
    for button in [&zoom_out, &reset, &zoom_in] {
        button.add_css_class("flat");
        zoom.append(button);
    }
    let zoom_revealer = chrome_revealer(
        &zoom,
        gtk::RevealerTransitionType::None,
        gtk::Align::End,
        gtk::Align::End,
        true,
    );
    workspace.add_overlay(&zoom_revealer);
    workspace.add_overlay(&replay_overlay(canvas, session));

    let restore_button = gtk::Button::builder().tooltip_text("Tools (F10)").build();
    let restore_content = adw::ButtonContent::builder()
        .icon_name("view-reveal-symbolic")
        .label("Tools")
        .build();
    restore_button.set_child(Some(&restore_content));
    restore_button.add_css_class("pill");
    restore_button.add_css_class("suggested-action");
    restore_button.add_css_class("restore-tools");
    let restore = chrome_revealer(
        &restore_button,
        gtk::RevealerTransitionType::None,
        gtk::Align::End,
        gtk::Align::Start,
        false,
    );
    workspace.add_overlay(&restore);
    (
        workspace,
        WorkspaceChrome {
            tools: tools_revealer,
            options: options_revealer,
            colors,
            widths,
            colors_wanted,
            widths_wanted,
            show_colors,
            show_widths,
            chrome_visible,
            status: status_revealer,
            zoom: zoom_revealer,
            restore,
            restore_button,
            restore_colors,
            restore_widths,
        },
    )
}

fn sync_options_stack(canvas: &Canvas, options: &gtk::Stack) {
    if canvas.active_layer_kind().is_spreadsheet() {
        options.set_visible_child_name("spreadsheet");
        return;
    }
    let page = match canvas.tool() {
        Tool::Select => "select",
        Tool::Pen | Tool::Highlighter | Tool::Brush => "ink",
        Tool::Eraser => "eraser",
        Tool::Pan => "pan",
        Tool::Text => "text",
        Tool::Shape | Tool::Connector => "diagram",
        Tool::Space => "space",
        Tool::Measure => "measure",
    };
    options.set_visible_child_name(page);
}

fn tool_strip(
    canvas: &Canvas,
    options: &gtk::Stack,
    options_host: &gtk::Revealer,
    options_page: &Rc<RefCell<String>>,
) -> gtk::Box {
    let tools = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(3)
        .build();
    tools.add_css_class("floating-panel");
    tools.add_css_class("tool-palette");
    let select = tool_button(
        "Select · V  (click, lasso, or follow a [[page]] link)",
        "select",
        Tool::Select,
        canvas,
        options,
        options_host,
        options_page,
        None,
    );
    let pen = tool_button(
        "Pen · P",
        "ink",
        Tool::Pen,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    pen.set_active(true);
    let brush = tool_button(
        "Brush",
        "ink",
        Tool::Brush,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let highlighter = tool_button(
        "Highlighter · H  (drawn behind ink)",
        "ink",
        Tool::Highlighter,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let eraser = tool_button(
        "Eraser · E  (splits ink strokes)",
        "eraser",
        Tool::Eraser,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let pan = tool_button(
        "Pan",
        "pan",
        Tool::Pan,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let text = tool_button(
        "Text · T  (click a table cell to edit it)",
        "text",
        Tool::Text,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let shape = tool_button(
        "Shape · S",
        "diagram",
        Tool::Shape,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let connector = tool_button(
        "Connector",
        "diagram",
        Tool::Connector,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let space = tool_button(
        "Insert space",
        "space",
        Tool::Space,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    let measure = tool_button(
        "Measure millimetres (ISO 129) · M. Hold Shift for 15°",
        "measure",
        Tool::Measure,
        canvas,
        options,
        options_host,
        options_page,
        Some(&select),
    );
    tools.append(&select);
    tools.append(&tool_separator());
    tools.append(&pen);
    tools.append(&brush);
    tools.append(&highlighter);
    tools.append(&eraser);
    tools.append(&tool_separator());
    tools.append(&pan);
    tools.append(&space);
    tools.append(&measure);
    tools.append(&tool_separator());
    tools.append(&text);
    tools.append(&shape);
    tools.append(&connector);
    tools.append(&tool_separator());
    let replay = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Replay ink on this page (Ctrl+Shift+R)")
        .action_name("win.replay")
        .build();
    replay.add_css_class("flat");
    replay.add_css_class("circular");
    replay.add_css_class("tool-button");
    tools.append(&replay);
    canvas.connect_tool_changed({
        let buttons = [
            (Tool::Select, select.clone()),
            (Tool::Pen, pen.clone()),
            (Tool::Brush, brush.clone()),
            (Tool::Highlighter, highlighter.clone()),
            (Tool::Eraser, eraser.clone()),
            (Tool::Pan, pan.clone()),
            (Tool::Text, text.clone()),
            (Tool::Shape, shape.clone()),
            (Tool::Connector, connector.clone()),
            (Tool::Space, space.clone()),
            (Tool::Measure, measure.clone()),
        ];
        move |tool| {
            for (candidate, button) in &buttons {
                if *candidate == tool && !button.is_active() {
                    button.set_active(true);
                }
            }
        }
    });
    tools
}

fn tool_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("tool-separator");
    separator
}

fn replay_overlay(canvas: &Canvas, session: &Rc<RefCell<Session>>) -> gtk::Revealer {
    let bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .margin_bottom(CHROME_MARGIN)
        .valign(gtk::Align::Center)
        .build();
    bar.add_css_class("floating-panel");
    bar.add_css_class("replay-chip");

    let play = gtk::Button::builder()
        .icon_name("media-playback-pause-symbolic")
        .tooltip_text("Pause or resume (Space)")
        .build();
    play.add_css_class("flat");
    play.add_css_class("circular");
    play.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.toggle_replay()
    });
    let stop = gtk::Button::builder()
        .icon_name("media-playback-stop-symbolic")
        .tooltip_text("Stop replay (Escape)")
        .build();
    stop.add_css_class("flat");
    stop.add_css_class("circular");
    stop.set_action_name(Some("win.replay-stop"));
    let caption = gtk::Label::builder()
        .label("0.0 / 0.0s")
        .xalign(0.0)
        .width_chars(12)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    caption.add_css_class("caption");
    bar.append(&play);
    bar.append(&stop);
    bar.append(&caption);

    let speeds = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .build();
    speeds.add_css_class("linked");
    let updating = Rc::new(Cell::new(false));
    let mut leader: Option<gtk::ToggleButton> = None;
    let current_speed = canvas.replay_status().speed;
    let speed_buttons: Vec<(f32, gtk::ToggleButton)> = REPLAY_SPEEDS
        .into_iter()
        .map(|speed| {
            let label = if (speed - 1.0).abs() < f32::EPSILON {
                "1×".to_owned()
            } else if speed < 1.0 {
                format!("{speed:.1}×")
            } else {
                format!("{:.0}×", speed)
            };
            let button = gtk::ToggleButton::builder()
                .label(label)
                .tooltip_text(format!("Replay at {speed}×"))
                .build();
            button.add_css_class("flat");
            button.add_css_class("replay-speed");
            if let Some(leader) = &leader {
                button.set_group(Some(leader));
            } else {
                leader = Some(button.clone());
            }
            if (speed - current_speed).abs() < f32::EPSILON {
                button.set_active(true);
            }
            button.connect_toggled({
                let canvas = canvas.clone();
                let session = session.clone();
                let updating = updating.clone();
                move |button| {
                    if updating.get() || !button.is_active() {
                        return;
                    }
                    canvas.set_replay_speed(speed);
                    persist_prefs(&session, |prefs| prefs.replay_speed = speed);
                }
            });
            speeds.append(&button);
            (speed, button)
        })
        .collect();
    bar.append(&speeds);

    let revealer = chrome_revealer(
        &bar,
        gtk::RevealerTransitionType::None,
        gtk::Align::Center,
        gtk::Align::End,
        false,
    );
    canvas.connect_replay_changed({
        let revealer = revealer.clone();
        let play = play.clone();
        let caption = caption.clone();
        let updating = updating.clone();
        move |status: ReplayStatus| {
            revealer.set_reveal_child(status.active);
            if status.playing {
                play.set_icon_name("media-playback-pause-symbolic");
                play.set_tooltip_text(Some("Pause (Space)"));
            } else {
                play.set_icon_name("media-playback-start-symbolic");
                play.set_tooltip_text(Some("Resume (Space)"));
            }
            if status.active {
                caption.set_label(&format!(
                    "{:.1} / {:.1}s",
                    status.elapsed_secs, status.duration_secs
                ));
            }
            updating.set(true);
            for (speed, button) in &speed_buttons {
                button.set_active((speed - status.speed).abs() < f32::EPSILON);
            }
            updating.set(false);
        }
    });
    revealer
}

fn floating_tray(
    title: &str,
    content: &impl IsA<gtk::Widget>,
) -> (gtk::Revealer, gtk::Button, gtk::Box) {
    let handle = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .valign(gtk::Align::Fill)
        .hexpand(false)
        .width_request(10)
        .tooltip_text("Drag to reposition")
        .build();
    handle.add_css_class("panel-grip");
    let close = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text(format!("Hide {title}"))
        .valign(gtk::Align::Center)
        .build();
    close.add_css_class("flat");
    close.add_css_class("circular");
    close.add_css_class("panel-close");
    let tray = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .hexpand(false)
        .build();
    tray.add_css_class("floating-panel");
    tray.add_css_class("floating-tray");
    tray.append(&handle);
    tray.append(content);
    tray.append(&close);
    let hover = gtk::EventControllerMotion::new();
    hover.connect_enter({
        let close = close.clone();
        move |_, _, _| close.add_css_class("panel-close-visible")
    });
    hover.connect_leave({
        let close = close.clone();
        move |_| close.remove_css_class("panel-close-visible")
    });
    tray.add_controller(hover);
    let revealer = chrome_revealer(
        &tray,
        gtk::RevealerTransitionType::None,
        gtk::Align::Start,
        gtk::Align::Start,
        true,
    );
    revealer.set_hexpand(false);
    revealer.set_vexpand(false);
    (revealer, close, handle)
}

fn pin_end_overlay(
    overlay: &gtk::Overlay,
    panel: &impl IsA<gtk::Widget>,
    margin_end: i32,
    margin_top: i32,
    dragged: &Rc<Cell<bool>>,
) {
    let overlay = overlay.clone();
    let panel = panel.clone().upcast::<gtk::Widget>();
    panel.set_halign(gtk::Align::Start);
    panel.set_valign(gtk::Align::Start);
    panel.set_hexpand(false);
    let dragged = dragged.clone();
    let pending = Rc::new(Cell::new(false));
    let canvas_width = Rc::new(Cell::new(0i32));
    let reposition = Rc::new({
        let overlay = overlay.clone();
        let panel = panel.clone();
        let dragged = dragged.clone();
        let pending = pending.clone();
        let canvas_width = canvas_width.clone();
        move || {
            if pending.get() {
                return;
            }
            pending.set(true);
            let overlay = overlay.clone();
            let panel = panel.clone();
            let dragged = dragged.clone();
            let pending = pending.clone();
            let canvas_width = canvas_width.clone();
            glib::idle_add_local_once(move || {
                pending.set(false);
                if dragged.get() {
                    return;
                }
                let overlay_w = canvas_width.get().max(overlay.width());
                if overlay_w <= 1 {
                    return;
                }
                panel.set_size_request(-1, -1);
                let measured = panel
                    .first_child()
                    .unwrap_or_else(|| panel.clone())
                    .measure(gtk::Orientation::Horizontal, -1)
                    .1;
                let fallback = 360;
                let natural = if measured < 80 || measured > overlay_w * 2 / 3 {
                    fallback
                } else {
                    measured + 8
                };
                if panel.width() != natural {
                    panel.set_size_request(natural, -1);
                }
                let x = (overlay_w - natural - margin_end).max(CHROME_MARGIN);
                if panel.margin_start() != x {
                    panel.set_margin_start(x);
                }
                if panel.margin_top() != margin_top {
                    panel.set_margin_top(margin_top);
                }
            });
        }
    });
    overlay
        .clone()
        .upcast::<gtk::Widget>()
        .connect_notify_local(Some("width"), {
            let reposition = Rc::clone(&reposition);
            move |_, _| reposition()
        });
    if let Some(child) = overlay.child()
        && let Ok(area) = child.downcast::<gtk::DrawingArea>()
    {
        area.connect_resize({
            let reposition = Rc::clone(&reposition);
            let canvas_width = canvas_width.clone();
            move |_, width, _| {
                canvas_width.set(width);
                reposition();
            }
        });
    }
    panel.connect_map({
        let reposition = Rc::clone(&reposition);
        move |_| reposition()
    });
    glib::timeout_add_local_once(std::time::Duration::from_millis(80), {
        let reposition = Rc::clone(&reposition);
        move || reposition()
    });
}

fn make_draggable(
    handle: &impl IsA<gtk::Widget>,
    panel: &impl IsA<gtk::Widget>,
    overlay: &gtk::Overlay,
    dragged: &Rc<Cell<bool>>,
) {
    let panel = panel.clone().upcast::<gtk::Widget>();
    let overlay = overlay.clone();
    let origin = Rc::new(Cell::new((0i32, 0i32)));
    let dragged = dragged.clone();
    let drag = gtk::GestureDrag::new();
    drag.set_button(1);
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    drag.connect_drag_begin({
        let panel = panel.clone();
        let overlay = overlay.clone();
        let origin = origin.clone();
        let dragged = dragged.clone();
        move |_, _, _| {
            dragged.set(true);
            let overlay_widget = overlay.clone().upcast::<gtk::Widget>();
            let point = panel
                .compute_point(&overlay_widget, &gtk::graphene::Point::new(0.0, 0.0))
                .unwrap_or_else(|| gtk::graphene::Point::new(8.0, 8.0));
            let x = point.x().round() as i32;
            let y = point.y().round() as i32;
            if let Some(parent) = panel.parent()
                && parent != overlay_widget
            {
                if let Ok(box_) = parent.downcast::<gtk::Box>() {
                    box_.remove(&panel);
                }
                overlay.add_overlay(&panel);
            }
            panel.set_halign(gtk::Align::Start);
            panel.set_valign(gtk::Align::Start);
            panel.set_hexpand(false);
            panel.set_size_request(-1, -1);
            panel.set_margin_end(0);
            panel.set_margin_start(x.max(8));
            panel.set_margin_top(y.max(8));
            origin.set((x.max(8), y.max(8)));
        }
    });
    drag.connect_drag_update({
        let panel = panel.clone();
        let origin = origin.clone();
        move |_, dx, dy| {
            let (ox, oy) = origin.get();
            let mut x = ox + dx.round() as i32;
            let mut y = oy + dy.round() as i32;
            if let Some(parent) = panel.parent() {
                let max_x = (parent.allocated_width() - panel.allocated_width()).max(8);
                let max_y = (parent.allocated_height() - panel.allocated_height()).max(8);
                x = x.clamp(8, max_x);
                y = y.clamp(8, max_y);
            } else {
                x = x.max(8);
                y = y.max(8);
            }
            panel.set_margin_start(x);
            panel.set_margin_top(y);
        }
    });
    handle.set_cursor_from_name(Some("grab"));
    handle.add_controller(drag);
}

fn island_restore_chip(title: &str, action: &str) -> gtk::Revealer {
    let button = gtk::Button::builder()
        .tooltip_text(format!("Show {title}"))
        .action_name(action)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    let content = adw::ButtonContent::builder()
        .icon_name("view-reveal-symbolic")
        .label(title)
        .build();
    button.set_child(Some(&content));
    button.add_css_class("pill");
    button.add_css_class("suggested-action");
    button.add_css_class("restore-island");
    let revealer = chrome_revealer(
        &button,
        gtk::RevealerTransitionType::None,
        gtk::Align::Start,
        gtk::Align::Start,
        false,
    );
    revealer.set_hexpand(false);
    revealer.set_vexpand(false);
    revealer
}

fn panel_action(
    name: &str,
    revealer: &gtk::Revealer,
    restore: &gtk::Revealer,
    wanted: &Rc<Cell<bool>>,
    chrome_visible: &Rc<Cell<bool>>,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new_stateful(name, None, &true.to_variant());
    action.connect_activate(|action, _| {
        let current = action
            .state()
            .and_then(|value| value.get::<bool>())
            .unwrap_or(true);
        action.change_state(&(!current).to_variant());
    });
    action.connect_change_state({
        let revealer = revealer.clone();
        let restore = restore.clone();
        let wanted = wanted.clone();
        let chrome_visible = chrome_visible.clone();
        move |action, value| {
            let Some(value) = value else {
                return;
            };
            let show = value.get::<bool>().unwrap_or(true);
            action.set_state(value);
            wanted.set(show);
            revealer.set_reveal_child(show && chrome_visible.get());
            restore.set_reveal_child(!show && chrome_visible.get());
        }
    });
    action
}

#[allow(clippy::too_many_arguments)]
fn tool_button(
    tooltip: &str,
    options_name: &str,
    tool: Tool,
    canvas: &Canvas,
    options: &gtk::Stack,
    options_host: &gtk::Revealer,
    options_page: &Rc<RefCell<String>>,
    group: Option<&gtk::ToggleButton>,
) -> gtk::ToggleButton {
    let glyph = tool_glyph(tool);
    let button = gtk::ToggleButton::builder()
        .child(&glyph)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class("tool-button");
    button.add_css_class("flat");
    if let Some(group) = group {
        button.set_group(Some(group));
    }
    button.connect_state_flags_changed({
        let glyph = glyph.clone();
        move |_, _| glyph.queue_draw()
    });
    let options_name_owned = options_name.to_string();
    button.connect_toggled({
        let canvas = canvas.clone();
        let options = options.clone();
        let options_host = options_host.clone();
        let options_page = options_page.clone();
        let glyph = glyph.clone();
        let options_name = options_name_owned.clone();
        move |button| {
            glyph.queue_draw();
            if button.is_active() {
                canvas.set_tool(tool);
                if canvas.active_layer_kind().is_spreadsheet() {
                    options.set_visible_child_name("spreadsheet");
                } else {
                    options.set_visible_child_name(&options_name);
                }
                options_page.replace(options_name.to_string());
                options_host.set_reveal_child(true);
            }
        }
    });
    button
}

fn tool_glyph(tool: Tool) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(20)
        .content_height(20)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    area.set_draw_func(move |widget, context, width, height| {
        #[allow(deprecated)]
        let color = widget.style_context().color();
        context.set_source_rgba(
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha()),
        );
        context.set_line_width(1.7);
        context.set_line_cap(gtk::cairo::LineCap::Round);
        context.set_line_join(gtk::cairo::LineJoin::Round);
        let size = f64::from(width.min(height));
        context.translate(
            (f64::from(width) - size) / 2.0,
            (f64::from(height) - size) / 2.0,
        );
        context.scale(size / 16.0, size / 16.0);
        draw_tool_glyph(context, tool);
    });
    area
}

fn draw_tool_glyph(context: &gtk::cairo::Context, tool: Tool) {
    match tool {
        Tool::Select => {
            context.set_dash(&[2.2, 1.6], 0.0);
            context.rectangle(3.2, 3.2, 9.6, 9.6);
            let _ = context.stroke();
            context.set_dash(&[], 0.0);
            context.rectangle(2.4, 2.4, 2.4, 2.4);
            context.rectangle(11.2, 2.4, 2.4, 2.4);
            context.rectangle(2.4, 11.2, 2.4, 2.4);
            context.rectangle(11.2, 11.2, 2.4, 2.4);
            let _ = context.fill();
        }
        Tool::Pen => {
            context.move_to(3.4, 12.6);
            context.line_to(10.8, 3.8);
            let _ = context.stroke();
            context.move_to(10.4, 3.2);
            context.line_to(12.8, 5.6);
            context.line_to(11.6, 6.4);
            context.close_path();
            let _ = context.fill();
            context.move_to(3.0, 13.8);
            context.line_to(6.4, 13.8);
            let _ = context.stroke();
        }
        Tool::Brush => {
            context.set_line_width(3.2);
            context.move_to(3.2, 12.8);
            context.line_to(11.4, 4.2);
            let _ = context.stroke();
            context.set_line_width(1.6);
            context.move_to(10.8, 3.6);
            context.line_to(13.0, 5.8);
            context.line_to(11.4, 7.0);
            context.close_path();
            let _ = context.fill();
        }
        Tool::Highlighter => {
            context.set_line_width(3.8);
            context.move_to(3.2, 11.4);
            context.line_to(11.6, 4.0);
            let _ = context.stroke();
            context.set_line_width(1.6);
            context.move_to(11.2, 3.4);
            context.line_to(13.0, 5.2);
            let _ = context.stroke();
        }
        Tool::Eraser => {
            context.move_to(4.0, 11.4);
            context.line_to(8.6, 4.2);
            context.line_to(12.2, 6.4);
            context.line_to(7.6, 13.6);
            context.close_path();
            let _ = context.stroke();
            context.move_to(6.2, 10.0);
            context.line_to(10.0, 7.6);
            let _ = context.stroke();
        }
        Tool::Pan => {
            context.move_to(8.0, 2.4);
            context.line_to(8.0, 13.6);
            context.move_to(2.4, 8.0);
            context.line_to(13.6, 8.0);
            let _ = context.stroke();
            for (x1, y1, x2, y2, x3, y3) in [
                (8.0, 2.4, 6.6, 4.4, 9.4, 4.4),
                (8.0, 13.6, 6.6, 11.6, 9.4, 11.6),
                (2.4, 8.0, 4.4, 6.6, 4.4, 9.4),
                (13.6, 8.0, 11.6, 6.6, 11.6, 9.4),
            ] {
                context.move_to(x1, y1);
                context.line_to(x2, y2);
                context.move_to(x1, y1);
                context.line_to(x3, y3);
            }
            let _ = context.stroke();
        }
        Tool::Text => {
            context.set_line_width(1.8);
            context.move_to(3.4, 3.6);
            context.line_to(12.6, 3.6);
            context.move_to(8.0, 3.6);
            context.line_to(8.0, 13.2);
            context.move_to(5.4, 13.2);
            context.line_to(10.6, 13.2);
            let _ = context.stroke();
        }
        Tool::Shape => {
            rounded_glyph_rect(context, 3.0, 3.0, 10.0, 10.0, 2.2);
            let _ = context.stroke();
        }
        Tool::Connector => {
            context.arc(4.2, 4.2, 1.7, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
            context.arc(11.8, 11.8, 1.7, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
            context.move_to(5.5, 5.5);
            context.line_to(10.5, 10.5);
            let _ = context.stroke();
        }
        Tool::Space => {
            context.move_to(8.0, 2.6);
            context.line_to(8.0, 13.4);
            context.move_to(5.2, 5.0);
            context.line_to(8.0, 2.6);
            context.line_to(10.8, 5.0);
            context.move_to(5.2, 11.0);
            context.line_to(8.0, 13.4);
            context.line_to(10.8, 11.0);
            let _ = context.stroke();
            context.move_to(3.2, 8.0);
            context.line_to(12.8, 8.0);
            let _ = context.stroke();
        }
        Tool::Measure => {
            context.move_to(3.2, 12.6);
            context.line_to(12.8, 3.4);
            let _ = context.stroke();
            context.set_line_width(1.3);
            context.move_to(3.2, 12.6);
            context.line_to(5.6, 13.4);
            context.move_to(12.8, 3.4);
            context.line_to(11.0, 2.2);
            let _ = context.stroke();
            context.move_to(6.4, 6.2);
            context.line_to(9.6, 9.4);
            let _ = context.stroke();
        }
    }
}

fn rounded_glyph_rect(context: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    context.new_sub_path();
    context.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    context.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    context.arc(
        x + r,
        y + h - r,
        r,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    context.arc(
        x + r,
        y + r,
        r,
        std::f64::consts::PI,
        3.0 * std::f64::consts::FRAC_PI_2,
    );
    context.close_path();
}

fn select_options() -> gtk::Box {
    let row = option_row();
    for (tooltip, icon, action) in [
        ("Cut", "edit-cut-symbolic", "win.cut"),
        ("Copy", "edit-copy-symbolic", "win.copy"),
        ("Copy SVG", "document-export-symbolic", "win.copy-svg"),
        ("Copy PNG", "image-x-generic-symbolic", "win.copy-png"),
        ("Paste", "edit-paste-symbolic", "win.paste"),
        ("Duplicate", "edit-copy-symbolic", "win.duplicate"),
    ] {
        row.append(&option_icon_button(icon, tooltip, action));
    }
    row.append(&option_separator());
    for (tooltip, icon, action) in [
        (
            "Align left",
            "format-justify-left-symbolic",
            "win.align-left",
        ),
        (
            "Align centre",
            "format-justify-center-symbolic",
            "win.align-center",
        ),
        (
            "Align right",
            "format-justify-right-symbolic",
            "win.align-right",
        ),
        (
            "Distribute",
            "object-flip-horizontal-symbolic",
            "win.distribute-x",
        ),
        ("Same width", "zoom-fit-best-symbolic", "win.same-width"),
    ] {
        row.append(&option_icon_button(icon, tooltip, action));
    }
    row.append(&option_separator());
    for (tooltip, icon, action) in [
        ("Bring to front", "go-up-symbolic", "win.bring-front"),
        ("Send to back", "go-down-symbolic", "win.send-back"),
        ("Rotate", "object-rotate-right-symbolic", "win.rotate"),
    ] {
        row.append(&option_icon_button(icon, tooltip, action));
    }
    row.append(&option_separator());
    row.append(&option_icon_button(
        "user-trash-symbolic",
        "Delete",
        "win.delete",
    ));
    row
}

fn ink_options(canvas: &Canvas, session: &Rc<RefCell<Session>>) -> gtk::Box {
    let row = option_row();
    let updating = Rc::new(Cell::new(false));
    let shape = gtk::ToggleButton::builder()
        .label("Shape")
        .tooltip_text("Convert a tidy freehand stroke into a line, rectangle, or ellipse. Shift snaps ink to 15°.")
        .active(canvas.ink_to_shape())
        .build();
    shape.add_css_class("flat");
    shape.add_css_class("style-chip");
    shape.connect_toggled({
        let canvas = canvas.clone();
        let session = session.clone();
        let updating = updating.clone();
        move |button| {
            if updating.get() {
                return;
            }
            canvas.set_ink_to_shape(button.is_active());
            persist_prefs(&session, |prefs| prefs.ink_to_shape = button.is_active());
        }
    });
    let ruler = gtk::ToggleButton::builder()
        .label("Ruler")
        .tooltip_text("Constrain ink to horizontal or vertical. Shift instead uses 15° ISO angles.")
        .active(canvas.ruler())
        .build();
    ruler.add_css_class("flat");
    ruler.add_css_class("style-chip");
    ruler.connect_toggled({
        let canvas = canvas.clone();
        let session = session.clone();
        let updating = updating.clone();
        move |button| {
            if updating.get() {
                return;
            }
            canvas.set_ruler(button.is_active());
            persist_prefs(&session, |prefs| prefs.ruler = button.is_active());
        }
    });
    row.append(&shape);
    row.append(&ruler);
    let stabilizer = gtk::ToggleButton::builder()
        .label("Lazy")
        .tooltip_text("Stabilise ink so the stroke follows the pointer more slowly")
        .active(canvas.stabilizer())
        .build();
    stabilizer.add_css_class("flat");
    stabilizer.add_css_class("style-chip");
    stabilizer.connect_toggled({
        let canvas = canvas.clone();
        let session = session.clone();
        let updating = updating.clone();
        move |button| {
            if updating.get() {
                return;
            }
            canvas.set_stabilizer(button.is_active());
            persist_prefs(&session, |prefs| prefs.stabilizer = button.is_active());
        }
    });
    let palm = gtk::ToggleButton::builder()
        .label("Palm")
        .tooltip_text("Ignore touch while a pen is in use")
        .active(canvas.ignore_touch())
        .build();
    palm.add_css_class("flat");
    palm.add_css_class("style-chip");
    palm.connect_toggled({
        let canvas = canvas.clone();
        let session = session.clone();
        let updating = updating.clone();
        move |button| {
            if updating.get() {
                return;
            }
            canvas.set_ignore_touch(button.is_active());
            persist_prefs(&session, |prefs| prefs.ignore_touch = button.is_active());
        }
    });
    let record = gtk::ToggleButton::builder()
        .label("Audio")
        .tooltip_text("Record while inking if pw-record, parecord, or arecord is installed")
        .active(canvas.is_recording_audio())
        .build();
    record.add_css_class("flat");
    record.add_css_class("style-chip");
    record.connect_toggled({
        let canvas = canvas.clone();
        move |button| {
            if button.is_active() {
                if canvas.start_audio_capture().is_err() {
                    button.set_active(false);
                }
            } else {
                let _ = canvas.stop_audio_capture();
            }
        }
    });
    row.append(&stabilizer);
    row.append(&palm);
    row.append(&record);
    row.append(&style_toggles(canvas, false));
    canvas.connect_ink_prefs_changed({
        let canvas = canvas.clone();
        let updating = updating.clone();
        let shape = shape.clone();
        let ruler = ruler.clone();
        let stabilizer = stabilizer.clone();
        let palm = palm.clone();
        move || {
            updating.set(true);
            shape.set_active(canvas.ink_to_shape());
            ruler.set_active(canvas.ruler());
            stabilizer.set_active(canvas.stabilizer());
            palm.set_active(canvas.ignore_touch());
            updating.set(false);
        }
    });
    row
}

fn text_options(canvas: &Canvas) -> gtk::Box {
    let row = option_row();
    let note = gtk::Entry::builder()
        .placeholder_text("Type text, then click the canvas. Click existing text to edit.")
        .width_chars(18)
        .build();
    note.connect_changed({
        let canvas = canvas.clone();
        move |entry| canvas.set_text(entry.text().to_string())
    });
    canvas.connect_text_loaded({
        let note = note.clone();
        move |text| {
            if note.text() != text {
                note.set_text(&text);
            }
        }
    });
    row.append(&note);
    let bold = format_toggle("B", "Bold", canvas.text_bold());
    bold.connect_toggled({
        let canvas = canvas.clone();
        move |button| canvas.set_text_bold(button.is_active())
    });
    let italic = format_toggle("I", "Italic", canvas.text_italic());
    italic.connect_toggled({
        let canvas = canvas.clone();
        move |button| canvas.set_text_italic(button.is_active())
    });
    let underline = format_toggle("U", "Underline", canvas.text_underline());
    underline.connect_toggled({
        let canvas = canvas.clone();
        move |button| canvas.set_text_underline(button.is_active())
    });
    row.append(&bold);
    row.append(&italic);
    row.append(&underline);
    let bullets = gtk::DropDown::from_strings(&["Paragraph", "Bullets", "Numbered", "To-do"]);
    bullets.set_tooltip_text(Some("List style"));
    bullets.connect_selected_notify({
        let canvas = canvas.clone();
        move |picker| {
            let style = match picker.selected() {
                1 => ListStyle::Bullet,
                2 => ListStyle::Numbered,
                3 => ListStyle::Checklist,
                _ => ListStyle::None,
            };
            canvas.set_list_style(style);
        }
    });
    row.append(&bullets);
    let size = gtk::SpinButton::with_range(6.0, 240.0, 1.0);
    size.set_digits(0);
    size.set_value(canvas.current_font_size() as f64);
    size.set_tooltip_text(Some("Typewriter font size"));
    size.add_css_class("width-spin");
    size.connect_value_changed({
        let canvas = canvas.clone();
        move |spin| canvas.set_font_size(spin.value() as f32)
    });
    row.append(&size);
    row
}

fn format_toggle(label: &str, tooltip: &str, active: bool) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .label(label)
        .tooltip_text(tooltip)
        .active(active)
        .build();
    button.add_css_class("flat");
    button.add_css_class("format-chip");
    button
}

fn diagram_options(canvas: &Canvas) -> gtk::Box {
    let row = option_row();
    let groups = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    groups.add_css_class("shape-grid");
    let mut current = None::<&'static str>;
    let mut chips: Option<gtk::Box> = None;
    let mut count = 0;
    for (index, kind) in ShapeKind::ALL.iter().enumerate() {
        if current != Some(kind.group()) || count == 6 {
            if current != Some(kind.group()) {
                current = Some(kind.group());
                let heading = gtk::Label::builder()
                    .label(kind.group())
                    .xalign(0.0)
                    .build();
                heading.add_css_class("caption-heading");
                heading.add_css_class("dim-label");
                groups.append(&heading);
            }
            let line = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(4)
                .homogeneous(false)
                .build();
            groups.append(&line);
            chips = Some(line);
            count = 0;
        }
        let button = gtk::Button::builder()
            .label(kind.short_name())
            .tooltip_text(ShapeKind::NAMES[index])
            .build();
        button.add_css_class("flat");
        button.add_css_class("shape-chip");
        button.connect_clicked({
            let canvas = canvas.clone();
            let kind = *kind;
            move |_| canvas.set_shape_kind(kind)
        });
        if let Some(line) = &chips {
            line.append(&button);
            count += 1;
        }
    }
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(220)
        .max_content_height(280)
        .width_request(280)
        .child(&groups)
        .build();
    let popover = gtk::Popover::builder()
        .autohide(true)
        .child(&scrolled)
        .build();
    let trigger = gtk::MenuButton::builder()
        .label("Symbols")
        .tooltip_text(
            "Geometry, IEC 60617, logic gates, and ISO 128/129 symbols. Shift constrains.",
        )
        .popover(&popover)
        .build();
    trigger.add_css_class("flat");
    let label = gtk::Entry::builder()
        .placeholder_text("Optional label")
        .width_chars(14)
        .build();
    label.connect_changed({
        let canvas = canvas.clone();
        move |entry| canvas.set_label(entry.text().to_string())
    });
    row.append(&trigger);
    row.append(&label);
    row.append(&style_toggles(canvas, true));
    row
}

fn spreadsheet_options(canvas: &Canvas) -> (gtk::Box, gtk::Entry, gtk::Entry) {
    let row = compact_option_row();
    let addr = gtk::Entry::builder()
        .text("A1")
        .width_chars(7)
        .max_width_chars(12)
        .tooltip_text("Name box · type A1 or A1:B5 and press Enter")
        .build();
    addr.add_css_class("sheet-address");
    addr.connect_activate({
        let canvas = canvas.clone();
        let addr = addr.clone();
        move |_| {
            if !canvas.goto_sheet_address(&addr.text()) {
                addr.set_text(&canvas.sheet_address());
            }
        }
    });
    let formula = gtk::Entry::builder()
        .placeholder_text("=SUM(A1:A10)")
        .tooltip_text("Formula bar · Enter commits, F2 edits on the canvas")
        .width_chars(16)
        .build();
    formula.add_css_class("sheet-formula");
    let updating = Rc::new(Cell::new(false));
    formula.connect_changed({
        let canvas = canvas.clone();
        let updating = updating.clone();
        move |entry| {
            if updating.get() {
                return;
            }
            canvas.set_sheet_formula(entry.text().to_string());
        }
    });
    formula.connect_activate({
        let canvas = canvas.clone();
        move |_| canvas.commit_sheet_formula_and_move()
    });
    canvas.connect_sheet_changed({
        let canvas = canvas.clone();
        let formula = formula.clone();
        let updating = updating.clone();
        move || {
            if formula.has_focus() {
                return;
            }
            updating.set(true);
            formula.set_text(&canvas.sheet_formula());
            updating.set(false);
        }
    });
    let bold = gtk::Button::builder()
        .label("B")
        .tooltip_text("Bold selected cells (Ctrl+B)")
        .build();
    bold.add_css_class("flat");
    bold.add_css_class("pill");
    bold.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.toggle_sheet_bold()
    });
    let align = gtk::Button::builder()
        .label("Align")
        .tooltip_text("Cycle left, center, and right alignment")
        .build();
    align.add_css_class("flat");
    align.add_css_class("pill");
    align.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.cycle_sheet_align()
    });
    let fill = gtk::Button::builder()
        .label("Fill")
        .tooltip_text("Fill the rest of the selection from the first row or cell")
        .build();
    fill.add_css_class("flat");
    fill.add_css_class("pill");
    fill.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.fill_sheet_selection()
    });
    let italic = gtk::Button::builder()
        .label("I")
        .tooltip_text("Italic selected cells (Ctrl+I)")
        .build();
    italic.add_css_class("flat");
    italic.add_css_class("pill");
    italic.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.toggle_sheet_italic()
    });
    let percent = gtk::Button::builder()
        .label("%/$")
        .tooltip_text("Cycle number, percent, currency, and date formats")
        .build();
    percent.add_css_class("flat");
    percent.add_css_class("pill");
    percent.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.cycle_number_format()
    });
    let merge = gtk::Button::builder()
        .label("Merge")
        .tooltip_text("Merge the selected cells")
        .build();
    merge.add_css_class("flat");
    merge.add_css_class("pill");
    merge.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.merge_sheet_selection()
    });
    let sort = gtk::Button::builder()
        .label("A↓")
        .tooltip_text("Sort the selection by the first column")
        .build();
    sort.add_css_class("flat");
    sort.add_css_class("pill");
    sort.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.sort_sheet_selection()
    });
    let add_cols = gtk::Button::builder()
        .label("+Col")
        .tooltip_text("Add five columns to the right of the grid")
        .build();
    add_cols.add_css_class("flat");
    add_cols.add_css_class("pill");
    add_cols.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.add_sheet_cols()
    });
    let add_rows = gtk::Button::builder()
        .label("+Row")
        .tooltip_text("Add five rows to the bottom of the grid")
        .build();
    add_rows.add_css_class("flat");
    add_rows.add_css_class("pill");
    add_rows.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.add_sheet_rows()
    });
    let insert_col = gtk::Button::builder()
        .label("Ins C")
        .tooltip_text("Insert a column at the selection")
        .build();
    insert_col.add_css_class("flat");
    insert_col.add_css_class("pill");
    insert_col.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.insert_sheet_col()
    });
    let insert_row = gtk::Button::builder()
        .label("Ins R")
        .tooltip_text("Insert a row at the selection")
        .build();
    insert_row.add_css_class("flat");
    insert_row.add_css_class("pill");
    insert_row.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.insert_sheet_row()
    });
    let freeze = gtk::Button::builder()
        .label("Freeze")
        .tooltip_text("Freeze rows above and columns left of the active cell (A1 clears)")
        .build();
    freeze.add_css_class("flat");
    freeze.add_css_class("pill");
    freeze.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.freeze_sheet_panes()
    });
    let sheet = gtk::Button::builder()
        .label("Sheet")
        .tooltip_text("Add a worksheet tab")
        .build();
    sheet.add_css_class("flat");
    sheet.add_css_class("pill");
    sheet.connect_clicked({
        let canvas = canvas.clone();
        move |_| canvas.add_workbook_sheet()
    });
    row.append(&option_hint("fx"));
    row.append(&addr);
    row.append(&formula);
    row.append(&bold);
    row.append(&italic);
    row.append(&align);
    row.append(&percent);
    row.append(&freeze);
    let tools = compact_option_row();
    tools.append(&fill);
    tools.append(&merge);
    tools.append(&sort);
    tools.append(&add_cols);
    tools.append(&add_rows);
    tools.append(&insert_col);
    tools.append(&insert_row);
    tools.append(&sheet);
    tools.append(&sheet_fill_colors(canvas));
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .halign(gtk::Align::Center)
        .build();
    column.append(&row);
    column.append(&tools);
    (column, addr, formula)
}

fn hint_options(text: &str) -> gtk::Box {
    let row = option_row();
    row.append(&option_hint(text));
    row
}

fn option_row() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .hexpand(false)
        .build()
}

fn compact_option_row() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk::Align::Center)
        .margin_start(6)
        .margin_end(6)
        .margin_top(2)
        .margin_bottom(2)
        .build()
}

fn option_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Vertical);
    separator.add_css_class("option-separator");
    separator
}

fn option_icon_button(icon: &str, tooltip: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .action_name(action)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("option-icon");
    button
}

fn option_hint(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label
}

fn sheet_fill_colors(canvas: &Canvas) -> gtk::Box {
    const COLORS: [(&str, &str, Color); 6] = [
        ("swatch-ink", "Ink", Color::INK),
        ("swatch-blue", "Blue", Color::BLUE),
        ("swatch-red", "Red", Color::rgb(0.84, 0.16, 0.20)),
        ("swatch-green", "Green", Color::rgb(0.05, 0.56, 0.32)),
        ("swatch-violet", "Violet", Color::rgb(0.48, 0.20, 0.78)),
        ("swatch-amber", "Amber", Color::rgb(0.95, 0.62, 0.05)),
    ];
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();
    for (class, name, color) in COLORS {
        let swatch = gtk::Button::builder().build();
        swatch.set_tooltip_text(Some(&format!("Fill selected cells with {name}")));
        swatch.add_css_class("color-swatch");
        swatch.add_css_class(class);
        swatch.add_css_class("flat");
        swatch.connect_clicked({
            let canvas = canvas.clone();
            move |_| canvas.fill_sheet_color(color)
        });
        row.append(&swatch);
    }
    row
}

fn color_picker(canvas: &Canvas) -> gtk::Box {
    const COLORS: [(&str, &str, Color); 12] = [
        ("swatch-ink", "Ink", Color::INK),
        ("swatch-white", "White", Color::rgb(0.97, 0.97, 0.96)),
        ("swatch-gray", "Gray", Color::rgb(0.42, 0.45, 0.50)),
        ("swatch-blue", "Blue", Color::BLUE),
        ("swatch-cyan", "Cyan", Color::rgb(0.05, 0.60, 0.66)),
        ("swatch-green", "Green", Color::rgb(0.05, 0.56, 0.32)),
        ("swatch-amber", "Amber", Color::rgb(0.95, 0.62, 0.05)),
        ("swatch-orange", "Orange", Color::rgb(0.89, 0.42, 0.07)),
        ("swatch-red", "Red", Color::rgb(0.84, 0.16, 0.20)),
        ("swatch-pink", "Pink", Color::rgb(0.84, 0.24, 0.53)),
        ("swatch-violet", "Violet", Color::rgb(0.48, 0.20, 0.78)),
        ("swatch-brown", "Brown", Color::rgb(0.54, 0.35, 0.20)),
    ];
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .valign(gtk::Align::Center)
        .build();
    let group: Rc<RefCell<Option<gtk::ToggleButton>>> = Rc::new(RefCell::new(None));
    for (class, name, color) in COLORS {
        let swatch = color_swatch(canvas, &group, name, color);
        swatch.add_css_class(class);
        row.append(&swatch);
    }
    let custom = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add a custom color")
        .build();
    custom.add_css_class("flat");
    custom.add_css_class("circular");
    custom.add_css_class("color-add");
    custom.connect_clicked({
        let canvas = canvas.clone();
        let row = row.clone();
        let group = group.clone();
        move |button| {
            let Some(parent) = button.root().and_downcast::<gtk::Window>() else {
                return;
            };
            let dialog = gtk::ColorChooserDialog::builder()
                .title("Choose a color")
                .transient_for(&parent)
                .modal(true)
                .build();
            dialog.set_use_alpha(false);
            dialog.set_rgba(&canvas_rgba(&canvas));
            dialog.connect_response({
                let canvas = canvas.clone();
                let row = row.clone();
                let group = group.clone();
                let add = button.clone();
                move |dialog, response| {
                    if response == gtk::ResponseType::Ok {
                        let rgba = dialog.rgba();
                        let color = Color::rgb(rgba.red(), rgba.green(), rgba.blue());
                        let swatch = color_swatch(&canvas, &group, "Custom", color);
                        paint_swatch(&swatch, color);
                        if let Some(before) = add.prev_sibling() {
                            row.insert_child_after(&swatch, Some(&before));
                        } else {
                            row.prepend(&swatch);
                        }
                        swatch.set_active(true);
                    }
                    dialog.close();
                }
            });
            dialog.present();
        }
    });
    row.append(&custom);
    if let Some(first) = group.borrow().as_ref() {
        first.set_active(true);
    }
    row
}

fn color_swatch(
    canvas: &Canvas,
    group: &Rc<RefCell<Option<gtk::ToggleButton>>>,
    name: &str,
    color: Color,
) -> gtk::ToggleButton {
    let swatch = gtk::ToggleButton::builder().tooltip_text(name).build();
    swatch.add_css_class("color-swatch");
    if let Some(leader) = group.borrow().as_ref() {
        swatch.set_group(Some(leader));
    } else {
        *group.borrow_mut() = Some(swatch.clone());
    }
    swatch.connect_toggled({
        let canvas = canvas.clone();
        move |button| {
            if button.is_active() {
                canvas.set_color(color);
            }
        }
    });
    swatch
}

fn paint_swatch(swatch: &gtk::ToggleButton, color: Color) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&format!(
        ".color-swatch {{ background-color: rgb({},{},{}); }}",
        (color.red * 255.0).round() as u8,
        (color.green * 255.0).round() as u8,
        (color.blue * 255.0).round() as u8
    ));
    #[allow(deprecated)]
    swatch
        .style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

fn canvas_rgba(canvas: &Canvas) -> gtk::gdk::RGBA {
    let color = canvas.current_color();
    gtk::gdk::RGBA::new(color.red, color.green, color.blue, color.alpha)
}

fn width_control(canvas: &Canvas) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .valign(gtk::Align::Center)
        .hexpand(false)
        .build();
    let spin = gtk::SpinButton::with_range(
        f64::from(pt_to_mm(MIN_STROKE_WIDTH)),
        f64::from(pt_to_mm(MAX_STROKE_WIDTH)),
        0.05,
    );
    spin.set_digits(2);
    spin.set_numeric(true);
    spin.set_snap_to_ticks(false);
    spin.set_increments(0.05, 0.25);
    spin.set_value(f64::from(pt_to_mm(canvas.current_width())));
    spin.set_tooltip_text(Some("Stroke width in millimetres (ISO 128)"));
    spin.add_css_class("width-spin");
    spin.add_css_class("compact-spin");
    spin.set_width_chars(3);
    let stepper = width_stepper(&spin);

    let chips = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(3)
        .valign(gtk::Align::Center)
        .hexpand(false)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .overlay_scrolling(true)
        .propagate_natural_width(true)
        .propagate_natural_height(true)
        .hexpand(false)
        .halign(gtk::Align::Start)
        .child(&chips)
        .build();
    scroller.add_css_class("width-presets");
    scroller.set_max_content_width(240);

    let group: Rc<RefCell<Option<gtk::ToggleButton>>> = Rc::new(RefCell::new(None));
    let updating = Rc::new(Cell::new(false));
    let defaults = ISO_LINE_WIDTHS_MM;
    for width in defaults {
        chips.append(&width_chip(
            canvas, &spin, &group, &updating, width, false, &chips,
        ));
    }

    let add = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(
            "Add the current width to quick presets. Right-click a custom chip to remove it.",
        )
        .valign(gtk::Align::Center)
        .build();
    add.add_css_class("flat");
    add.add_css_class("circular");
    add.add_css_class("width-add");
    add.connect_clicked({
        let canvas = canvas.clone();
        let chips = chips.clone();
        let group = group.clone();
        let updating = updating.clone();
        let spin = spin.clone();
        move |_| {
            let mm = ((spin.value() * 100.0).round() / 100.0) as f32;
            if let Some(existing) = find_width_chip(&chips, mm) {
                existing.set_active(true);
                return;
            }
            let chip = width_chip(&canvas, &spin, &group, &updating, mm, true, &chips);
            chips.prepend(&chip);
            chip.set_active(true);
        }
    });

    spin.connect_value_changed({
        let canvas = canvas.clone();
        let chips = chips.clone();
        let updating = updating.clone();
        move |spin| {
            if updating.get() {
                return;
            }
            let mm = spin.value() as f32;
            canvas.set_width(mm_to_pt(mm));
            updating.set(true);
            activate_matching_chip(&chips, mm);
            updating.set(false);
            spin.set_tooltip_text(Some(&format!("Stroke width {mm:.2} mm")));
        }
    });

    row.append(&stepper);
    row.append(&scroller);
    row.append(&add);
    row.append(&dash_toggle(canvas));
    let current = pt_to_mm(canvas.current_width());
    if let Some(chip) = find_width_chip(&chips, current) {
        chip.set_active(true);
    }
    row
}

fn width_stepper(spin: &gtk::SpinButton) -> gtk::Box {
    let stepper = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .valign(gtk::Align::Center)
        .build();
    stepper.add_css_class("width-stepper");
    let minus = gtk::Button::from_icon_name("list-remove-symbolic");
    let plus = gtk::Button::from_icon_name("list-add-symbolic");
    minus.set_tooltip_text(Some("Decrease width"));
    plus.set_tooltip_text(Some("Increase width"));
    minus.add_css_class("flat");
    plus.add_css_class("flat");
    minus.connect_clicked({
        let spin = spin.clone();
        move |_| {
            let next = ((spin.value() - 0.05) * 100.0).round() / 100.0;
            spin.set_value(next.max(f64::from(pt_to_mm(MIN_STROKE_WIDTH))));
        }
    });
    plus.connect_clicked({
        let spin = spin.clone();
        move |_| {
            let next = ((spin.value() + 0.05) * 100.0).round() / 100.0;
            spin.set_value(next.min(f64::from(pt_to_mm(MAX_STROKE_WIDTH))));
        }
    });
    stepper.append(&minus);
    stepper.append(spin);
    stepper.append(&plus);
    stepper
}

fn dash_toggle(canvas: &Canvas) -> gtk::ToggleButton {
    let glyph = gtk::DrawingArea::builder()
        .content_width(18)
        .content_height(18)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    glyph.set_draw_func(move |widget, context, _width, height| {
        #[allow(deprecated)]
        let color = widget.style_context().color();
        context.set_source_rgba(
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha()),
        );
        context.set_line_width(1.7);
        context.set_line_cap(gtk::cairo::LineCap::Round);
        let y = f64::from(height) / 2.0;
        for start in [1.6, 7.2, 12.8] {
            context.move_to(start, y);
            context.line_to(start + 3.2, y);
            let _ = context.stroke();
        }
    });
    let button = gtk::ToggleButton::builder()
        .child(&glyph)
        .tooltip_text("Dashed stroke")
        .active(canvas.is_dashed())
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("style-icon");
    button.connect_state_flags_changed({
        let glyph = glyph.clone();
        move |_, _| glyph.queue_draw()
    });
    button.connect_toggled({
        let canvas = canvas.clone();
        let glyph = glyph.clone();
        move |button| {
            canvas.set_dashed(button.is_active());
            glyph.queue_draw();
        }
    });
    button
}

fn width_chip(
    canvas: &Canvas,
    spin: &gtk::SpinButton,
    group: &Rc<RefCell<Option<gtk::ToggleButton>>>,
    updating: &Rc<Cell<bool>>,
    width: f32,
    removable: bool,
    chips: &gtk::Box,
) -> gtk::ToggleButton {
    let label = if (width * 100.0 - (width * 100.0).round()).abs() < 0.01
        && (width * 10.0 - (width * 10.0).round()).abs() < 0.01
    {
        if (width - width.round()).abs() < 0.01 {
            format!("{}", width.round() as i32)
        } else {
            format!("{width:.1}")
        }
    } else {
        format!("{width:.2}")
    };
    let chip = gtk::ToggleButton::builder()
        .label(&label)
        .tooltip_text(format!("{width:.2} mm"))
        .build();
    chip.add_css_class("width-chip");
    chip.add_css_class("flat");
    chip.set_widget_name(&format!("width-mm-{width:.2}"));
    if let Some(leader) = group.borrow().as_ref() {
        chip.set_group(Some(leader));
    } else {
        *group.borrow_mut() = Some(chip.clone());
    }
    chip.connect_toggled({
        let canvas = canvas.clone();
        let spin = spin.clone();
        let updating = updating.clone();
        move |button| {
            if !button.is_active() {
                return;
            }
            canvas.set_width(mm_to_pt(width));
            if !updating.get() {
                updating.set(true);
                spin.set_value(width as f64);
                updating.set(false);
            }
        }
    });
    if removable {
        let click = gtk::GestureClick::new();
        click.set_button(3);
        click.connect_pressed({
            let chip = chip.clone();
            let chips = chips.clone();
            move |_, _, _, _| chips.remove(&chip)
        });
        chip.add_controller(click);
    }
    chip
}

fn find_width_chip(chips: &gtk::Box, width: f32) -> Option<gtk::ToggleButton> {
    let name = format!("width-mm-{width:.2}");
    let mut child = chips.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.widget_name() == name
            && let Ok(button) = widget.downcast::<gtk::ToggleButton>()
        {
            return Some(button);
        }
    }
    None
}

fn activate_matching_chip(chips: &gtk::Box, width: f32) {
    if let Some(chip) = find_width_chip(chips, width) {
        chip.set_active(true);
        return;
    }
    let mut child = chips.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.downcast::<gtk::ToggleButton>() {
            button.set_active(false);
        }
    }
}

fn style_toggles(canvas: &Canvas, include_fill: bool) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .valign(gtk::Align::Center)
        .build();
    let dashed = gtk::ToggleButton::builder()
        .label("Dash")
        .tooltip_text("Dashed stroke")
        .active(canvas.is_dashed())
        .build();
    dashed.add_css_class("flat");
    dashed.add_css_class("style-chip");
    dashed.connect_toggled({
        let canvas = canvas.clone();
        move |button| canvas.set_dashed(button.is_active())
    });
    row.append(&dashed);
    if include_fill {
        let fill = gtk::ToggleButton::builder()
            .label("Fill")
            .tooltip_text("Fill shapes with a translucent wash of the current color")
            .active(canvas.fill_enabled())
            .build();
        fill.add_css_class("flat");
        fill.add_css_class("style-chip");
        fill.connect_toggled({
            let canvas = canvas.clone();
            move |button| canvas.set_fill_enabled(button.is_active())
        });
        row.append(&fill);
    }
    row
}

#[allow(clippy::too_many_arguments)]
fn install_actions(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
    split_view: &adw::OverlaySplitView,
    chrome: &WorkspaceChrome,
    tools_toggle: &gtk::ToggleButton,
) {
    window.add_action(&gio::PropertyAction::new(
        "toggle-sidebar",
        split_view,
        "show-sidebar",
    ));
    add_action(window, "new", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || {
            prompt_new_notebook(&window, &canvas, &navigator, &status);
        }
    });
    add_action(window, "open", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || {
            if canvas.is_dirty() {
                confirm_discard(&window, {
                    let window = window.clone();
                    let canvas = canvas.clone();
                    let status = status.clone();
                    let navigator = navigator.clone();
                    move || choose_open(&window, &canvas, &status, &navigator)
                });
            } else {
                choose_open(&window, &canvas, &status, &navigator);
            }
        }
    });
    add_action(window, "new-category", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || prompt_new_category(&window, &canvas, &navigator, &status)
    });
    add_action(window, "show-library", {
        let navigator = navigator.clone();
        let status = status.clone();
        move || {
            let root = navigator.library.borrow().root.clone();
            let _ = std::process::Command::new("xdg-open").arg(&root).spawn();
            status.whisper("Opened library in Files");
        }
    });
    add_action(window, "move-notebook", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || prompt_move_notebook(&window, &canvas, &navigator, &status)
    });
    add_action(window, "add-to-library", {
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let window = window.clone();
        let status = status.clone();
        move || {
            let personal = navigator.library.borrow().root.join("Personal");
            match canvas.move_current_notebook(&personal) {
                Ok(path) => {
                    let _ = navigator.session.borrow_mut().remember(&path);
                    navigator.refresh(&canvas);
                    navigator.refresh_library(&canvas);
                    refresh_status(&window, &canvas, &status, "Moved into Personal");
                }
                Err(error) => show_error(&window, &error.to_string()),
            }
        }
    });
    add_action(window, "save", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || match canvas.save_current() {
            Ok(true) => refresh_status(&window, &canvas, &status, "Saved"),
            Ok(false) => choose_save(&window, &canvas, &status, &navigator),
            Err(error) => show_error(&window, &error.to_string()),
        }
    });
    add_action(window, "save-as", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || choose_save(&window, &canvas, &status, &navigator)
    });
    add_action(window, "export-svg", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export(&window, &canvas, &status)
    });
    add_action(window, "export-pdf", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export_pdf(&window, &canvas, &status)
    });
    add_action(window, "export-png", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export_image(&window, &canvas, &status, "png", "PNG image")
    });
    add_action(window, "export-jpeg", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export_image(&window, &canvas, &status, "jpeg", "JPEG image")
    });
    add_action(window, "print", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || print_notebook(&window, &canvas, &status)
    });
    add_action(window, "import-media", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_import(&window, &canvas, &status)
    });
    add_action(window, "insert-table", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.insert_table(
                canvas.runtime_prefs().table_cols,
                canvas.runtime_prefs().table_rows,
            );
            let prefs = canvas.runtime_prefs();
            status.show(format!(
                "Inserted a {}×{} table",
                prefs.table_cols, prefs.table_rows
            ));
        }
    });
    add_action(window, "insert-date", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.insert_date();
            status.show("Inserted date and time");
        }
    });
    add_action(window, "calculate", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || match canvas.calculate_selection_or_pending() {
            Ok(result) => status.show(format!("Calculated {result}")),
            Err(err) => status.whisper(err),
        }
    });
    add_action(window, "open-attachment", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || match canvas.open_selected_attachment() {
            Ok(true) => status.show("Opened attachment"),
            Ok(false) => status.whisper("Select a file, PDF, or audio card first"),
            Err(error) => show_error(&window, &error.to_string()),
        }
    });
    add_action(window, "delete-section", {
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move || {
            if canvas.remove_active_section() {
                navigator.refresh(&canvas);
                status.show("Section moved to Recycle bin");
            } else {
                status.show("A notebook needs at least one section");
            }
        }
    });
    add_action(window, "fullscreen", {
        let window = window.clone();
        move || {
            if window.is_fullscreen() {
                window.unfullscreen();
            } else {
                window.fullscreen();
            }
        }
    });
    let insert_tag = gio::SimpleAction::new("insert-tag", Some(glib::VariantTy::UINT32));
    insert_tag.connect_activate({
        let canvas = canvas.clone();
        let status = status.clone();
        move |_, value| {
            let Some(index) = value.and_then(|value| value.get::<u32>()) else {
                return;
            };
            if let Some(kind) = TagKind::ALL.get(index as usize).copied() {
                canvas.insert_tag(kind);
                status.show(format!("Inserted {} tag", kind.label()));
            }
        }
    });
    window.add_action(&insert_tag);
    let apply_template = gio::SimpleAction::new("apply-template", Some(glib::VariantTy::UINT32));
    apply_template.connect_activate({
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move |_, value| {
            let Some(index) = value.and_then(|value| value.get::<u32>()) else {
                return;
            };
            if let Some(template) = PageTemplate::ALL.get(index as usize).copied() {
                canvas.apply_template(template);
                navigator.refresh(&canvas);
                status.show(format!("{} template", PageTemplate::NAMES[index as usize]));
            }
        }
    });
    window.add_action(&apply_template);
    add_action(window, "undo", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || {
            canvas.undo();
            navigator.refresh(&canvas);
            refresh_status(&window, &canvas, &status, "Undid last edit");
        }
    });
    add_action(window, "redo", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || {
            canvas.redo();
            navigator.refresh(&canvas);
            refresh_status(&window, &canvas, &status, "Redid last edit");
        }
    });
    add_action(window, "delete", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.delete_selection();
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Deleted {count} selected object(s)"),
            );
        }
    });
    add_action(window, "duplicate", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.duplicate_selection();
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Duplicated {count} selected object(s)"),
            );
        }
    });
    add_action(window, "copy", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.copy_selection();
            status.whisper("Copied selection");
        }
    });
    add_action(window, "cut", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.cut_selection();
            status.whisper("Cut selection");
        }
    });
    add_action(window, "paste", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.paste_clipboard();
            status.whisper("Pasted");
        }
    });
    add_action(window, "bring-front", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.bring_selection_to_front();
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Brought {count} object(s) to front"),
            );
        }
    });
    add_action(window, "send-back", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.send_selection_to_back();
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Sent {count} object(s) to back"),
            );
        }
    });
    add_action(window, "rotate", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.rotate_selection(90.0);
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Rotated {count} object(s)"),
            );
        }
    });
    add_action(window, "previous-page", {
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move || {
            if canvas.cycle_page(-1) {
                navigator.refresh(&canvas);
                status.whisper("Previous page");
            }
        }
    });
    add_action(window, "next-page", {
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move || {
            if canvas.cycle_page(1) {
                navigator.refresh(&canvas);
                status.whisper("Next page");
            }
        }
    });
    add_action(window, "reset-view", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.reset_view();
            refresh_status_quiet(&window, &canvas, &status, "View reset to 100%");
        }
    });
    add_action(window, "zoom-in", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let step = canvas.runtime_prefs().zoom_step.max(1.05);
            let target = canvas.animate_zoom_by(step);
            status.whisper(format!("Zooming to {target}%"));
        }
    });
    add_action(window, "zoom-out", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let step = canvas.runtime_prefs().zoom_step.max(1.05);
            let target = canvas.animate_zoom_by(1.0 / step);
            status.whisper(format!("Zooming to {target}%"));
        }
    });
    add_action(window, "focus-search", {
        let split_view = split_view.clone();
        let navigator = navigator.clone();
        move || {
            split_view.set_show_sidebar(true);
            navigator.search.grab_focus();
        }
    });
    add_action(window, "night-paper", {
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move || {
            canvas.apply_night_paper();
            navigator.refresh(&canvas);
            status.show("Night paper");
        }
    });
    add_action(window, "replay", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            if canvas.start_replay() {
                status.whisper("Replaying ink. Space pauses, Escape stops.");
            } else {
                status.whisper("This page has nothing to replay");
            }
        }
    });
    add_action(window, "replay-toggle", {
        let canvas = canvas.clone();
        move || canvas.toggle_replay()
    });
    add_action(window, "replay-stop", {
        let canvas = canvas.clone();
        move || canvas.stop_replay()
    });
    add_action(window, "copy-svg", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || match canvas.copy_selection_svg() {
            Some(svg) => {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&svg);
                    status.show("Copied selection as SVG");
                }
            }
            None => status.whisper("Select objects first"),
        }
    });
    add_action(window, "copy-png", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || match canvas.copy_selection_png() {
            Ok(Some(png)) => {
                let loader = gdk_pixbuf::PixbufLoader::new();
                if loader.write(&png).is_ok()
                    && loader.close().is_ok()
                    && let Some(pixbuf) = loader.pixbuf()
                    && let Some(display) = gtk::gdk::Display::default()
                {
                    display
                        .clipboard()
                        .set_texture(&gtk::gdk::Texture::for_pixbuf(&pixbuf));
                    status.show("Copied selection as PNG");
                }
            }
            Ok(None) => status.whisper("Select objects first"),
            Err(error) => status.show(error.to_string()),
        }
    });
    add_action(window, "align-left", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.align_selection(AlignMode::Left);
            status.whisper(format!("Aligned {count} object(s)"));
        }
    });
    add_action(window, "align-center", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.align_selection(AlignMode::CenterX);
            status.whisper(format!("Aligned {count} object(s)"));
        }
    });
    add_action(window, "align-right", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.align_selection(AlignMode::Right);
            status.whisper(format!("Aligned {count} object(s)"));
        }
    });
    add_action(window, "distribute-x", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.align_selection(AlignMode::DistributeX);
            status.whisper(format!("Distributed {count} object(s)"));
        }
    });
    add_action(window, "same-width", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let count = canvas.align_selection(AlignMode::SameWidth);
            status.whisper(format!("Matched width on {count} object(s)"));
        }
    });
    add_action(window, "copy-page-link", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let link = canvas.copy_page_link();
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&link);
            }
            status.show(format!("Copied {link}"));
        }
    });
    add_action(window, "insert-page-link", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            canvas.insert_page_link();
            status.show("Inserted a [[Page]] link. Click the canvas with Text to place it.");
        }
    });
    add_action(window, "export-folder", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export_folder(&window, &canvas, &status, false)
    });
    add_action(window, "export-layers", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_export_folder(&window, &canvas, &status, true)
    });
    add_action(window, "import-pdf-pages", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move || choose_pdf_pages(&window, &canvas, &status, &navigator)
    });
    add_action(window, "settings", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        let split_view = split_view.clone();
        let chrome = chrome.clone();
        let tools_toggle = tools_toggle.clone();
        move || {
            present_settings(
                &window,
                &canvas,
                &status,
                &navigator,
                &split_view,
                &chrome,
                &tools_toggle,
            );
        }
    });
    add_action(window, "about", {
        let window = window.clone();
        move || {
            let dialog = adw::AboutDialog::builder()
                .application_name("Inkstone")
                .developer_name("Inkstone")
                .version(env!("CARGO_PKG_VERSION"))
                .comments("A native, local-first canvas notebook for Linux.")
                .license_type(gtk::License::MitX11)
                .build();
            dialog.present(Some(&window));
        }
    });

    application.set_accels_for_action("win.new", &["<primary>n"]);
}

fn install_shortcuts(application: &adw::Application) {
    for (action, accelerators) in [
        ("win.focus-search", &["<primary>f"][..]),
        ("win.toggle-sidebar", &["F9"][..]),
        ("win.toggle-chrome", &["F10"][..]),
        ("win.fullscreen", &["F11"][..]),
        ("win.open", &["<primary>o"][..]),
        ("win.save", &["<primary>s"][..]),
        ("win.save-as", &["<primary><shift>s"][..]),
        ("win.import-media", &["<primary>i"][..]),
        ("win.export-svg", &["<primary>e"][..]),
        ("win.export-pdf", &["<primary><shift>e"][..]),
        ("win.export-png", &["<primary><shift>p"][..]),
        ("win.print", &["<primary>p"][..]),
        ("win.undo", &["<primary>z"][..]),
        ("win.redo", &["<primary><shift>z", "<primary>y"][..]),
        ("win.cut", &["<primary>x"][..]),
        ("win.copy", &["<primary>c"][..]),
        ("win.paste", &["<primary>v"][..]),
        ("win.duplicate", &["<primary>d"][..]),
        ("win.copy", &["<primary>c"][..]),
        ("win.cut", &["<primary>x"][..]),
        ("win.paste", &["<primary>v"][..]),
        ("win.bring-front", &["<primary>bracketright"][..]),
        ("win.send-back", &["<primary>bracketleft"][..]),
        ("win.rotate", &["<primary>r"][..]),
        ("win.delete", &["Delete", "BackSpace"][..]),
        ("win.previous-page", &["<alt>Left"][..]),
        ("win.next-page", &["<alt>Right"][..]),
        ("win.zoom-in", &["<primary>plus", "<primary>equal"][..]),
        ("win.zoom-out", &["<primary>minus"][..]),
        ("win.reset-view", &["<primary>0"][..]),
        ("win.replay", &["<primary><shift>r"][..]),
        ("win.settings", &["<primary>comma"][..]),
    ] {
        application.set_accels_for_action(action, accelerators);
    }
}

fn persist_prefs(session: &Rc<RefCell<Session>>, edit: impl FnOnce(&mut Preferences)) {
    let _ = session.borrow_mut().save_preferences(edit);
}

fn persist_workspace_preferences(
    session: &Rc<RefCell<Session>>,
    split_view: &adw::OverlaySplitView,
    tools_toggle: &gtk::ToggleButton,
    chrome: &WorkspaceChrome,
) {
    split_view.connect_notify_local(Some("show-sidebar"), {
        let session = session.clone();
        move |split, _| {
            persist_prefs(&session, |prefs| prefs.show_sidebar = split.shows_sidebar());
        }
    });
    tools_toggle.connect_toggled({
        let session = session.clone();
        move |button| persist_prefs(&session, |prefs| prefs.show_chrome = button.is_active())
    });
    chrome.show_colors.connect_notify_local(Some("state"), {
        let session = session.clone();
        let wanted = chrome.colors_wanted.clone();
        move |_, _| persist_prefs(&session, |prefs| prefs.show_colors = wanted.get())
    });
    chrome.show_widths.connect_notify_local(Some("state"), {
        let session = session.clone();
        let wanted = chrome.widths_wanted.clone();
        move |_, _| persist_prefs(&session, |prefs| prefs.show_widths = wanted.get())
    });
}

fn apply_session_to_canvas(canvas: &Canvas, prefs: &Preferences) {
    canvas.apply_ink_preferences(
        prefs.stabilizer,
        prefs.ignore_touch,
        prefs.ink_to_shape,
        prefs.ruler,
        prefs.replay_speed,
    );
    canvas.set_runtime_prefs(runtime_from_prefs(prefs));
    canvas.set_page_defaults(PageDefaults {
        paper: prefs.default_paper,
        pattern: prefs.default_pattern,
        grid_mm: prefs.default_grid_mm,
        night: prefs.night_paper_on_new_pages,
        layout: prefs.default_layout,
        template: prefs.default_template,
    });
}

fn apply_session_style(canvas: &Canvas, prefs: &Preferences) {
    canvas.apply_style_defaults(
        prefs.default_tool,
        prefs.default_color,
        prefs.default_width_mm,
        prefs.default_dashed,
        prefs.default_fill,
        prefs.default_shape,
        prefs.default_font_size,
        prefs.default_list,
        prefs.default_bold,
    );
}

fn runtime_from_prefs(prefs: &Preferences) -> RuntimePrefs {
    let motion = !prefs.reduce_motion;
    RuntimePrefs {
        stabilizer_strength: prefs.stabilizer_strength,
        palm_reject_ms: prefs.palm_reject_ms,
        use_tilt: prefs.use_tilt,
        use_pressure: prefs.use_pressure,
        barrel_eraser: prefs.barrel_eraser,
        highlighter_alpha: prefs.highlighter_alpha,
        highlighter_width_scale: prefs.highlighter_width_scale,
        brush_width_scale: prefs.brush_width_scale,
        eraser_scale: prefs.eraser_scale,
        iso_angle_snap: prefs.iso_angle_snap,
        tool_shortcuts: prefs.tool_shortcuts,
        min_zoom: prefs.min_zoom,
        max_zoom: prefs.max_zoom.max(prefs.min_zoom + 0.1),
        animate_zoom: prefs.animate_zoom && motion,
        show_empty_hint: prefs.show_empty_hint && motion,
        page_fade: prefs.page_fade && motion,
        selection_flash: prefs.selection_flash && motion,
        autosave_ms: (prefs.autosave_seconds * 1000.0).round() as u64,
        jpeg_quality: prefs.jpeg_quality.clamp(40, 100) as i32,
        pdf_dpi: prefs.pdf_dpi.clamp(72, 300),
        table_cols: prefs.table_cols.clamp(1, 20),
        table_rows: prefs.table_rows.clamp(1, 30),
        date_stamp: prefs.date_stamp,
        insert_after_current: prefs.insert_after_current,
        zoom_step: prefs.zoom_step.max(1.05),
        startup_zoom: prefs.startup_zoom.clamp(prefs.min_zoom, prefs.max_zoom),
    }
}

fn apply_theme(theme: ThemePref) {
    let scheme = match theme {
        ThemePref::System => adw::ColorScheme::Default,
        ThemePref::Light => adw::ColorScheme::ForceLight,
        ThemePref::Dark => adw::ColorScheme::ForceDark,
    };
    adw::StyleManager::default().set_color_scheme(scheme);
}

fn apply_live_preferences(canvas: &Canvas, navigator: &Navigator, feedback: &Feedback) {
    let prefs = navigator.session.borrow().preferences.clone();
    apply_session_to_canvas(canvas, &prefs);
    apply_theme(prefs.theme);
    navigator.tabs.set_visible(prefs.show_open_tabs);
    feedback.toast_seconds.set(prefs.toast_seconds);
    navigator.watch_library(canvas);
}

fn pref_switch(
    title: &str,
    subtitle: &str,
    active: bool,
    changed: impl Fn(bool) + 'static,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .active(active)
        .build();
    row.connect_active_notify(move |row| changed(row.is_active()));
    row
}

fn pref_combo(
    title: &str,
    subtitle: &str,
    names: &[&str],
    selected: u32,
    changed: impl Fn(u32) + 'static,
) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .subtitle(subtitle)
        .model(&gtk::StringList::new(names))
        .selected(selected)
        .build();
    row.connect_selected_notify(move |row| changed(row.selected()));
    row
}

#[allow(clippy::too_many_arguments)]
fn pref_spin(
    title: &str,
    subtitle: &str,
    min: f64,
    max: f64,
    step: f64,
    digits: u32,
    value: f64,
    changed: impl Fn(f64) + 'static,
) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, step);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.set_digits(digits);
    row.set_value(value);
    row.connect_value_notify(move |row| changed(row.value()));
    row
}

fn combo_index<T: PartialEq>(all: &[T], value: &T) -> u32 {
    all.iter().position(|item| item == value).unwrap_or(0) as u32
}

fn apply_session_chrome(
    prefs: &Preferences,
    split_view: &adw::OverlaySplitView,
    tools_toggle: &gtk::ToggleButton,
    chrome: &WorkspaceChrome,
) {
    split_view.set_show_sidebar(prefs.show_sidebar);
    if tools_toggle.is_active() != prefs.show_chrome {
        tools_toggle.set_active(prefs.show_chrome);
    }
    chrome
        .show_colors
        .change_state(&prefs.show_colors.to_variant());
    chrome
        .show_widths
        .change_state(&prefs.show_widths.to_variant());
}

#[allow(clippy::too_many_arguments)]
fn present_settings(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
    split_view: &adw::OverlaySplitView,
    chrome: &WorkspaceChrome,
    tools_toggle: &gtk::ToggleButton,
) {
    let prefs = navigator.session.borrow().preferences.clone();
    let dialog = adw::PreferencesDialog::builder()
        .title("Settings")
        .search_enabled(true)
        .build();

    let general = adw::PreferencesPage::builder()
        .name("general")
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();
    let launch = adw::PreferencesGroup::builder()
        .title("On startup")
        .description("Sidebar and tools update immediately. The last-notebook option is used on the next launch.")
        .build();
    let restore = adw::SwitchRow::builder()
        .title("Open last notebook")
        .subtitle("Otherwise the first notebook in the library opens")
        .active(prefs.restore_last_notebook)
        .build();
    restore.connect_active_notify({
        let session = navigator.session.clone();
        move |row| {
            persist_prefs(&session, |prefs| {
                prefs.restore_last_notebook = row.is_active()
            })
        }
    });
    let sidebar = adw::SwitchRow::builder()
        .title("Show notebook sidebar")
        .subtitle("F9 still toggles it while you work")
        .active(split_view.shows_sidebar())
        .build();
    sidebar.connect_active_notify({
        let session = navigator.session.clone();
        let split_view = split_view.clone();
        move |row| {
            split_view.set_show_sidebar(row.is_active());
            persist_prefs(&session, |prefs| prefs.show_sidebar = row.is_active());
        }
    });
    let tools = adw::SwitchRow::builder()
        .title("Show drawing tools")
        .subtitle("F10 still hides the canvas chrome")
        .active(tools_toggle.is_active())
        .build();
    tools.connect_active_notify({
        let session = navigator.session.clone();
        let tools_toggle = tools_toggle.clone();
        move |row| {
            if tools_toggle.is_active() != row.is_active() {
                tools_toggle.set_active(row.is_active());
            }
            persist_prefs(&session, |prefs| prefs.show_chrome = row.is_active());
        }
    });
    let colors = adw::SwitchRow::builder()
        .title("Show colours")
        .active(chrome.colors_wanted.get())
        .build();
    colors.connect_active_notify({
        let session = navigator.session.clone();
        let chrome = chrome.clone();
        move |row| {
            chrome
                .show_colors
                .change_state(&row.is_active().to_variant());
            persist_prefs(&session, |prefs| prefs.show_colors = row.is_active());
        }
    });
    let widths = adw::SwitchRow::builder()
        .title("Show widths")
        .active(chrome.widths_wanted.get())
        .build();
    widths.connect_active_notify({
        let session = navigator.session.clone();
        let chrome = chrome.clone();
        move |row| {
            chrome
                .show_widths
                .change_state(&row.is_active().to_variant());
            persist_prefs(&session, |prefs| prefs.show_widths = row.is_active());
        }
    });
    launch.add(&restore);
    launch.add(&sidebar);
    launch.add(&tools);
    launch.add(&colors);
    launch.add(&widths);
    general.add(&launch);

    let appearance = adw::PreferencesGroup::builder().title("Appearance").build();
    appearance.add(&pref_combo(
        "Colour scheme",
        "Applies to the window chrome and dialogs",
        &ThemePref::NAMES,
        combo_index(&ThemePref::ALL, &prefs.theme),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.theme = ThemePref::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or_default();
                });
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    appearance.add(&pref_switch(
        "Reduce motion",
        "Skip zoom animation, page fade, empty-page hint, and selection flash",
        prefs.reduce_motion,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.reduce_motion = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    appearance.add(&pref_switch(
        "Remember window size",
        "Restore width and height on the next launch",
        prefs.remember_window,
        {
            let session = navigator.session.clone();
            move |active| persist_prefs(&session, |prefs| prefs.remember_window = active)
        },
    ));
    general.add(&appearance);

    let files_behaviour = adw::PreferencesGroup::builder().title("Saving").build();
    files_behaviour.add(&pref_switch(
        "Ask before discarding unsaved notes",
        "When a notebook cannot be saved on close",
        prefs.confirm_discard,
        {
            let session = navigator.session.clone();
            move |active| persist_prefs(&session, |prefs| prefs.confirm_discard = active)
        },
    ));
    files_behaviour.add(&pref_switch(
        "Save when switching notebooks",
        "Otherwise keep unsaved ink until you save",
        prefs.save_on_switch,
        {
            let session = navigator.session.clone();
            move |active| persist_prefs(&session, |prefs| prefs.save_on_switch = active)
        },
    ));
    files_behaviour.add(&pref_spin(
        "Autosave delay",
        "Seconds after ink. 0 turns autosave off",
        0.0,
        30.0,
        0.5,
        1,
        prefs.autosave_seconds as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.autosave_seconds = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    files_behaviour.add(&pref_spin(
        "Toast duration",
        "Seconds for confirmation toasts. 0 keeps them until dismissed",
        0.0,
        10.0,
        1.0,
        0,
        prefs.toast_seconds as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.toast_seconds = value as u32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    let reset = adw::ActionRow::builder()
        .title("Reset all settings")
        .subtitle("Keeps the library folder and last notebook")
        .activatable(true)
        .build();
    reset.add_suffix(&gtk::Image::from_icon_name("edit-undo-symbolic"));
    reset.connect_activated({
        let session = navigator.session.clone();
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let split_view = split_view.clone();
        let chrome = chrome.clone();
        let tools_toggle = tools_toggle.clone();
        let dialog = dialog.clone();
        let window = window.clone();
        move |_| {
            let library_root = session.borrow().preferences.library_root.clone();
            persist_prefs(&session, |prefs| {
                let last = library_root.clone();
                *prefs = Preferences::default();
                prefs.library_root = last;
            });
            apply_session_style(&canvas, &session.borrow().preferences);
            apply_live_preferences(&canvas, &navigator, &status);
            apply_session_chrome(
                &session.borrow().preferences,
                &split_view,
                &tools_toggle,
                &chrome,
            );
            status.whisper("Settings reset");
            dialog.close();
            present_settings(
                &window,
                &canvas,
                &status,
                &navigator,
                &split_view,
                &chrome,
                &tools_toggle,
            );
        }
    });
    files_behaviour.add(&reset);
    general.add(&files_behaviour);
    dialog.add(&general);

    let ink = adw::PreferencesPage::builder()
        .name("ink")
        .title("Ink")
        .icon_name("input-tablet-symbolic")
        .build();
    let tablet = adw::PreferencesGroup::builder()
        .title("Pen and touch")
        .description("These also live on the ink chip above the canvas.")
        .build();
    let lazy = adw::SwitchRow::builder()
        .title("Lazy ink")
        .subtitle("Smooth strokes by lagging slightly behind the pointer")
        .active(canvas.stabilizer())
        .build();
    lazy.connect_active_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            canvas.set_stabilizer(row.is_active());
            persist_prefs(&session, |prefs| prefs.stabilizer = row.is_active());
        }
    });
    let palm = adw::SwitchRow::builder()
        .title("Palm rejection")
        .subtitle("Ignore touch after the stylus has been down")
        .active(canvas.ignore_touch())
        .build();
    palm.connect_active_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            canvas.set_ignore_touch(row.is_active());
            persist_prefs(&session, |prefs| prefs.ignore_touch = row.is_active());
        }
    });
    let shape = adw::SwitchRow::builder()
        .title("Ink to shape")
        .subtitle("Turn tidy freehand strokes into lines, rectangles, or ellipses")
        .active(canvas.ink_to_shape())
        .build();
    shape.connect_active_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            canvas.set_ink_to_shape(row.is_active());
            persist_prefs(&session, |prefs| prefs.ink_to_shape = row.is_active());
        }
    });
    let ruler = adw::SwitchRow::builder()
        .title("Ruler")
        .subtitle("Keep freehand ink horizontal or vertical")
        .active(canvas.ruler())
        .build();
    ruler.connect_active_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            canvas.set_ruler(row.is_active());
            persist_prefs(&session, |prefs| prefs.ruler = row.is_active());
        }
    });
    let speed = adw::ComboRow::builder()
        .title("Ink replay speed")
        .subtitle("Used when you press Replay")
        .model(&gtk::StringList::new(&["0.5×", "1×", "2×", "4×"]))
        .selected(
            REPLAY_SPEEDS
                .iter()
                .position(|value| (*value - canvas.replay_status().speed).abs() < f32::EPSILON)
                .unwrap_or(1) as u32,
        )
        .build();
    speed.connect_selected_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            let speed = REPLAY_SPEEDS
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(1.0);
            canvas.set_replay_speed(speed);
            persist_prefs(&session, |prefs| prefs.replay_speed = speed);
        }
    });
    tablet.add(&lazy);
    tablet.add(&palm);
    tablet.add(&shape);
    tablet.add(&ruler);
    tablet.add(&speed);
    ink.add(&tablet);

    let feel = adw::PreferencesGroup::builder().title("Feel").build();
    feel.add(&pref_spin(
        "Lazy ink strength",
        "Higher lags more and smooths more",
        0.20,
        0.90,
        0.02,
        2,
        prefs.stabilizer_strength as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.stabilizer_strength = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_spin(
        "Palm rejection window",
        "Milliseconds to ignore touch after the stylus",
        0.0,
        2000.0,
        50.0,
        0,
        prefs.palm_reject_ms as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.palm_reject_ms = value as u64);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_switch(
        "Stylus pressure",
        "Use GTK pressure samples when the tablet reports them",
        prefs.use_pressure,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.use_pressure = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_switch(
        "Stylus tilt",
        "Widen pressure slightly when the pen is tilted",
        prefs.use_tilt,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.use_tilt = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_switch(
        "Barrel button as eraser",
        "Stylus button 2 erases. The physical eraser tip still works",
        prefs.barrel_eraser,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.barrel_eraser = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_switch(
        "ISO 15° snap with Shift",
        "Ink and lines snap to 15° while Shift is held",
        prefs.iso_angle_snap,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.iso_angle_snap = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    feel.add(&pref_switch(
        "Canvas tool keys",
        "V P H E T S M while the canvas has focus",
        prefs.tool_shortcuts,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.tool_shortcuts = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    ink.add(&feel);

    let strokes = adw::PreferencesGroup::builder()
        .title("Stroke multipliers")
        .build();
    strokes.add(&pref_spin(
        "Highlighter opacity",
        "Alpha of highlighter ink",
        0.10,
        0.80,
        0.05,
        2,
        prefs.highlighter_alpha as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.highlighter_alpha = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    strokes.add(&pref_spin(
        "Highlighter width",
        "Times the current width chip",
        2.0,
        12.0,
        0.5,
        1,
        prefs.highlighter_width_scale as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| {
                    prefs.highlighter_width_scale = value as f32
                });
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    strokes.add(&pref_spin(
        "Brush width",
        "Times the current width chip",
        1.0,
        4.0,
        0.1,
        1,
        prefs.brush_width_scale as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.brush_width_scale = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    strokes.add(&pref_spin(
        "Eraser size",
        "Times the current width chip",
        1.0,
        8.0,
        0.5,
        1,
        prefs.eraser_scale as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.eraser_scale = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    ink.add(&strokes);
    dialog.add(&ink);

    let pages = adw::PreferencesPage::builder()
        .name("pages")
        .title("Pages")
        .icon_name("document-page-setup-symbolic")
        .build();
    let page_defaults = adw::PreferencesGroup::builder()
        .title("New pages")
        .description("Applied when you add a page or create a notebook.")
        .build();
    let paper = adw::ComboRow::builder()
        .title("Paper")
        .subtitle("ISO 216")
        .model(&gtk::StringList::new(&PaperSize::NAMES))
        .selected(
            PaperSize::ALL
                .iter()
                .position(|value| *value == prefs.default_paper)
                .unwrap_or(0) as u32,
        )
        .build();
    paper.connect_selected_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            let size = PaperSize::ALL
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(PaperSize::Infinite);
            persist_prefs(&session, |prefs| prefs.default_paper = size);
            let mut defaults = canvas.page_defaults();
            defaults.paper = size;
            canvas.set_page_defaults(defaults);
        }
    });
    let pattern = adw::ComboRow::builder()
        .title("Background")
        .model(&gtk::StringList::new(&BackgroundPattern::NAMES))
        .selected(
            BackgroundPattern::ALL
                .iter()
                .position(|value| *value == prefs.default_pattern)
                .unwrap_or(1) as u32,
        )
        .build();
    pattern.connect_selected_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            let value = BackgroundPattern::ALL
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(BackgroundPattern::Grid);
            persist_prefs(&session, |prefs| prefs.default_pattern = value);
            let mut defaults = canvas.page_defaults();
            defaults.pattern = value;
            canvas.set_page_defaults(defaults);
        }
    });
    let grid = adw::SpinRow::with_range(1.0, 50.0, 1.0);
    grid.set_title("Grid spacing");
    grid.set_subtitle("Millimetres");
    grid.set_digits(0);
    grid.set_value(prefs.default_grid_mm as f64);
    grid.connect_value_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            let mm = row.value() as f32;
            persist_prefs(&session, |prefs| prefs.default_grid_mm = mm);
            let mut defaults = canvas.page_defaults();
            defaults.grid_mm = mm;
            canvas.set_page_defaults(defaults);
        }
    });
    let night = adw::SwitchRow::builder()
        .title("Night paper on new pages")
        .subtitle("Dark sheet, same as View → Night Paper")
        .active(prefs.night_paper_on_new_pages)
        .build();
    night.connect_active_notify({
        let canvas = canvas.clone();
        let session = navigator.session.clone();
        move |row| {
            persist_prefs(&session, |prefs| {
                prefs.night_paper_on_new_pages = row.is_active();
            });
            let mut defaults = canvas.page_defaults();
            defaults.night = row.is_active();
            canvas.set_page_defaults(defaults);
        }
    });
    let apply = adw::ActionRow::builder()
        .title("Apply defaults to this page")
        .activatable(true)
        .build();
    apply.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    apply.connect_activated({
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        move |_| {
            canvas.apply_page_defaults();
            navigator.refresh(&canvas);
            status.whisper("Applied page defaults");
        }
    });
    page_defaults.add(&paper);
    page_defaults.add(&pattern);
    page_defaults.add(&grid);
    page_defaults.add(&night);
    page_defaults.add(&apply);
    page_defaults.add(&pref_combo(
        "Layout",
        "How new pages scroll",
        &PageLayout::NAMES,
        combo_index(&PageLayout::ALL, &prefs.default_layout),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_layout = PageLayout::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or_default();
                });
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    page_defaults.add(&pref_combo(
        "Template",
        "Applied to new pages. Blank leaves the page empty",
        &PageTemplate::NAMES,
        combo_index(&PageTemplate::ALL, &prefs.default_template),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_template = PageTemplate::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or_default();
                });
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    page_defaults.add(&pref_switch(
        "Insert new page after the current one",
        "Otherwise append at the end of the notebook",
        prefs.insert_after_current,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.insert_after_current = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    pages.add(&page_defaults);
    dialog.add(&pages);

    let drawing = adw::PreferencesPage::builder()
        .name("drawing")
        .title("Drawing")
        .icon_name("document-edit-symbolic")
        .build();
    let defaults = adw::PreferencesGroup::builder()
        .title("Defaults")
        .description("Applied at launch and when you change them here.")
        .build();
    let color_names: Vec<&str> = INK_COLOR_CHOICES.iter().map(|(name, _)| *name).collect();
    let color_selected = INK_COLOR_CHOICES
        .iter()
        .position(|(_, color)| *color == prefs.default_color)
        .unwrap_or(0) as u32;
    defaults.add(&pref_combo(
        "Tool",
        "Active tool when Inkstone starts",
        &Tool::NAMES,
        combo_index(&Tool::ALL, &prefs.default_tool),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_tool = Tool::ALL.get(index as usize).copied().unwrap_or_default();
                });
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    defaults.add(&pref_combo(
        "Colour",
        "Ink colour at launch",
        &color_names,
        color_selected,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_color = INK_COLOR_CHOICES
                        .get(index as usize)
                        .map(|(_, color)| *color)
                        .unwrap_or(Color::INK);
                });
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    defaults.add(&pref_spin(
        "Stroke width",
        "Millimetres (ISO 128)",
        0.13,
        8.0,
        0.05,
        2,
        prefs.default_width_mm as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.default_width_mm = value as f32);
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    defaults.add(&pref_combo(
        "Shape",
        "Default geometry and IEC/ISO symbol",
        &ShapeKind::NAMES,
        combo_index(&ShapeKind::ALL, &prefs.default_shape),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_shape = ShapeKind::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or(ShapeKind::Rectangle);
                });
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    defaults.add(&pref_switch(
        "Dashed strokes",
        "Pen, highlighter, and shapes start dashed",
        prefs.default_dashed,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.default_dashed = active);
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    defaults.add(&pref_switch(
        "Fill shapes",
        "New rectangles and ellipses are filled",
        prefs.default_fill,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.default_fill = active);
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    drawing.add(&defaults);
    dialog.add(&drawing);

    let text = adw::PreferencesPage::builder()
        .name("text")
        .title("Text")
        .icon_name("insert-text-symbolic")
        .build();
    let typewriter = adw::PreferencesGroup::builder().title("Typewriter").build();
    typewriter.add(&pref_spin(
        "Font size",
        "Points for new notes",
        8.0,
        72.0,
        1.0,
        0,
        prefs.default_font_size as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.default_font_size = value as f32);
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    typewriter.add(&pref_combo(
        "List style",
        "New notes start as this list kind",
        &ListStyle::NAMES,
        combo_index(&ListStyle::ALL, &prefs.default_list),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.default_list = ListStyle::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or_default();
                });
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    typewriter.add(&pref_switch(
        "Bold new notes",
        "Default typewriter weight",
        prefs.default_bold,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.default_bold = active);
                apply_session_style(&canvas, &session.borrow().preferences);
            }
        },
    ));
    typewriter.add(&pref_combo(
        "Date stamp",
        "Insert → Date and Time",
        &DateStamp::NAMES,
        combo_index(&DateStamp::ALL, &prefs.date_stamp),
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |index| {
                persist_prefs(&session, |prefs| {
                    prefs.date_stamp = DateStamp::ALL
                        .get(index as usize)
                        .copied()
                        .unwrap_or_default();
                });
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    typewriter.add(&pref_spin(
        "Table columns",
        "Insert → Table",
        1.0,
        12.0,
        1.0,
        0,
        prefs.table_cols as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.table_cols = value as u32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    typewriter.add(&pref_spin(
        "Table rows",
        "Insert → Table",
        1.0,
        20.0,
        1.0,
        0,
        prefs.table_rows as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.table_rows = value as u32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    text.add(&typewriter);
    dialog.add(&text);

    let view = adw::PreferencesPage::builder()
        .name("view")
        .title("View")
        .icon_name("zoom-fit-best-symbolic")
        .build();
    let zoom = adw::PreferencesGroup::builder().title("Zoom").build();
    zoom.add(&pref_spin(
        "Startup zoom",
        "1.00 is 100%",
        0.25,
        4.0,
        0.05,
        2,
        prefs.startup_zoom as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.startup_zoom = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    zoom.add(&pref_spin(
        "Zoom step",
        "Ctrl++ and Ctrl+− multiply by this",
        1.05,
        2.0,
        0.05,
        2,
        prefs.zoom_step as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.zoom_step = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    zoom.add(&pref_spin(
        "Minimum zoom",
        "How far you can zoom out",
        0.05,
        1.0,
        0.01,
        2,
        prefs.min_zoom as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.min_zoom = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    zoom.add(&pref_spin(
        "Maximum zoom",
        "How far you can zoom in",
        2.0,
        32.0,
        1.0,
        0,
        prefs.max_zoom as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.max_zoom = value as f32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    zoom.add(&pref_switch(
        "Animate zoom",
        "Also respects the desktop reduce-motion setting",
        prefs.animate_zoom,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.animate_zoom = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    view.add(&zoom);
    let chrome_extra = adw::PreferencesGroup::builder().title("Workspace").build();
    chrome_extra.add(&pref_switch(
        "Show open-notebook tabs",
        "Under the current notebook name in the sidebar",
        prefs.show_open_tabs,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.show_open_tabs = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    chrome_extra.add(&pref_switch(
        "Empty-page hint",
        "Start a page fades in on blank sheets",
        prefs.show_empty_hint,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.show_empty_hint = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    chrome_extra.add(&pref_switch(
        "Page fade",
        "Brief fade when changing pages",
        prefs.page_fade,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.page_fade = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    chrome_extra.add(&pref_switch(
        "Selection flash",
        "Highlight a hit after Find",
        prefs.selection_flash,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.selection_flash = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    view.add(&chrome_extra);
    dialog.add(&view);

    let library_page = adw::PreferencesPage::builder()
        .name("library")
        .title("Library")
        .icon_name("folder-symbolic")
        .build();
    let files = adw::PreferencesGroup::builder()
        .title("Local files")
        .description("Folders are categories. Each .inkstone file is one notebook.")
        .build();
    let root_label = navigator
        .library
        .borrow()
        .root
        .to_string_lossy()
        .to_string();
    let library_row = adw::ActionRow::builder()
        .title("Notebook library")
        .subtitle(&root_label)
        .build();
    if env_library_root().is_some() {
        library_row.set_subtitle(&format!(
            "{root_label} — INKSTONE_LIBRARY overrides the saved folder"
        ));
    }
    let choose = gtk::Button::builder()
        .label("Choose")
        .valign(gtk::Align::Center)
        .sensitive(env_library_root().is_none())
        .build();
    choose.add_css_class("flat");
    choose.connect_clicked({
        let window = window.clone();
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let library_row = library_row.clone();
        move |_| choose_library_folder(&window, &canvas, &navigator, &status, &library_row)
    });
    library_row.add_suffix(&choose);
    let show = gtk::Button::builder()
        .label("Show")
        .valign(gtk::Align::Center)
        .action_name("win.show-library")
        .build();
    show.add_css_class("flat");
    library_row.add_suffix(&show);
    let settings_path = config_dir().join("session.json").display().to_string();
    let config = adw::ActionRow::builder()
        .title("Settings file")
        .subtitle(&settings_path)
        .build();
    let open_config = gtk::Button::builder()
        .label("Open")
        .valign(gtk::Align::Center)
        .build();
    open_config.add_css_class("flat");
    open_config.connect_clicked(|_| {
        let _ = std::process::Command::new("xdg-open")
            .arg(config_dir())
            .spawn();
    });
    config.add_suffix(&open_config);
    files.add(&library_row);
    files.add(&config);
    library_page.add(&files);

    let sync = adw::PreferencesGroup::builder()
        .title("Library behaviour")
        .build();
    sync.add(&pref_switch(
        "Watch the library folder",
        "Refresh when Files adds or moves notebooks",
        prefs.watch_library,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |active| {
                persist_prefs(&session, |prefs| prefs.watch_library = active);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    sync.add(&pref_spin(
        "Search hit limit",
        "Library-wide search stops after this many matches",
        10.0,
        200.0,
        10.0,
        0,
        prefs.search_limit as f64,
        {
            let session = navigator.session.clone();
            move |value| persist_prefs(&session, |prefs| prefs.search_limit = value as u32)
        },
    ));
    library_page.add(&sync);

    let export = adw::PreferencesGroup::builder()
        .title("Export and import")
        .build();
    export.add(&pref_spin(
        "JPEG quality",
        "Export JPEG",
        40.0,
        100.0,
        1.0,
        0,
        prefs.jpeg_quality as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.jpeg_quality = value as u32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    export.add(&pref_spin(
        "PDF import DPI",
        "Rasterise PDF pages with pdftoppm or Ghostscript",
        72.0,
        300.0,
        12.0,
        0,
        prefs.pdf_dpi as f64,
        {
            let session = navigator.session.clone();
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let status = status.clone();
            move |value| {
                persist_prefs(&session, |prefs| prefs.pdf_dpi = value as u32);
                apply_live_preferences(&canvas, &navigator, &status);
            }
        },
    ));
    library_page.add(&export);
    dialog.add(&library_page);

    dialog.present(Some(window));
}

fn choose_library_folder(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    navigator: &Navigator,
    status: &Feedback,
    library_row: &adw::ActionRow,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Choose library folder")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::SelectFolder)
        .accept_label("Use")
        .cancel_label("Cancel")
        .build();
    let _ =
        chooser.set_current_folder(Some(&gio::File::for_path(&navigator.library.borrow().root)));
    chooser.connect_response({
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let library_row = library_row.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                match Library::open(&path) {
                    Ok(library) => {
                        persist_prefs(&navigator.session, |prefs| {
                            prefs.library_root = Some(path.clone());
                        });
                        *navigator.library.borrow_mut() = library;
                        navigator.watch_library(&canvas);
                        navigator.refresh_library(&canvas);
                        library_row.set_subtitle(&path.to_string_lossy());
                        status.show("Library folder updated");
                    }
                    Err(error) => status.show(error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn add_action(window: &adw::ApplicationWindow, name: &str, callback: impl Fn() + 'static) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| callback());
    window.add_action(&action);
}

fn prompt_new_notebook(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    navigator: &Navigator,
    status: &Feedback,
) {
    if canvas.is_dirty() {
        let _ = canvas.save_current();
    }
    let paths = navigator.library.borrow().category_paths();
    let labels: Vec<String> = paths.iter().map(|(label, _)| label.clone()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let categories = gtk::DropDown::from_strings(&refs);
    let default = canvas
        .current_path()
        .as_deref()
        .and_then(Path::parent)
        .and_then(|parent| paths.iter().position(|(_, path)| path == parent))
        .or_else(|| paths.iter().position(|(label, _)| label == "Personal"))
        .unwrap_or(0);
    categories.set_selected(default as u32);
    categories.set_tooltip_text(Some("Category folder in Files"));
    let name = gtk::Entry::builder()
        .placeholder_text("Notebook name")
        .text("Notebook")
        .hexpand(true)
        .build();
    let extra = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    extra.append(&name);
    extra.append(&categories);
    let dialog = adw::AlertDialog::new(
        Some("New notebook"),
        Some(
            "Inkstone saves it as a .inkstone file in that folder, the same place your file manager shows.",
        ),
    );
    dialog.set_extra_child(Some(&extra));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", "Create");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, {
        let window = window.clone();
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let name = name.clone();
        let categories = categories.clone();
        move |_, response| {
            if response != "create" {
                return;
            }
            let title = name.text();
            let index = categories.selected() as usize;
            let folder = paths
                .get(index)
                .map(|(_, path)| path.clone())
                .unwrap_or_else(|| navigator.library.borrow().root.clone());
            let created = navigator.library.borrow().create_notebook(&folder, &title);
            match created {
                Ok(path) => {
                    if canvas.is_dirty() {
                        let _ = canvas.save_current();
                    }
                    match canvas.load(&path) {
                        Ok(()) => {
                            canvas.apply_page_defaults();
                            let _ = canvas.save_current();
                            let _ = navigator.session.borrow_mut().remember(&path);
                            navigator.refresh(&canvas);
                            navigator.refresh_library(&canvas);
                            refresh_status(&window, &canvas, &status, "Created notebook");
                        }
                        Err(error) => show_error(&window, &error.to_string()),
                    }
                }
                Err(error) => show_error(&window, &error.to_string()),
            }
        }
    });
    dialog.present(Some(window));
}

fn prompt_new_category(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    navigator: &Navigator,
    status: &Feedback,
) {
    let paths = navigator.library.borrow().category_paths();
    let labels: Vec<String> = paths.iter().map(|(label, _)| label.clone()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let parents = gtk::DropDown::from_strings(&refs);
    let default = canvas
        .current_path()
        .as_deref()
        .and_then(Path::parent)
        .and_then(|parent| paths.iter().position(|(_, path)| path == parent))
        .or_else(|| paths.iter().position(|(label, _)| label == "Personal"))
        .unwrap_or(0);
    parents.set_selected(default as u32);
    parents.set_tooltip_text(Some("Create inside this folder"));
    let name = gtk::Entry::builder()
        .placeholder_text("Category name")
        .text("Projects")
        .hexpand(true)
        .build();
    let extra = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    extra.append(&name);
    extra.append(&parents);
    let dialog = adw::AlertDialog::new(
        Some("New category"),
        Some("A folder in your Inkstone library. Nest as many as you want."),
    );
    dialog.set_extra_child(Some(&extra));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", "Create");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, {
        let window = window.clone();
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let name = name.clone();
        let parents = parents.clone();
        move |_, response| {
            if response != "create" {
                return;
            }
            let title = name.text();
            let index = parents.selected() as usize;
            let folder = paths
                .get(index)
                .map(|(_, path)| path.clone())
                .unwrap_or_else(|| navigator.library.borrow().root.clone());
            let created = navigator.library.borrow().create_category(&folder, &title);
            match created {
                Ok(_) => {
                    navigator.refresh_library(&canvas);
                    status.show("Created category folder");
                }
                Err(error) => show_error(&window, &error.to_string()),
            }
        }
    });
    dialog.present(Some(window));
}

fn prompt_move_notebook(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    navigator: &Navigator,
    status: &Feedback,
) {
    if canvas.current_path().is_none() {
        status.whisper("Save this notebook first");
        return;
    }
    if canvas.is_dirty() {
        let _ = canvas.save_current();
    }
    let paths = navigator.library.borrow().category_paths();
    let labels: Vec<String> = paths.iter().map(|(label, _)| label.clone()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let categories = gtk::DropDown::from_strings(&refs);
    let default = canvas
        .current_path()
        .as_deref()
        .and_then(Path::parent)
        .and_then(|parent| paths.iter().position(|(_, path)| path == parent))
        .unwrap_or(0);
    categories.set_selected(default as u32);
    let dialog = adw::AlertDialog::new(
        Some("Move notebook"),
        Some(
            "Choose a category folder. Inkstone moves the .inkstone file there so Files stays in sync.",
        ),
    );
    dialog.set_extra_child(Some(&categories));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("move", "Move");
    dialog.set_response_appearance("move", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("move"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, {
        let window = window.clone();
        let canvas = canvas.clone();
        let navigator = navigator.clone();
        let status = status.clone();
        let categories = categories.clone();
        move |_, response| {
            if response != "move" {
                return;
            }
            let index = categories.selected() as usize;
            let folder = paths
                .get(index)
                .map(|(_, path)| path.clone())
                .unwrap_or_else(|| navigator.library.borrow().root.clone());
            match canvas.move_current_notebook(&folder) {
                Ok(path) => {
                    let _ = navigator.session.borrow_mut().remember(&path);
                    navigator.refresh(&canvas);
                    navigator.refresh_library(&canvas);
                    refresh_status(&window, &canvas, &status, "Moved notebook");
                }
                Err(error) => show_error(&window, &error.to_string()),
            }
        }
    });
    dialog.present(Some(window));
}

fn choose_open(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Open Inkstone document")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .accept_label("Open")
        .cancel_label("Cancel")
        .build();
    add_document_filter(&chooser);
    let library_root = navigator.library.borrow().root.clone();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(&library_root)));
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                if canvas.is_dirty() {
                    let _ = canvas.save_current();
                }
                match canvas.load(&path) {
                    Ok(()) => {
                        let _ = navigator.session.borrow_mut().remember(&path);
                        navigator.refresh(&canvas);
                        navigator.refresh_library(&canvas);
                        refresh_status(&window, &canvas, &status, "Opened");
                    }
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_save(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Save Inkstone document")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Save)
        .accept_label("Save")
        .cancel_label("Cancel")
        .build();
    chooser.set_current_name("untitled.inkstone");
    add_document_filter(&chooser);
    let library_root = navigator.library.borrow().root.clone();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(&library_root)));
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let path = with_extension(path, "inkstone");
                match canvas.save(&path) {
                    Ok(()) => {
                        let _ = navigator.session.borrow_mut().remember(&path);
                        navigator.refresh(&canvas);
                        navigator.refresh_library(&canvas);
                        refresh_status(&window, &canvas, &status, "Saved");
                    }
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_export(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Export SVG")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Save)
        .accept_label("Export")
        .cancel_label("Cancel")
        .build();
    chooser.set_current_name("inkstone-export.svg");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Scalable Vector Graphics"));
    filter.add_pattern("*.svg");
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let path = with_extension(path, "svg");
                match canvas.export_svg(&path) {
                    Ok(()) => refresh_status(&window, &canvas, &status, "Exported SVG"),
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_export_pdf(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Export notebook as PDF")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Save)
        .accept_label("Export")
        .cancel_label("Cancel")
        .build();
    chooser.set_current_name("inkstone-notebook.pdf");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Portable Document Format"));
    filter.add_pattern("*.pdf");
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let path = with_extension(path, "pdf");
                match canvas.export_pdf(&path) {
                    Ok(()) => refresh_status(&window, &canvas, &status, "Exported PDF"),
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_export_image(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    extension: &'static str,
    title: &'static str,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(format!("Export {title}"))
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Save)
        .accept_label("Export")
        .cancel_label("Cancel")
        .build();
    chooser.set_current_name(&format!("inkstone-export.{extension}"));
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(title));
    filter.add_pattern(&format!("*.{extension}"));
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let path = with_extension(path, extension);
                let result = if extension == "png" {
                    canvas.export_png(&path)
                } else {
                    canvas.export_jpeg(&path)
                };
                match result {
                    Ok(()) => refresh_status(
                        &window,
                        &canvas,
                        &status,
                        &format!("Exported {}", extension.to_ascii_uppercase()),
                    ),
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_export_folder(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    layers_only: bool,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(if layers_only {
            "Export layers"
        } else {
            "Export notebook folder"
        })
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::SelectFolder)
        .accept_label("Export")
        .cancel_label("Cancel")
        .build();
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let result = if layers_only {
                    canvas
                        .export_layers_folder(&path)
                        .map(|count| format!("Exported {count} layer SVG file(s)"))
                } else {
                    canvas
                        .export_notebook_folder(&path)
                        .map(|()| "Exported PDF pages as SVG and PNG".to_owned())
                };
                match result {
                    Ok(message) => refresh_status(&window, &canvas, &status, &message),
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn choose_pdf_pages(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Open PDF as ink pages")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .accept_label("Open")
        .cancel_label("Cancel")
        .build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("PDF"));
    filter.add_pattern("*.pdf");
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                match canvas.import_pdf_as_pages(&path) {
                    Ok(count) => {
                        navigator.refresh(&canvas);
                        refresh_status(
                            &window,
                            &canvas,
                            &status,
                            &format!("Opened {count} PDF page(s) you can ink on"),
                        );
                    }
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn print_notebook(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let operation = gtk::PrintOperation::new();
    operation.set_n_pages(canvas.page_count() as i32);
    operation.set_embed_page_setup(true);
    operation.set_job_name("Inkstone notebook");
    operation.connect_draw_page({
        let canvas = canvas.clone();
        move |_, context, page| {
            let cairo = context.cairo_context();
            if let Err(error) =
                canvas.render_page_to(&cairo, page as usize, context.width(), context.height())
            {
                eprintln!("print page {page}: {error}");
            }
        }
    });
    match operation.run(gtk::PrintOperationAction::PrintDialog, Some(window)) {
        Ok(gtk::PrintOperationResult::Apply | gtk::PrintOperationResult::InProgress) => {
            status.show("Sent to printer");
        }
        Ok(_) => {}
        Err(error) => show_error(window, &error.to_string()),
    }
}

fn choose_import(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Import image, PDF, SVG, audio, or file")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .accept_label("Import")
        .cancel_label("Cancel")
        .build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Notes attachments"));
    for pattern in [
        "*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.pdf", "*.svg", "*.mp3", "*.wav", "*.ogg",
        "*.flac", "*.m4a", "*.txt", "*.zip",
    ] {
        filter.add_pattern(pattern);
    }
    chooser.add_filter(&filter);
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    chooser.add_filter(&all);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                match canvas.import_path(&path) {
                    Ok(()) => refresh_status(&window, &canvas, &status, "Imported media"),
                    Err(error) => show_error(&window, &error.to_string()),
                }
            }
            chooser.destroy();
        }
    });
    chooser.show();
}

fn add_document_filter(chooser: &gtk::FileChooserNative) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Inkstone documents"));
    filter.add_pattern("*.inkstone");
    filter.add_mime_type("application/json");
    chooser.add_filter(&filter);
}

fn with_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

fn refresh_status(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    message: &str,
) {
    apply_window_title(window, canvas, status);
    status.show(format!(
        "{message} • {} objects • {}%",
        canvas.element_count(),
        canvas.zoom_percent()
    ));
}

fn refresh_status_quiet(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    message: &str,
) {
    apply_window_title(window, canvas, status);
    status.whisper(format!(
        "{message} • {} objects • {}%",
        canvas.element_count(),
        canvas.zoom_percent()
    ));
}

fn apply_window_title(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let name = canvas
        .current_path()
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| canvas.document_title());
    let dirty = if canvas.is_dirty() {
        " • modified"
    } else {
        ""
    };
    window.set_title(Some(&format!("Inkstone — {name}{dirty}")));
    status.title.set_subtitle(&format!("{name}{dirty}"));
}

fn confirm_discard(window: &adw::ApplicationWindow, on_discard: impl Fn() + 'static) {
    confirm_discard_maybe(window, true, on_discard);
}

fn confirm_discard_maybe(
    window: &adw::ApplicationWindow,
    ask: bool,
    on_discard: impl Fn() + 'static,
) {
    if !ask {
        on_discard();
        return;
    }
    let dialog = adw::AlertDialog::new(
        Some("Discard unsaved changes?"),
        Some("The current note has changes that have not been saved."),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("discard", "Discard");
    dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response == "discard" {
            on_discard();
        }
    });
    dialog.present(Some(window));
}

fn protect_unsaved_close(
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    session: &Rc<RefCell<Session>>,
) {
    window.connect_close_request({
        let window = window.clone();
        let canvas = canvas.clone();
        let session = session.clone();
        move |_| {
            persist_prefs(&session, |prefs| {
                prefs.window_width = window.width().max(760);
                prefs.window_height = window.height().max(520);
            });
            if !canvas.is_dirty() {
                return glib::Propagation::Proceed;
            }
            match canvas.save_current() {
                Ok(true) => glib::Propagation::Proceed,
                Ok(false) => {
                    confirm_discard_maybe(&window, session.borrow().preferences.confirm_discard, {
                        let window = window.clone();
                        move || window.destroy()
                    });
                    glib::Propagation::Stop
                }
                Err(_) => {
                    confirm_discard_maybe(&window, session.borrow().preferences.confirm_discard, {
                        let window = window.clone();
                        move || window.destroy()
                    });
                    glib::Propagation::Stop
                }
            }
        }
    });
}

fn show_error(window: &adw::ApplicationWindow, message: &str) {
    let dialog = adw::AlertDialog::new(
        Some("Inkstone could not complete the operation"),
        Some(message),
    );
    dialog.add_response("close", "Close");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");
    dialog.present(Some(window));
}
