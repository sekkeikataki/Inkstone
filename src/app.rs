use crate::canvas::{Canvas, Tool};
use crate::document::{Color, ShapeKind};
use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk4 as gtk;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

const APP_ID: &str = "dev.inkstone.Inkstone";

#[derive(Clone)]
struct Navigator {
    sidebar: gtk::ScrolledWindow,
    search: gtk::SearchEntry,
    page_list: gtk::ListBox,
    layer_list: gtk::ListBox,
    grid: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct Feedback {
    status: gtk::Label,
    zoom: gtk::Label,
    toasts: adw::ToastOverlay,
    title: adw::WindowTitle,
}

impl Feedback {
    fn show(&self, message: impl AsRef<str>) {
        let message = message.as_ref();
        self.status.set_text(message);
        let toast = adw::Toast::new(message);
        toast.set_timeout(2);
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
            transition: 160ms cubic-bezier(0.22, 1, 0.36, 1);
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
        .floating-panel {
            background-color: alpha(@window_bg_color, 0.88);
            border: 1px solid alpha(@borders, 0.42);
            box-shadow: 0 14px 36px alpha(black, 0.14);
            transition: 180ms cubic-bezier(0.22, 1, 0.36, 1);
        }
        .floating-panel:hover {
            border-color: alpha(@accent_bg_color, 0.4);
            box-shadow: 0 16px 40px alpha(black, 0.16);
        }
        .tool-palette {
            border-radius: 28px;
            padding: 8px;
        }
        .tool-options {
            border-radius: 22px;
            padding: 6px 12px;
            min-height: 48px;
        }
        .tool-button {
            border-radius: 999px;
            min-width: 38px;
            min-height: 38px;
            padding: 8px;
            color: @window_fg_color;
            transition: 180ms cubic-bezier(0.22, 1, 0.36, 1);
        }
        .tool-button:hover {
            background-color: alpha(@accent_bg_color, 0.14);
        }
        .tool-button:hover image {
            -gtk-icon-transform: scale(1.08);
        }
        .tool-button:checked {
            color: @accent_fg_color;
            background-color: @accent_bg_color;
            box-shadow: 0 8px 18px alpha(@accent_bg_color, 0.32);
        }
        .tool-button:checked image {
            color: @accent_fg_color;
            -gtk-icon-transform: scale(1.0);
        }
        .tool-separator {
            margin: 4px 10px;
            opacity: 0.45;
        }
        .canvas-surface {
            background-color: @view_bg_color;
        }
        .status-chip, .zoom-chip {
            border-radius: 999px;
            padding: 5px 8px;
        }
        .zoom-value {
            min-width: 3.6em;
            font-weight: 600;
        }
        .color-swatch {
            min-width: 24px;
            min-height: 24px;
            padding: 0;
            border-radius: 999px;
            border: 1px solid alpha(@borders, 0.5);
            transition: 180ms cubic-bezier(0.22, 1, 0.36, 1);
        }
        .color-swatch:hover {
            box-shadow: 0 0 0 3px alpha(@accent_bg_color, 0.28);
        }
        .color-swatch:checked {
            box-shadow: 0 0 0 2px @window_bg_color, 0 0 0 4px @accent_bg_color;
        }
        .swatch-ink { background-color: #1a1f29; }
        .swatch-blue { background-color: #1f61e0; }
        .swatch-red { background-color: #d62933; }
        .swatch-green { background-color: #0d8f52; }
        .swatch-violet { background-color: #7a33c7; }
        .swatch-amber { background-color: #f29e0d; }
        .circular-icon {
            min-width: 32px;
            min-height: 32px;
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
        .min_sidebar_width(248.0)
        .max_sidebar_width(320.0)
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
    };
    canvas.connect_view_changed({
        let feedback = feedback.clone();
        move |percent| feedback.set_zoom(percent)
    });

    let navigator = Navigator::new(&canvas, &feedback);
    let (workspace, chrome) = build_canvas_workspace(&canvas, &feedback);
    split_view.set_sidebar(Some(&navigator.sidebar));
    split_view.set_content(Some(&workspace));
    toast_overlay.set_child(Some(&split_view));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.set_reveal_top_bars(false);
    let header = build_header(&window, &feedback.title);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&toast_overlay));
    window.set_content(Some(&toolbar_view));
    wire_immersive_chrome(&canvas, &toolbar_view, &chrome);

    install_actions(
        application,
        &window,
        &canvas,
        &feedback,
        &navigator,
        &split_view,
    );
    install_shortcuts(application);
    protect_unsaved_close(&window, &canvas);
    play_chrome_entrance(&toolbar_view, &chrome);
    window.present();
    feedback.whisper("Ready — draw, type, or import");
}

impl Navigator {
    fn new(canvas: &Canvas, feedback: &Feedback) -> Self {
        let page_list = boxed_list("page-list");
        let layer_list = boxed_list("layer-list");
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

        let brand = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();
        let brand_icon = gtk::Image::from_icon_name("document-edit-symbolic");
        brand_icon.add_css_class("accent");
        brand_icon.set_pixel_size(22);
        let brand_text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        let title = gtk::Label::builder().label("Notebook").xalign(0.0).build();
        title.add_css_class("heading");
        let subtitle = gtk::Label::builder()
            .label("Local-first · autosaved")
            .xalign(0.0)
            .build();
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");
        brand_text.append(&title);
        brand_text.append(&subtitle);
        brand.append(&brand_icon);
        brand.append(&brand_text);
        content.append(&brand);

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search notes and labels")
            .tooltip_text("Press Enter to jump to the next match")
            .build();
        search.add_css_class("sidebar-search");
        content.append(&search);

        let add_page = circular_icon_button("list-add-symbolic", "Add page");
        content.append(&section_header("Pages", Some(&add_page)));
        content.append(&page_list);
        let previous_page = circular_icon_button("go-previous-symbolic", "Previous page");
        let next_page = circular_icon_button("go-next-symbolic", "Next page");
        let remove_page = circular_icon_button("list-remove-symbolic", "Remove page");
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
        let page_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        page_spacer.set_hexpand(true);
        page_actions.append(&page_spacer);
        page_actions.append(&remove_page);
        content.append(&page_actions);

        let add_layer = circular_icon_button("list-add-symbolic", "Add layer");
        content.append(&section_header("Layers", Some(&add_layer)));
        content.append(&layer_list);
        let remove_layer = circular_icon_button("list-remove-symbolic", "Remove layer");
        remove_layer.set_halign(gtk::Align::End);
        content.append(&remove_layer);

        let grid = adw::SwitchRow::builder()
            .title("Canvas grid")
            .subtitle("Light page lines while you sketch")
            .build();
        let view_list = boxed_list("view-list");
        view_list.set_selection_mode(gtk::SelectionMode::None);
        view_list.append(&grid);
        content.append(&view_list);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        content.append(&spacer);
        let hint = gtk::Label::builder()
            .label("Click a name to rename it. F9 hides this sidebar.")
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
            page_list,
            layer_list,
            grid,
            updating,
        };

        navigator.page_list.connect_row_selected({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            move |_, row| {
                if navigator.updating.get() {
                    return;
                }
                if let Some(index) = row.map(|row| row.index() as usize) {
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
        remove_page.connect_clicked({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |_| {
                if canvas.remove_active_page() {
                    navigator.refresh(&canvas);
                    feedback.show("Page removed");
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
        navigator.grid.connect_active_notify({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |row| {
                if navigator.updating.get() {
                    return;
                }
                if canvas.grid_visible() != row.is_active() {
                    canvas.toggle_grid();
                }
                feedback.whisper(if canvas.grid_visible() {
                    "Grid on"
                } else {
                    "Grid off"
                });
            }
        });
        search.connect_activate({
            let canvas = canvas.clone();
            let navigator = navigator.clone();
            let feedback = feedback.clone();
            move |search| {
                if let Some(hit) = canvas.find_next(search.text().as_str()) {
                    navigator.refresh(&canvas);
                    feedback.show(format!("{} · {}", hit.page_title, hit.snippet));
                } else {
                    feedback.show("No matching note or label");
                }
            }
        });
        navigator.refresh(canvas);
        navigator
    }

    fn refresh(&self, canvas: &Canvas) {
        self.updating.set(true);
        self.refresh_lists(canvas);
        self.grid.set_active(canvas.grid_visible());
        self.updating.set(false);
    }

    fn refresh_after_page_change(&self, canvas: &Canvas) {
        self.updating.set(true);
        self.refresh_layer_list(canvas);
        self.grid.set_active(canvas.grid_visible());
        self.updating.set(false);
    }

    fn refresh_lists(&self, canvas: &Canvas) {
        let updating = self.updating.get();
        self.updating.set(true);
        self.refresh_page_list(canvas);
        self.refresh_layer_list(canvas);
        self.updating.set(updating);
    }

    fn refresh_page_list(&self, canvas: &Canvas) {
        while let Some(child) = self.page_list.first_child() {
            self.page_list.remove(&child);
        }
        let counts = canvas.page_object_counts();
        let titles = canvas.page_titles();
        let active_page = canvas.active_page_index();
        for (index, title) in titles.iter().enumerate() {
            let count = counts.get(index).copied().unwrap_or(0);
            let subtitle = if count == 0 {
                "Empty page".to_owned()
            } else {
                format!("{count} objects")
            };
            let (row, name) = editable_nav_row(title, Some(subtitle.as_str()));
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

    fn refresh_layer_list(&self, canvas: &Canvas) {
        while let Some(child) = self.layer_list.first_child() {
            self.layer_list.remove(&child);
        }
        let active_layer = canvas.active_layer_index();
        for (index, (title, visible, locked)) in canvas.layer_summaries().into_iter().enumerate() {
            let (row, name) = editable_nav_row(&title, None);
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

fn section_header(text: &str, action: Option<&gtk::Button>) -> gtk::Box {
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
    label.add_css_class("heading");
    label.add_css_class("dim-label");
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

fn build_header(window: &adw::ApplicationWindow, title: &adw::WindowTitle) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(title));

    let sidebar_content = adw::ButtonContent::builder()
        .icon_name("sidebar-show-symbolic")
        .label("Notebook")
        .build();
    let sidebar = gtk::ToggleButton::builder()
        .child(&sidebar_content)
        .tooltip_text("Show or hide the notebook sidebar (F9)")
        .action_name("win.toggle-sidebar")
        .active(true)
        .build();
    sidebar.add_css_class("flat");
    sidebar.add_css_class("pill");
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
    save.add_css_class("suggested-action");
    header.pack_start(&sidebar);
    header.pack_start(&open);
    header.pack_start(&save);

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
    window.set_title(Some("Inkstone — Untitled note"));
    header
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("New"), Some("win.new"));
    file.append(Some("Open…"), Some("win.open"));
    file.append(Some("Save"), Some("win.save"));
    file.append(Some("Save As…"), Some("win.save-as"));
    file.append(Some("Import Image or PDF…"), Some("win.import-media"));
    file.append(Some("Export SVG…"), Some("win.export-svg"));
    file.append(Some("Export PDF…"), Some("win.export-pdf"));
    menu.append_section(None, &file);

    let edit = gio::Menu::new();
    edit.append(Some("Undo"), Some("win.undo"));
    edit.append(Some("Redo"), Some("win.redo"));
    edit.append(Some("Duplicate Selection"), Some("win.duplicate"));
    edit.append(Some("Delete Selection"), Some("win.delete"));
    menu.append_section(None, &edit);

    let view = gio::Menu::new();
    view.append(Some("Find in Notebook"), Some("win.focus-search"));
    view.append(Some("Notebook Sidebar"), Some("win.toggle-sidebar"));
    view.append(Some("Previous Page"), Some("win.previous-page"));
    view.append(Some("Next Page"), Some("win.next-page"));
    view.append(Some("Zoom In"), Some("win.zoom-in"));
    view.append(Some("Zoom Out"), Some("win.zoom-out"));
    view.append(Some("Reset View"), Some("win.reset-view"));
    menu.append_section(None, &view);

    let about = gio::Menu::new();
    about.append(Some("About Inkstone"), Some("win.about"));
    menu.append_section(None, &about);
    menu
}

fn chrome_revealer(
    child: &impl IsA<gtk::Widget>,
    transition: gtk::RevealerTransitionType,
    halign: gtk::Align,
    valign: gtk::Align,
) -> gtk::Revealer {
    let duration =
        if gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()) {
            240
        } else {
            0
        };
    let revealer = gtk::Revealer::builder()
        .transition_type(transition)
        .transition_duration(duration)
        .reveal_child(false)
        .halign(halign)
        .valign(valign)
        .child(child)
        .build();
    revealer.connect_reveal_child_notify(|revealer| {
        revealer.set_can_target(revealer.is_child_revealed());
    });
    revealer
}

fn play_chrome_entrance(toolbar: &adw::ToolbarView, chrome: &WorkspaceChrome) {
    let animated =
        gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations());
    toolbar.set_reveal_top_bars(false);
    chrome.tools.set_reveal_child(false);
    chrome.options.set_reveal_child(false);
    chrome.status.set_reveal_child(false);
    chrome.zoom.set_reveal_child(false);
    let delay = if animated { 40 } else { 0 };
    let toolbar = toolbar.clone();
    let tools = chrome.tools.clone();
    let options = chrome.options.clone();
    let status = chrome.status.clone();
    let zoom = chrome.zoom.clone();
    glib::timeout_add_local(Duration::from_millis(delay), move || {
        toolbar.set_reveal_top_bars(true);
        tools.set_reveal_child(true);
        options.set_reveal_child(true);
        status.set_reveal_child(true);
        zoom.set_reveal_child(true);
        glib::ControlFlow::Break
    });
}

fn wire_immersive_chrome(canvas: &Canvas, toolbar: &adw::ToolbarView, chrome: &WorkspaceChrome) {
    let generation = Rc::new(Cell::new(0_u64));
    canvas.connect_busy({
        let toolbar = toolbar.clone();
        let tools = chrome.tools.clone();
        let options = chrome.options.clone();
        let status = chrome.status.clone();
        let zoom = chrome.zoom.clone();
        let generation = generation.clone();
        move |busy| {
            if busy {
                generation.set(generation.get().wrapping_add(1));
                toolbar.set_reveal_top_bars(false);
                tools.set_reveal_child(false);
                options.set_reveal_child(false);
                status.set_reveal_child(false);
                zoom.set_reveal_child(false);
                return;
            }
            let token = generation.get().wrapping_add(1);
            generation.set(token);
            let toolbar = toolbar.clone();
            let tools = tools.clone();
            let options = options.clone();
            let status = status.clone();
            let zoom = zoom.clone();
            let generation = generation.clone();
            glib::timeout_add_local(Duration::from_millis(200), move || {
                if generation.get() == token {
                    toolbar.set_reveal_top_bars(true);
                    tools.set_reveal_child(true);
                    options.set_reveal_child(true);
                    status.set_reveal_child(true);
                    zoom.set_reveal_child(true);
                }
                glib::ControlFlow::Break
            });
        }
    });
}

#[derive(Clone)]
struct WorkspaceChrome {
    tools: gtk::Revealer,
    options: gtk::Revealer,
    status: gtk::Revealer,
    zoom: gtk::Revealer,
}

fn build_canvas_workspace(canvas: &Canvas, feedback: &Feedback) -> (gtk::Overlay, WorkspaceChrome) {
    canvas.widget().add_css_class("canvas-surface");
    let workspace = gtk::Overlay::new();
    workspace.set_child(Some(canvas.widget()));

    let tools = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    tools.add_css_class("floating-panel");
    tools.add_css_class("tool-palette");

    let options = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(200)
        .halign(gtk::Align::Center)
        .build();
    options.add_named(&select_options(), Some("select"));
    options.add_named(&ink_options(canvas), Some("ink"));
    options.add_named(&text_options(canvas), Some("text"));
    options.add_named(&diagram_options(canvas), Some("diagram"));
    options.add_named(
        &hint_options("Drag over an object to erase it"),
        Some("eraser"),
    );
    options.add_named(
        &hint_options("Drag the canvas · middle mouse always pans"),
        Some("pan"),
    );
    options.set_visible_child_name("ink");

    let options_frame = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::Center)
        .margin_top(14)
        .build();
    options_frame.add_css_class("floating-panel");
    options_frame.add_css_class("tool-options");
    options_frame.append(&options);

    let select = tool_button(
        "Select (click or lasso)",
        "select",
        Tool::Select,
        canvas,
        &options,
        None,
    );
    let pen = tool_button("Pen", "ink", Tool::Pen, canvas, &options, Some(&select));
    pen.set_active(true);
    let highlighter = tool_button(
        "Highlighter",
        "ink",
        Tool::Highlighter,
        canvas,
        &options,
        Some(&select),
    );
    let eraser = tool_button(
        "Eraser",
        "eraser",
        Tool::Eraser,
        canvas,
        &options,
        Some(&select),
    );
    let pan = tool_button("Pan", "pan", Tool::Pan, canvas, &options, Some(&select));
    let text = tool_button("Text", "text", Tool::Text, canvas, &options, Some(&select));
    let shape = tool_button(
        "Shape",
        "diagram",
        Tool::Shape,
        canvas,
        &options,
        Some(&select),
    );
    let connector = tool_button(
        "Connector",
        "diagram",
        Tool::Connector,
        canvas,
        &options,
        Some(&select),
    );
    fn tool_separator() -> gtk::Separator {
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.add_css_class("tool-separator");
        separator
    }
    tools.append(&select);
    tools.append(&tool_separator());
    tools.append(&pen);
    tools.append(&highlighter);
    tools.append(&eraser);
    tools.append(&tool_separator());
    tools.append(&pan);
    tools.append(&tool_separator());
    tools.append(&text);
    tools.append(&shape);
    tools.append(&connector);

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let top_space = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_space.set_vexpand(true);
    let bottom_space = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_space.set_vexpand(true);
    column.append(&top_space);
    column.append(&tools);
    column.append(&bottom_space);

    let tools_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_width(true)
        .overlay_scrolling(true)
        .vexpand(true)
        .hexpand(false)
        .child(&column)
        .build();
    tools_scroll.set_width_request(54);

    let tools_revealer = chrome_revealer(
        &tools_scroll,
        gtk::RevealerTransitionType::SlideRight,
        gtk::Align::Start,
        gtk::Align::Fill,
    );
    tools_revealer.set_margin_start(14);
    tools_revealer.set_margin_top(72);
    tools_revealer.set_margin_bottom(64);
    let options_revealer = chrome_revealer(
        &options_frame,
        gtk::RevealerTransitionType::SlideDown,
        gtk::Align::Center,
        gtk::Align::Start,
    );
    workspace.add_overlay(&tools_revealer);
    workspace.add_overlay(&options_revealer);

    let status_chip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(16)
        .margin_bottom(16)
        .build();
    status_chip.add_css_class("floating-panel");
    status_chip.add_css_class("status-chip");
    let status_icon = gtk::Image::from_icon_name("document-edit-symbolic");
    status_icon.add_css_class("dim-label");
    status_chip.append(&status_icon);
    status_chip.append(&feedback.status);
    let status_revealer = chrome_revealer(
        &status_chip,
        gtk::RevealerTransitionType::SlideUp,
        gtk::Align::Start,
        gtk::Align::End,
    );
    workspace.add_overlay(&status_revealer);

    let zoom = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .margin_end(16)
        .margin_bottom(16)
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
        gtk::RevealerTransitionType::SlideUp,
        gtk::Align::End,
        gtk::Align::End,
    );
    workspace.add_overlay(&zoom_revealer);
    (
        workspace,
        WorkspaceChrome {
            tools: tools_revealer,
            options: options_revealer,
            status: status_revealer,
            zoom: zoom_revealer,
        },
    )
}

fn tool_button(
    tooltip: &str,
    options_name: &'static str,
    tool: Tool,
    canvas: &Canvas,
    options: &gtk::Stack,
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
    button.connect_toggled({
        let canvas = canvas.clone();
        let options = options.clone();
        let glyph = glyph.clone();
        move |button| {
            glyph.queue_draw();
            if button.is_active() {
                canvas.set_tool(tool);
                options.set_visible_child_name(options_name);
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
    row.append(&option_hint("Selection"));
    for (label, icon, action) in [
        ("Duplicate", "edit-copy-symbolic", "win.duplicate"),
        ("Delete", "user-trash-symbolic", "win.delete"),
    ] {
        let content = adw::ButtonContent::builder()
            .icon_name(icon)
            .label(label)
            .build();
        let button = gtk::Button::builder()
            .child(&content)
            .action_name(action)
            .build();
        button.add_css_class("flat");
        button.add_css_class("pill");
        row.append(&button);
    }
    row
}

fn ink_options(canvas: &Canvas) -> gtk::Box {
    let row = option_row();
    row.append(&option_hint("Ink"));
    row.append(&color_picker(canvas));
    row.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    row.append(&option_hint("Width"));
    row.append(&width_control(canvas));
    row
}

fn text_options(canvas: &Canvas) -> gtk::Box {
    let row = option_row();
    let note = gtk::Entry::builder()
        .placeholder_text("Type text, then click the canvas")
        .width_chars(28)
        .build();
    note.connect_changed({
        let canvas = canvas.clone();
        move |entry| canvas.set_text(entry.text().to_string())
    });
    row.append(&note);
    row.append(&color_picker(canvas));
    row
}

fn diagram_options(canvas: &Canvas) -> gtk::Box {
    let row = option_row();
    let shape_picker = gtk::DropDown::from_strings(&ShapeKind::NAMES);
    shape_picker.set_tooltip_text(Some("Schematic primitive"));
    shape_picker.connect_selected_notify({
        let canvas = canvas.clone();
        move |picker| {
            if let Some(kind) = ShapeKind::ALL.get(picker.selected() as usize) {
                canvas.set_shape_kind(*kind);
            }
        }
    });
    let label = gtk::Entry::builder()
        .placeholder_text("Optional label")
        .width_chars(14)
        .build();
    label.connect_changed({
        let canvas = canvas.clone();
        move |entry| canvas.set_label(entry.text().to_string())
    });
    row.append(&shape_picker);
    row.append(&label);
    row.append(&color_picker(canvas));
    row.append(&width_control(canvas));
    row
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
        .halign(gtk::Align::Center)
        .margin_start(10)
        .margin_end(10)
        .margin_top(6)
        .margin_bottom(6)
        .build()
}

fn option_hint(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label
}

fn color_picker(canvas: &Canvas) -> gtk::Box {
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
    let mut group: Option<gtk::ToggleButton> = None;
    for (index, (class, name, color)) in COLORS.iter().enumerate() {
        let swatch = gtk::ToggleButton::builder().tooltip_text(*name).build();
        swatch.add_css_class("color-swatch");
        swatch.add_css_class(class);
        if let Some(group) = &group {
            swatch.set_group(Some(group));
        } else {
            group = Some(swatch.clone());
            swatch.set_active(true);
        }
        swatch.connect_toggled({
            let canvas = canvas.clone();
            let color = *color;
            move |button| {
                if button.is_active() {
                    canvas.set_color(color);
                }
            }
        });
        if index == 0 {
            swatch.set_active(true);
        }
        row.append(&swatch);
    }
    row
}

fn width_control(canvas: &Canvas) -> gtk::Scale {
    let width = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.5, 32.0, 0.5);
    width.set_value(2.5);
    width.set_draw_value(false);
    width.set_width_request(108);
    width.set_tooltip_text(Some("Stroke width 2.5"));
    width.connect_value_changed({
        let canvas = canvas.clone();
        move |control| {
            let value = control.value();
            canvas.set_width(value as f32);
            control.set_tooltip_text(Some(&format!("Stroke width {value:.1}")));
        }
    });
    width
}

fn install_actions(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    canvas: &Canvas,
    status: &Feedback,
    navigator: &Navigator,
    split_view: &adw::OverlaySplitView,
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
            if canvas.is_dirty() {
                confirm_discard(&window, {
                    let canvas = canvas.clone();
                    let window = window.clone();
                    let status = status.clone();
                    let navigator = navigator.clone();
                    move || {
                        canvas.new_document();
                        navigator.refresh(&canvas);
                        refresh_status(&window, &canvas, &status, "New document");
                    }
                });
            } else {
                canvas.new_document();
                navigator.refresh(&canvas);
                refresh_status(&window, &canvas, &status, "New document");
            }
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
    add_action(window, "save", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || match canvas.save_current() {
            Ok(true) => refresh_status(&window, &canvas, &status, "Saved"),
            Ok(false) => choose_save(&window, &canvas, &status),
            Err(error) => show_error(&window, &error.to_string()),
        }
    });
    add_action(window, "save-as", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_save(&window, &canvas, &status)
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
    add_action(window, "import-media", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || choose_import(&window, &canvas, &status)
    });
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
            let target = canvas.animate_zoom_by(1.2);
            status.whisper(format!("Zooming to {target}%"));
        }
    });
    add_action(window, "zoom-out", {
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let target = canvas.animate_zoom_by(1.0 / 1.2);
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
        ("win.open", &["<primary>o"][..]),
        ("win.save", &["<primary>s"][..]),
        ("win.save-as", &["<primary><shift>s"][..]),
        ("win.import-media", &["<primary>i"][..]),
        ("win.export-svg", &["<primary>e"][..]),
        ("win.export-pdf", &["<primary><shift>e"][..]),
        ("win.undo", &["<primary>z"][..]),
        ("win.redo", &["<primary><shift>z", "<primary>y"][..]),
        ("win.duplicate", &["<primary>d"][..]),
        ("win.delete", &["Delete", "BackSpace"][..]),
        ("win.previous-page", &["<alt>Left"][..]),
        ("win.next-page", &["<alt>Right"][..]),
        ("win.zoom-in", &["<primary>plus", "<primary>equal"][..]),
        ("win.zoom-out", &["<primary>minus"][..]),
        ("win.reset-view", &["<primary>0"][..]),
    ] {
        application.set_accels_for_action(action, accelerators);
    }
}

fn add_action(window: &adw::ApplicationWindow, name: &str, callback: impl Fn() + 'static) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| callback());
    window.add_action(&action);
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
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        let navigator = navigator.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                match canvas.load(&path) {
                    Ok(()) => {
                        navigator.refresh(&canvas);
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

fn choose_save(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
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
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                let path = with_extension(path, "inkstone");
                match canvas.save(&path) {
                    Ok(()) => refresh_status(&window, &canvas, &status, "Saved"),
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

fn choose_import(window: &adw::ApplicationWindow, canvas: &Canvas, status: &Feedback) {
    let chooser = gtk::FileChooserNative::builder()
        .title("Import image or PDF")
        .transient_for(window)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .accept_label("Import")
        .cancel_label("Cancel")
        .build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Images and PDF documents"));
    for pattern in ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.pdf"] {
        filter.add_pattern(pattern);
    }
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(path) = chooser.file().and_then(|file| file.path())
            {
                match canvas.import_media(&path) {
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

fn protect_unsaved_close(window: &adw::ApplicationWindow, canvas: &Canvas) {
    window.connect_close_request({
        let window = window.clone();
        let canvas = canvas.clone();
        move |_| {
            if !canvas.is_dirty() {
                return glib::Propagation::Proceed;
            }
            confirm_discard(&window, {
                let window = window.clone();
                move || window.destroy()
            });
            glib::Propagation::Stop
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
