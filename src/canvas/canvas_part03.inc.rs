        self.dirty = true;
        count
    }

    fn render_page(
        &self,
        context: &Context,
        page_index: usize,
        width: f64,
        height: f64,
    ) -> Result<(), DocumentError> {
        let page = self.notebook.pages.get(page_index).ok_or_else(|| {
            DocumentError::Invalid(format!("page index {page_index} is out of range"))
        })?;
        context.set_source_rgb(1.0, 1.0, 1.0);
        let _ = context.paint();
        if let Some(bounds) = page.content_bounds() {
            let bounds = bounds.expand(24.0);
            let scale = ((width - 36.0) / bounds.width as f64)
                .min((height - 36.0) / bounds.height as f64)
                .min(1.0);
            context.save().ok();
            context.translate(
                18.0 - bounds.x as f64 * scale,
                18.0 - bounds.y as f64 * scale,
            );
            context.scale(scale, scale);
            for element in page.visible_elements() {
                draw_element(context, element, &self.image_cache);
            }
            context.restore().ok();
        }
        Ok(())
    }

    fn render_page_png(&self, page_index: usize) -> Result<Vec<u8>, DocumentError> {
        let page = self.notebook.pages.get(page_index).ok_or_else(|| {
            DocumentError::Invalid(format!("page index {page_index} is out of range"))
        })?;
        let bounds = page
            .content_bounds()
            .unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                width: 640.0,
                height: 360.0,
            })
            .expand(24.0);
        let width = bounds.width.ceil().max(1.0) as i32;
        let height = bounds.height.ceil().max(1.0) as i32;
        let surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let context =
            Context::new(&surface).map_err(|error| DocumentError::Export(error.to_string()))?;
        set_source(&context, page.canvas.background);
        let _ = context.paint();
        context.translate(-bounds.x as f64, -bounds.y as f64);
        for element in page.visible_elements() {
            draw_element(&context, element, &self.image_cache);
        }
        surface.flush();
        let mut png = Vec::new();
        surface
            .write_to_png(&mut Cursor::new(&mut png))
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        Ok(png)
    }

    fn screen_to_world(&self, screen: Point) -> Point {
        Point::new(
            (screen.x - self.pan.x) / self.zoom,
            (screen.y - self.pan.y) / self.zoom,
        )
    }

    fn set_zoom_around(&mut self, requested_zoom: f32, screen: Point) {
        let world = self.screen_to_world(screen);
        self.zoom = requested_zoom.clamp(self.runtime.min_zoom, self.runtime.max_zoom);
        self.pan = Point::new(
            screen.x - world.x * self.zoom,
            screen.y - world.y * self.zoom,
        );
    }

    fn begin_input(
        &mut self,
        screen: Point,
        pressure: f32,
        eraser_tip: bool,
        button: u32,
        shift: bool,
    ) {
        if shift {
            self.constrain = true;
        }
        if button == 2 && !eraser_tip || self.tool == Tool::Pan {
            self.interaction = Some(Interaction::Pan {
                last_screen: screen,
            });
            return;
        }
        self.replay = None;

        let world = self.screen_to_world(screen);
        let effective_tool = if eraser_tip { Tool::Eraser } else { self.tool };
        match effective_tool {
            Tool::Select => {
                if let Some(handle) = self.hit_transform_handle(world) {
                    let before = self.capture_elements(&self.selection);
                    self.interaction = match handle {
                        TransformHandle::Resize { origin } => Some(Interaction::Resize {
                            origin,
                            start: world,
                            before,
                        }),
                        TransformHandle::Rotate { center } => Some(Interaction::Rotate {
                            center,
                            start_angle: (world.y - center.y).atan2(world.x - center.x),
                            before,
                        }),
                    };
                } else if let Some(id) = self.hit_test(world) {
                    if self.toggle_checkable(id, world) {
                        return;
                    }
                    if self.follow_text_link(id) {
                        return;
                    }
                    if !self.selection.contains(&id) {
                        self.selection.clear();
                        self.selection.insert(id);
                        self.flash_selection();
                    }
                    let before = self.capture_elements(&self.selection);
                    self.interaction = Some(Interaction::MoveSelection {
                        last_world: world,
                        before,
                    });
                } else {
                    self.selection.clear();
                    self.interaction = Some(Interaction::Lasso {
                        start: world,
                        current: world,
                    });
                }
            }
            Tool::Pen | Tool::Brush | Tool::Highlighter => {
                if self.active_layer().locked {
                    return;
                }
                let mut style = self.style.clone();
                let kind = match effective_tool {
                    Tool::Highlighter => {
                        style.color.alpha = self.runtime.highlighter_alpha;
                        style.width =
                            (style.width * self.runtime.highlighter_width_scale).max(mm_to_pt(2.0));
                        StrokeKind::Highlighter
                    }
                    Tool::Brush => {
                        style.width =
                            (style.width * self.runtime.brush_width_scale).max(style.width);
                        StrokeKind::Brush
                    }
                    _ => StrokeKind::Pen,
                };
                self.interaction = Some(Interaction::Stroke(Stroke {
                    id: Uuid::new_v4(),
                    kind,
                    style,
                    points: vec![StrokePoint::new(world, pressure.max(0.05))],
                }));
            }
            Tool::Eraser => {
                let layer = self.active_layer();
                let before = layer.elements.clone();
                let page_id = self.page().id;
                let layer_id = layer.id;
                self.interaction = Some(Interaction::Erase {
                    page_id,
                    layer_id,
                    before,
                    changed: false,
                });
                self.erase_at(world, true);
            }
            Tool::Text => {
                if self.active_layer().locked {
                    return;
                }
                if let Some(id) = self.hit_test(world) {
                    if self.load_table_cell(id, world) {
                        self.interaction = None;
                        return;
                    }
                    if self.load_text_element(id) {
                        self.interaction = None;
                        return;
                    }
                }
                let mut text = self.pending_text.trim().to_owned();
                if let Some(calculated) = local::evaluate_equation(&text) {
                    text = calculated;
                }
                if !text.is_empty() {
                    let mut note = TextNote::plain(
                        world,
                        text.clone(),
                        self.pending_font_size,
                        self.style.color,
                    );
                    note.max_width = Some(420.0);
                    note.bold = self.pending_bold;
                    note.italic = self.pending_italic;
                    note.underline = self.pending_underline;
                    note.list = self.pending_list;
                    note.href = self.pending_href.clone().or_else(|| {
                        local::parse_page_links(&text).into_iter().find_map(|name| {
                            local::resolve_page_name(&self.notebook, &name)
                                .map(|index| local::page_link_href(self.notebook.pages[index].id))
                        })
                    });
                    self.add_element(Element::Text(note));
                }
                self.editing_table = None;
                self.interaction = None;
            }
            Tool::Shape | Tool::Measure => {
                if self.active_layer().locked {
                    return;
                }
                self.interaction = Some(Interaction::Shape {
                    start: world,
                    current: world,
                });
            }
            Tool::Connector => {
                if self.active_layer().locked {
                    return;
                }
                let start = self.page().snap_endpoint(world, 18.0 / self.zoom);
                self.interaction = Some(Interaction::Connector {
                    start,
                    current: world,
                });
            }
            Tool::Pan => unreachable!(),
            Tool::Space => {
                if self.active_layer().locked {
                    return;
                }
                let ids: HashSet<Uuid> = self.page().elements().map(Element::id).collect();
                let before = self.capture_elements(&ids);
                self.interaction = Some(Interaction::Space {
                    start_y: world.y,
                    last_y: world.y,
                    before,
                });
            }
        }
    }

    fn update_input(&mut self, screen: Point, pressure: f32, shift: bool) {
        if shift {
            self.constrain = true;
        }
        let mut world = self.screen_to_world(screen);
        if self.ruler
            && let Some(Interaction::Stroke(stroke)) = &self.interaction
            && let Some(first) = stroke.points.first()
        {
            world = local::snap_to_ruler(first.point(), world);
        } else if self.constrain
            && self.runtime.iso_angle_snap
            && let Some(Interaction::Stroke(stroke)) = &self.interaction
            && let Some(first) = stroke.points.first()
        {
            world = local::snap_to_iso_angle(first.point(), world);
        }
        let zoom = self.zoom;
        if let Some((origin, start, before)) = match &self.interaction {
            Some(Interaction::Resize {
                origin,
                start,
                before,
            }) => Some((*origin, *start, before.clone())),
            _ => None,
        } {
            let mut sx = if (start.x - origin.x).abs() < 0.5 {
                1.0
            } else {
                (world.x - origin.x) / (start.x - origin.x)
            };
            let mut sy = if (start.y - origin.y).abs() < 0.5 {
                1.0
            } else {
                (world.y - origin.y) / (start.y - origin.y)
            };
            if sx.abs() < 0.05 {
                sx = 0.05_f32.copysign(sx);
            }
            if sy.abs() < 0.05 {
                sy = 0.05_f32.copysign(sy);
            }
            self.restore_slots(&before);
            self.scale_selection(origin, sx, sy);
            return;
        }
        if let Some((center, start_angle, before)) = match &self.interaction {
            Some(Interaction::Rotate {
                center,
                start_angle,
                before,
            }) => Some((*center, *start_angle, before.clone())),
            _ => None,
        } {
            let angle = (world.y - center.y).atan2(world.x - center.x);
            let degrees = (angle - start_angle).to_degrees();
            self.restore_slots(&before);
            self.rotate_selection(center, degrees);
            return;
        }
        if let Some((start_y, last_y)) = match &self.interaction {
            Some(Interaction::Space {
                start_y, last_y, ..
            }) => Some((*start_y, *last_y)),
            _ => None,
        } {
            let dy = world.y - last_y;
            if let Some(Interaction::Space { last_y, .. }) = self.interaction.as_mut() {
                *last_y = world.y;
            }
            if dy.abs() > f32::EPSILON {
                self.shift_below(start_y, dy);
            }
            return;
        }
        let mut selection_delta = None;
        match self.interaction.as_mut() {
            Some(Interaction::Stroke(stroke)) => {
                let mut target = world;
                if self.stabilizer
                    && let Some(last) = stroke.points.last()
                {
                    target = local::stabilize_point(
                        last.point(),
                        world,
                        self.runtime.stabilizer_strength,
                    );
                }
                let should_add = stroke
                    .points
                    .last()
                    .is_none_or(|last| last.point().distance_to(target) >= 0.7 / zoom);
                if should_add {
                    stroke
                        .points
                        .push(StrokePoint::new(target, pressure.max(0.05)));
                }
            }
            Some(Interaction::Shape { start, current }) => {
                let start = *start;
                let constrain = self.constrain;
                let tool = self.tool;
                let kind = self.shape_kind;
                *current = constrain_shape_point(
                    constrain,
                    self.runtime.iso_angle_snap,
                    tool,
                    kind,
                    start,
                    world,
                );
            }
            Some(Interaction::Connector { current, .. })
            | Some(Interaction::Lasso { current, .. }) => {
                *current = world;
            }
            Some(Interaction::MoveSelection { last_world, .. }) => {
                let mut delta = Point::new(world.x - last_world.x, world.y - last_world.y);
                if self.constrain {
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                selection_delta = Some(delta);
                *last_world = Point::new(last_world.x + delta.x, last_world.y + delta.y);
            }
            Some(Interaction::Pan { last_screen }) => {
                self.pan.x += screen.x - last_screen.x;
                self.pan.y += screen.y - last_screen.y;
                *last_screen = screen;
            }
            Some(Interaction::Erase { .. }) => self.erase_at(world, false),
            Some(Interaction::Resize { .. })
            | Some(Interaction::Rotate { .. })
            | Some(Interaction::Space { .. })
            | None => {}
        }
        if let Some(delta) = selection_delta {
            self.translate_selection(delta);
        }
    }

    fn constrained_shape_point(&self, start: Point, current: Point) -> Point {
        constrain_shape_point(
            self.constrain,
            self.runtime.iso_angle_snap,
            self.tool,
            self.shape_kind,
            start,
            current,
        )
    }

    fn load_text_element(&mut self, id: Uuid) -> bool {
        let Some(Element::Text(note)) = self
            .page()
            .elements()
            .find(|element| element.id() == id)
            .cloned()
        else {
            return false;
        };
        self.selection.clear();
        self.selection.insert(id);
        self.pending_text = note.text.clone();
        self.pending_font_size = note.font_size;
        self.pending_bold = note.bold;
        self.pending_italic = note.italic;
        self.pending_underline = note.underline;
        self.pending_list = note.list;
        self.pending_href = note.href.clone();
        self.flash_selection();
        self.text_loaded = true;
        true
    }

    fn load_table_cell(&mut self, id: Uuid, world: Point) -> bool {
        let Some(Element::Table(table)) = self
            .page()
            .elements()
            .find(|element| element.id() == id)
            .cloned()
        else {
            return false;
        };
        let Some(index) = table.cell_index(world) else {
            return false;
        };
        self.selection.clear();
        self.selection.insert(id);
        self.editing_table = Some((id, index));
        self.pending_text = table.cells.get(index).cloned().unwrap_or_default();
        self.flash_selection();
        self.text_loaded = true;
        true
    }

    fn follow_text_link(&mut self, id: Uuid) -> bool {
        let Some(Element::Text(note)) = self.page().elements().find(|element| element.id() == id)
        else {
            return false;
        };
        let href = note.href.clone().or_else(|| {
            local::parse_page_links(&note.text)
                .into_iter()
                .next()
                .map(|name| format!("[[{name}]]"))
        });
        let Some(href) = href else {
            return false;
        };
        let index = if let Some(id) = local::parse_page_link_href(&href) {
            self.notebook.page_by_id(id)
        } else {
            local::parse_page_links(&href)
                .into_iter()
                .find_map(|name| local::resolve_page_name(&self.notebook, &name))
        };
        let Some(index) = index else {
            return false;
        };
        self.active_page = index;
        self.active_layer = 0;
        self.selection.clear();
        self.begin_page_fade();
        true
    }

    fn align_selection(&mut self, mode: AlignMode) -> usize {
        let ids: Vec<Uuid> = self.selection.iter().copied().collect();
        if ids.len() < 2 && !matches!(mode, AlignMode::SameWidth | AlignMode::SameHeight) {
            return 0;
        }
        if ids.is_empty() {
            return 0;
        }
        let bounds: Vec<Rect> = ids
            .iter()
            .filter_map(|id| {
                self.page()
                    .elements()
                    .find(|element| element.id() == *id)
                    .map(Element::bounds)
            })
            .collect();
        if bounds.len() != ids.len() {
            return 0;
        }
        let deltas = local::align_bounds(&bounds, mode);
        let before = self.capture_elements(&self.selection);
        match mode {
            AlignMode::SameWidth | AlignMode::SameHeight => {
                for (id, scale) in ids.iter().zip(deltas.iter()) {
                    if let Some(element) = self.page_mut().element_mut(*id) {
                        let origin = element.bounds().normalized();
                        let origin_pt = Point::new(origin.x, origin.y);
                        element.scale_from(origin_pt, scale.x, scale.y);
                    }
                }
            }
            _ => {
                for (id, delta) in ids.iter().zip(deltas.iter()) {
                    if let Some(element) = self.page_mut().element_mut(*id) {
                        element.translate(*delta);
                    }
                }
            }
        }
        let page_id = self.page().id;
        self.push_history(HistoryEntry::ElementsChanged {
            page_id,
            slots: before,
        });
        self.dirty = true;
        ids.len()
    }

    fn selection_svg(&self) -> Option<String> {
        let selected: Vec<Element> = self
            .page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if selected.is_empty() {
            return None;
        }
        let bounds = selected
            .iter()
            .map(Element::bounds)
            .reduce(Rect::union)?
            .expand(16.0);
        let mut svg = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">\n",
            bounds.x, bounds.y, bounds.width, bounds.height
        );
        for element in selected {
            svg.push_str(&crate::document::element_svg(&element));
        }
        svg.push_str("</svg>\n");
        Some(svg)
    }

    fn selection_png(&self) -> Result<Option<Vec<u8>>, DocumentError> {
        let selected: Vec<Element> = self
            .page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if selected.is_empty() {
            return Ok(None);
        }
        let bounds = selected
            .iter()
            .map(Element::bounds)
            .reduce(Rect::union)
            .unwrap()
            .expand(16.0);
        let width = bounds.width.ceil().max(8.0) as i32;
        let height = bounds.height.ceil().max(8.0) as i32;
        let surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let context =
            Context::new(&surface).map_err(|error| DocumentError::Export(error.to_string()))?;
        set_source(&context, self.page().canvas.background);
        let _ = context.paint();
        context.translate(-bounds.x as f64, -bounds.y as f64);
        for element in &selected {
            draw_element(&context, element, &self.image_cache);
        }
        surface.flush();
        let mut png = Vec::new();
        surface
            .write_to_png(&mut Cursor::new(&mut png))
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        Ok(Some(png))
    }

    fn cancel_input(&mut self) {
        self.interaction = None;
        self.replay = None;
    }

    fn end_input(&mut self, screen: Point, pressure: f32, shift: bool) {
        self.update_input(screen, pressure, shift);
        let Some(interaction) = self.interaction.take() else {
            return;
        };
        match interaction {
            Interaction::Stroke(mut stroke) => {
                if stroke.points.len() == 1 {
                    let point = stroke.points[0];
                    stroke.points.push(StrokePoint {
                        x: point.x + 0.01,
                        ..point
                    });
                }
                if self.ink_to_shape
                    && let Some(shape) = local::stroke_to_shape(&stroke)
                {
                    self.add_element(Element::Shape(shape));
                } else {
                    self.add_element(Element::Stroke(stroke));
                }
            }
            Interaction::Shape { start, mut current } => {
                current = self.constrained_shape_point(start, current);
                if start.distance_to(current) < 4.0 / self.zoom {
                    current = Point::new(start.x + mm_to_pt(40.0), start.y + mm_to_pt(24.0));
                }
                let kind = if self.tool == Tool::Measure {
                    ShapeKind::Dimension
                } else {
                    self.shape_kind
                };
                let bounds = if kind.uses_drag_bounds() {
                    Rect::from_drag(start, current)
                } else {
                    Rect::from_points(start, current)
                };
                let label = if kind == ShapeKind::Dimension {
                    local::dimension_label(start, current)
                } else {
                    self.pending_label.trim().to_owned()
                };
                self.add_element(Element::Shape(Shape {
                    id: Uuid::new_v4(),
                    kind,
                    bounds,
                    rotation_degrees: 0.0,
                    style: self.style.clone(),
                    fill: self.fill_enabled.then_some(Color {
                        alpha: 0.22,
                        ..self.style.color
                    }),
                    label,
                }));
            }
            Interaction::Connector { start, current } => {
                let end = self.page().snap_endpoint(current, 18.0 / self.zoom);
                let midpoint_x = (start.point.x + end.point.x) / 2.0;
                self.add_element(Element::Connector(Connector {
                    id: Uuid::new_v4(),
                    start: start.clone(),
                    end: end.clone(),
                    route: vec![
                        Point::new(midpoint_x, start.point.y),
                        Point::new(midpoint_x, end.point.y),
                    ],
                    style: self.style.clone(),
                    label: self.pending_label.trim().to_owned(),
                }));
            }
            Interaction::MoveSelection { before, .. }
            | Interaction::Resize { before, .. }
            | Interaction::Rotate { before, .. }
            | Interaction::Space { before, .. } => {
                let changed = before.iter().any(|slot| {
                    slot.stored.as_ref().is_some_and(|stored| {
                        self.page()
                            .elements()
                            .find(|element| element.id() == slot.id)
                            .is_some_and(|current| current != stored)
                    })
                });
                if changed {
                    let page_id = self.page().id;
                    self.push_history(HistoryEntry::ElementsChanged {
                        page_id,
                        slots: before,
                    });
                    self.dirty = true;
                }
            }
            Interaction::Lasso { start, current } => {
                let lasso = Rect::from_points(start, current);
                self.selection = self
                    .page()
                    .layers
                    .iter()
                    .filter(|layer| layer.visible && !layer.locked)
                    .flat_map(|layer| layer.elements.iter())
                    .filter(|element| element.bounds().intersects(lasso))
                    .map(Element::id)
                    .collect();
                if !self.selection.is_empty() {
                    self.flash_selection();
                }
            }
            Interaction::Pan { .. } => {}
            Interaction::Erase {
                page_id,
                layer_id,
                before,
                changed,
            } => {
                if changed {
                    self.push_history(HistoryEntry::LayerReordered {
                        page_id,
                        layer_id,
                        stored: before,
                    });
                    self.dirty = true;
                }
            }
        }
    }

    fn add_element(&mut self, element: Element) {
        let id = element.id();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        self.active_layer_mut().elements.push(element);
        self.push_history(HistoryEntry::Added {
            page_id,
            layer_id,
            id,
            stored: None,
        });
        self.dirty = true;
    }

    fn toggle_checkable(&mut self, id: Uuid, world: Point) -> bool {
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if element.id() != id {
                    continue;
                }
                match element {
                    Element::Tag(tag)
                        if tag.kind == TagKind::ToDo && world.x <= tag.origin.x + 28.0 =>
                    {
                        tag.checked = !tag.checked;
                        self.dirty = true;
                        return true;
                    }
                    Element::Text(text)
                        if text.list == ListStyle::Checklist
                            && world.x <= text.origin.x + text.font_size =>
                    {
                        text.checked = !text.checked;
                        self.dirty = true;
                        return true;
                    }
                    _ => return false,
                }
            }
        }
        false
    }

    fn erase_at(&mut self, point: Point, record: bool) {
        let radius = (self.style.width * self.runtime.eraser_scale).max(10.0 / self.zoom);
        let mut changed = false;
        for layer_index in (0..self.page().layers.len()).rev() {
            if !self.page().layers[layer_index].visible || self.page().layers[layer_index].locked {
                continue;
            }
            let count = self.page().layers[layer_index].elements.len();
            for element_index in (0..count).rev() {
                let Element::Stroke(stroke) =
                    &self.page().layers[layer_index].elements[element_index]
                else {
                    continue;
                };
                let Some(pieces) = local::split_stroke(stroke, point, radius) else {
                    continue;
                };
                self.page_mut().layers[layer_index]
                    .elements
                    .remove(element_index);
                for piece in pieces.into_iter().rev() {
                    self.page_mut().layers[layer_index]
                        .elements
                        .insert(element_index, Element::Stroke(piece));
                }
                changed = true;
                break;
            }
            if changed {
                break;
            }
        }
        if !changed {
            if let Some(id) = self.hit_test(point)
                && !self
                    .page()
                    .elements()
                    .any(|element| element.id() == id && matches!(element, Element::Stroke(_)))
            {
                self.selection.clear();
                self.selection.insert(id);
                self.delete_selection();
                self.selection.clear();
            }
            return;
        }
        if let Some(Interaction::Erase { changed, .. }) = &mut self.interaction {
            *changed = true;
        }
        self.dirty = true;
        let _ = record;
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        if self.history.len() == HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.history.push(entry);
        self.redo.clear();
    }

    fn undo(&mut self) {
        let Some(mut entry) = self.history.pop() else {
            return;
        };
        self.toggle_history_entry(&mut entry);
        self.redo.push(entry);
        self.dirty = true;
    }

    fn redo(&mut self) {
        let Some(mut entry) = self.redo.pop() else {
            return;
        };
        self.toggle_history_entry(&mut entry);
        self.history.push(entry);
        self.dirty = true;
    }

    fn toggle_history_entry(&mut self, entry: &mut HistoryEntry) {
        match entry {
            HistoryEntry::Added {
                page_id,
                layer_id,
                id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                let Some(layer) = page.layers.iter_mut().find(|layer| layer.id == *layer_id) else {
                    return;
                };
                if let Some(index) = layer
                    .elements
                    .iter()
                    .position(|element| element.id() == *id)
                {
                    *stored = Some(layer.elements.remove(index));
                } else if let Some(element) = stored.take() {
                    layer.elements.push(element);
                }
            }
            HistoryEntry::ElementsChanged { page_id, slots } => {
                self.apply_element_slots(*page_id, slots);
            }
            HistoryEntry::PageAdded { id, stored } => {
                if let Some(index) = self.notebook.pages.iter().position(|page| page.id == *id) {
                    *stored = Some(self.notebook.pages.remove(index));
                    self.active_page = self.active_page.min(self.notebook.pages.len() - 1);
                } else if let Some(page) = stored.take() {
                    self.notebook.pages.push(page);
                    self.active_page = self.notebook.pages.len() - 1;
                }
                self.active_layer = 0;
            }
            HistoryEntry::PageRemoved { index, stored } => {
                if let Some(page) = stored.take() {
                    self.notebook.trash.retain(|item| item.id != page.id);
                    self.notebook
                        .pages
                        .insert((*index).min(self.notebook.pages.len()), page);
                    self.active_page = (*index).min(self.notebook.pages.len() - 1);
                } else if *index < self.notebook.pages.len() && self.notebook.pages.len() > 1 {
                    let page = self.notebook.pages.remove(*index);
                    self.notebook.trash.push(page.clone());
                    *stored = Some(page);
                    self.active_page = (*index).min(self.notebook.pages.len() - 1);
                }
                self.active_layer = 0;
            }
            HistoryEntry::LayerAdded {
                page_id,
                id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                if let Some(index) = page.layers.iter().position(|layer| layer.id == *id) {
                    *stored = Some(page.layers.remove(index));
                } else if let Some(layer) = stored.take() {
                    page.layers.push(layer);
                }
                self.active_layer = self.active_layer.min(page.layers.len() - 1);
            }
            HistoryEntry::LayerRemoved {
                page_id,
                index,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                if let Some(layer) = stored.take() {
                    page.layers.insert((*index).min(page.layers.len()), layer);
                } else if *index < page.layers.len() && page.layers.len() > 1 {
                    *stored = Some(page.layers.remove(*index));
                }
                self.active_layer = (*index).min(page.layers.len() - 1);
            }
            HistoryEntry::LayerReordered {
                page_id,
                layer_id,
                stored,
            } => {
                let Some(page) = self
                    .notebook
                    .pages
                    .iter_mut()
                    .find(|page| page.id == *page_id)
                else {
                    return;
                };
                let Some(layer) = page.layers.iter_mut().find(|layer| layer.id == *layer_id) else {
                    return;
                };
                std::mem::swap(&mut layer.elements, stored);
            }
        }
        self.selection.clear();
    }
}

