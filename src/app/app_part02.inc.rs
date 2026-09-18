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
