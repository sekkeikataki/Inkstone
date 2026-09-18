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