fn emit_busy(state: &Rc<RefCell<CanvasState>>) {
    let (busy, listeners) = {
        let state = state.borrow();
        (state.interaction.is_some(), state.busy_listeners.clone())
    };
    for listener in listeners {
        listener(busy);
    }
}

fn emit_replay(state: &Rc<RefCell<CanvasState>>) {
    let (status, listeners) = {
        let state = state.borrow();
        (state.replay_status(), state.replay_listeners.clone())
    };
    for listener in listeners {
        listener(status);
    }
}

fn emit_ink_prefs(state: &Rc<RefCell<CanvasState>>) {
    let listeners = state.borrow().ink_pref_listeners.clone();
    for listener in listeners {
        listener();
    }
}

fn apply_page_defaults_to(page: &mut NotebookPage, defaults: PageDefaults) {
    page.canvas.set_paper_size(defaults.paper);
    page.canvas.set_pattern(defaults.pattern);
    page.canvas.grid_spacing = mm_to_pt(defaults.grid_mm.clamp(1.0, 50.0));
    page.canvas.layout = defaults.layout;
    if defaults.night {
        local::apply_night_paper(page);
    }
    if defaults.template != PageTemplate::Blank {
        local::apply_template(page, defaults.template);
    }
}

fn start_replay_ticks(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let generation = {
        let state = state.borrow();
        state.replay.as_ref().map(|replay| replay.generation)
    };
    let Some(generation) = generation else {
        return;
    };
    let state = state.clone();
    area.add_tick_callback(move |area, _clock| {
        let keep = {
            let mut state = state.borrow_mut();
            let current = state.replay.as_ref().map(|replay| replay.generation);
            match current {
                None => false,
                Some(current) if current != generation => false,
                Some(_) => {
                    let duration = state.replay_duration();
                    if let Some(replay) = state.replay.as_mut() {
                        replay.commit();
                        if duration <= 0.0 || replay.elapsed_secs >= duration {
                            replay.elapsed_secs = duration.max(0.0);
                            replay.playing = false;
                        }
                        replay.playing
                    } else {
                        false
                    }
                }
            }
        };
        emit_replay(&state);
        area.queue_draw();
        if keep {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn emit_text_loaded(state: &Rc<RefCell<CanvasState>>) {
    let (text, listeners) = {
        let mut state = state.borrow_mut();
        if !state.text_loaded {
            return;
        }
        state.text_loaded = false;
        (state.pending_text.clone(), state.text_listeners.clone())
    };
    for listener in listeners {
        listener(text.clone());
    }
}

fn shift_held(controller: &impl gtk::prelude::EventControllerExt) -> bool {
    controller
        .current_event_state()
        .contains(gdk::ModifierType::SHIFT_MASK)
}

fn is_touch_event(controller: &impl gtk::prelude::EventControllerExt) -> bool {
    controller
        .current_event()
        .and_then(|event| event.device())
        .is_some_and(|device| device.source() == gdk::InputSource::Touchscreen)
}

fn constrain_shape_point(
    constrain: bool,
    iso_angle: bool,
    tool: Tool,
    shape_kind: ShapeKind,
    start: Point,
    current: Point,
) -> Point {
    if !constrain {
        return current;
    }
    let kind = if tool == Tool::Measure {
        ShapeKind::Dimension
    } else {
        shape_kind
    };
    if kind.uses_drag_bounds() {
        if iso_angle {
            local::snap_to_iso_angle(start, current)
        } else {
            current
        }
    } else {
        local::constrain_to_square(start, current)
    }
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

fn start_canvas_fx(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    emit_replay(state);
    if !animations_enabled() {
        area.queue_draw();
        return;
    }
    let generation = {
        let mut state = state.borrow_mut();
        state.fx_generation = state.fx_generation.wrapping_add(1);
        state.fx_generation
    };
    let state = state.clone();
    area.add_tick_callback(move |area, _clock| {
        let active = {
            let state = state.borrow();
            if state.fx_generation != generation {
                return glib::ControlFlow::Break;
            }
            fx_active(state.selection_flash, SELECTION_FLASH_SECS)
                || fx_active(state.page_fade, PAGE_FADE_SECS)
                || fx_active(state.empty_hint, EMPTY_HINT_SECS)
        };
        area.queue_draw();
        if active {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn fx_active(started: Option<Instant>, duration: f32) -> bool {
    started.is_some_and(|started| started.elapsed().as_secs_f32() < duration)
}

fn fx_ease(started: Option<Instant>, duration: f32) -> f32 {
    let Some(started) = started else {
        return 1.0;
    };
    let progress = (started.elapsed().as_secs_f32() / duration).clamp(0.0, 1.0);
    1.0 - (1.0 - progress).powi(3)
}

fn emit_view_changed(state: &Rc<RefCell<CanvasState>>) {
    let (percent, listeners) = {
        let state = state.borrow();
        (state.zoom_percent(), state.view_listeners.clone())
    };
    for listener in listeners {
        listener(percent);
    }
}

fn schedule_autosave(state: &Rc<RefCell<CanvasState>>) {
    let (generation, path, delay) = {
        let mut state = state.borrow_mut();
        if !state.dirty || state.path.is_none() || state.runtime.autosave_ms == 0 {
            return;
        }
        state.autosave_generation = state.autosave_generation.wrapping_add(1);
        (
            state.autosave_generation,
            state.path.clone(),
            state.runtime.autosave_ms,
        )
    };
    glib::timeout_add_local_once(Duration::from_millis(delay), {
        let state = state.clone();
        move || {
            let mut state = state.borrow_mut();
            if state.autosave_generation != generation || !state.dirty {
                return;
            }
            if let Some(path) = path
                && state.notebook.save(&path).is_ok()
            {
                state.dirty = false;
            }
        }
    });
}

fn decode_pixbuf(bytes: &[u8]) -> Result<Pixbuf, DocumentError> {
    let loader = PixbufLoader::new();
    loader
        .write(bytes)
        .map_err(|error| DocumentError::Invalid(format!("could not decode image: {error}")))?;
    loader
        .close()
        .map_err(|error| DocumentError::Invalid(format!("could not finish image: {error}")))?;
    loader.pixbuf().ok_or_else(|| {
        DocumentError::Invalid("the selected file did not contain a readable image".to_owned())
    })
}

fn attach_pointer_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let drag = gtk::GestureDrag::new();
    drag.set_button(0);
    drag.connect_drag_begin({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            {
                let mut state = state.borrow_mut();
                if state.stylus_active
                    || gesture
                        .current_event()
                        .and_then(|event| event.device_tool())
                        .is_some()
                    || (state.ignore_touch
                        && is_touch_event(gesture)
                        && state.last_stylus.is_some_and(|at| {
                            at.elapsed() < Duration::from_millis(state.runtime.palm_reject_ms)
                        }))
                {
                    return;
                }
                state.begin_input(
                    Point::new(x as f32, y as f32),
                    1.0,
                    false,
                    gesture.current_button(),
                    shift_held(gesture),
                );
            }
            emit_busy(&state);
            emit_text_loaded(&state);
            start_canvas_fx(&area, &state);
            area.grab_focus();
            area.queue_draw();
        }
    });
    drag.connect_drag_update({
        let state = state.clone();
        let area = area.clone();
        move |gesture, offset_x, offset_y| {
            if state.borrow().stylus_active {
                return;
            }
            let Some((start_x, start_y)) = gesture.start_point() else {
                return;
            };
            state.borrow_mut().update_input(
                Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32),
                1.0,
                shift_held(gesture),
            );
            area.queue_draw();
        }
    });
    drag.connect_drag_end({
        let state = state.clone();
        let area = area.clone();
        move |gesture, offset_x, offset_y| {
            if state.borrow().stylus_active {
                return;
            }
            let Some((start_x, start_y)) = gesture.start_point() else {
                return;
            };
            state.borrow_mut().end_input(
                Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32),
                1.0,
                shift_held(gesture),
            );
            schedule_autosave(&state);
            emit_busy(&state);
            start_canvas_fx(&area, &state);
            area.queue_draw();
        }
    });
    area.add_controller(drag);
}

fn stylus_pressure(state: &CanvasState, gesture: &gtk::GestureStylus, include_tilt: bool) -> f32 {
    if !state.runtime.use_pressure {
        return 1.0;
    }
    let pressure = gesture.axis(gdk::AxisUse::Pressure).unwrap_or(1.0) as f32;
    let tilt = if include_tilt && state.runtime.use_tilt {
        gesture
            .axis(gdk::AxisUse::Xtilt)
            .and_then(|x| gesture.axis(gdk::AxisUse::Ytilt).map(|y| (x, y)))
            .map(|(x, y)| (x.abs() + y.abs()) as f32 * 0.35)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    (pressure + tilt).clamp(0.05, 1.0)
}

fn attach_stylus_input(area: &gtk::DrawingArea, state: &Rc<RefCell<CanvasState>>) {
    let stylus = gtk::GestureStylus::new();
    stylus.connect_down({
        let state = state.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let (pressure, eraser) = {
                let state = state.borrow();
                let pressure = stylus_pressure(&state, gesture, true);
                let eraser = gesture
                    .device_tool()
                    .is_some_and(|tool| tool.tool_type() == gdk::DeviceToolType::Eraser)
                    || (state.runtime.barrel_eraser && gesture.current_button() == 2);
                (pressure, eraser)
            };
            {
                let mut state = state.borrow_mut();
                state.stylus_active = true;
                state.last_stylus = Some(Instant::now());
                state.begin_input(
                    Point::new(x as f32, y as f32),
                    pressure,
                    eraser,
                    1,
                    shift_held(gesture),
                );
            }
            emit_busy(&state);
            emit_text_loaded(&state);
            start_canvas_fx(&area, &state);
            area.grab_focus();
            area.queue_draw();
        }
    });
    stylus.connect_motion({
