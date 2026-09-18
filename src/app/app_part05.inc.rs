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
