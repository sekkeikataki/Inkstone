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
            Some(result) => status.show(format!("Calculated {result}")),
            None => status.whisper("Type an equation that ends with ="),
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
    add_action(window, "cut", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let (json, count) = canvas.cut_selection();
            if let Some(json) = json {
                window.clipboard().set_text(&json);
            }
            refresh_status(
                &window,
                &canvas,
                &status,
                &format!("Cut {count} selected object(s)"),
            );
        }
    });
    add_action(window, "copy", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            if let Some(json) = canvas.copy_selection() {
                window.clipboard().set_text(&json);
                refresh_status(&window, &canvas, &status, "Copied selection");
            } else {
                status.whisper("Nothing selected");
            }
        }
    });
    add_action(window, "paste", {
        let window = window.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        move || {
            let window = window.clone();
            let canvas = canvas.clone();
            let status = status.clone();
            window.clipboard().read_text_async(
                gio::Cancellable::NONE,
                move |result| match result {
                    Ok(Some(text)) => match canvas.paste_json(text.as_str()) {
                        Ok(count) => refresh_status(
                            &window,
                            &canvas,
                            &status,
                            &format!("Pasted {count} object(s)"),
                        ),
                        Err(error) => show_error(&window, &error.to_string()),
                    },
                    Ok(None) => status.whisper("Clipboard is empty"),
                    Err(error) => show_error(&window, &error.to_string()),
                },
            );
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
