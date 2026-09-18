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
    options_name: &'static str,
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
    button.connect_toggled({
        let canvas = canvas.clone();
        let options = options.clone();
        let options_host = options_host.clone();
        let options_page = options_page.clone();
        let glyph = glyph.clone();
        move |button| {
            glyph.queue_draw();
            if button.is_active() {
                canvas.set_tool(tool);
                options.set_visible_child_name(options_name);
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
