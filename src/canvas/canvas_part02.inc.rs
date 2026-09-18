        let mut state = self.state.borrow_mut();
        if state.page().layers.len() == 1 {
            return false;
        }
        let page_id = state.page().id;
        let index = state.active_layer;
        let layer = state.page_mut().layers.remove(index);
        state.push_history(HistoryEntry::LayerRemoved {
            page_id,
            index,
            stored: Some(layer),
        });
        state.active_layer = index.min(state.page().layers.len() - 1);
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
        true
    }

    pub fn toggle_active_layer_visibility(&self) {
        let mut state = self.state.borrow_mut();
        let index = state.active_layer;
        let layer = &mut state.page_mut().layers[index];
        layer.visible = !layer.visible;
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn toggle_active_layer_lock(&self) {
        let mut state = self.state.borrow_mut();
        let index = state.active_layer;
        let layer = &mut state.page_mut().layers[index];
        layer.locked = !layer.locked;
        state.selection.clear();
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn delete_selection(&self) -> usize {
        let mut state = self.state.borrow_mut();
        let count = state.delete_selection();
        drop(state);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn duplicate_selection(&self) -> usize {
        let mut state = self.state.borrow_mut();
        let count = state.duplicate_selection();
        if count > 0 {
            state.flash_selection();
        }
        drop(state);
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        count
    }

    pub fn find_next(&self, query: &str) -> Option<SearchHit> {
        let mut state = self.state.borrow_mut();
        let hits = state.notebook.search(query);
        if hits.is_empty() {
            state.search_position = 0;
            return None;
        }
        let index = state.search_position % hits.len();
        state.search_position = state.search_position.wrapping_add(1);
        let hit = hits[index].clone();
        state.active_page = hit.page_index;
        state.active_layer = state
            .page()
            .layers
            .iter()
            .position(|layer| layer.id == hit.layer_id)
            .unwrap_or(0);
        state.selection.clear();
        state.selection.insert(hit.element_id);
        state.flash_selection();
        state.begin_page_fade();
        let target_bounds = {
            state
                .page()
                .elements()
                .find(|element| element.id() == hit.element_id)
                .map(Element::bounds)
        };
        if let Some(bounds) = target_bounds {
            let center = bounds.center();
            state.pan = Point::new(
                self.area.width() as f32 / 2.0 - center.x * state.zoom,
                self.area.height() as f32 / 2.0 - center.y * state.zoom,
            );
        }
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.area.queue_draw();
        Some(hit)
    }

    pub fn import_media(&self, path: &Path) -> Result<(), DocumentError> {
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
            "" => {
                return Err(DocumentError::Invalid(
                    "the file needs an extension so Inkstone can attach it".to_owned(),
                ));
            }
            other => (MediaKind::File, format!("application/{other}")),
        };
        let bytes = fs::read(path)?;
        let asset_id = Uuid::new_v4();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Embedded file")
            .to_owned();
        let mut state = self.state.borrow_mut();
        let center = state.screen_to_world(Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        ));
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
            media_type: media_type.clone(),
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
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
        Ok(())
    }

    pub fn toggle_grid(&self) {
        let mut state = self.state.borrow_mut();
        let next = if state.page().canvas.pattern() == BackgroundPattern::Grid {
            BackgroundPattern::None
        } else {
            BackgroundPattern::Grid
        };
        state.page_mut().canvas.set_pattern(next);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn pattern(&self) -> BackgroundPattern {
        self.state.borrow().page().canvas.pattern()
    }

    pub fn set_pattern(&self, pattern: BackgroundPattern) {
        let mut state = self.state.borrow_mut();
        if state.page().canvas.pattern() == pattern {
            return;
        }
        state.page_mut().canvas.set_pattern(pattern);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn layout(&self) -> PageLayout {
        self.state.borrow().page().canvas.layout
    }

    pub fn set_layout(&self, layout: PageLayout) {
        let mut state = self.state.borrow_mut();
        if state.page().canvas.layout == layout {
            return;
        }
        state.page_mut().canvas.layout = layout;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn paper_size(&self) -> PaperSize {
        self.state.borrow().page().canvas.paper_size()
    }

    pub fn set_paper_size(&self, size: PaperSize) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.set_paper_size(size);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn grid_spacing_mm(&self) -> f32 {
        pt_to_mm(self.state.borrow().page().canvas.grid_spacing)
    }

    pub fn set_grid_spacing_mm(&self, mm: f32) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.grid_spacing = mm_to_pt(mm.clamp(1.0, 50.0));
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn background_color(&self) -> Color {
        self.state.borrow().page().canvas.background
    }

    pub fn set_background_color(&self, color: Color) {
        let mut state = self.state.borrow_mut();
        state.page_mut().canvas.background = color;
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn page_count(&self) -> usize {
        self.state.borrow().notebook.pages.len()
    }

    pub fn copy_selection(&self) -> Option<String> {
        self.state.borrow().selection_json()
    }

    pub fn cut_selection(&self) -> (Option<String>, usize) {
        let json = self.copy_selection();
        let count = self.delete_selection();
        (json, count)
    }

    pub fn paste_json(&self, json: &str) -> Result<usize, DocumentError> {
        let count = self.state.borrow_mut().paste_json(json)?;
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        Ok(count)
    }

    pub fn bring_selection_to_front(&self) -> usize {
        let count = self.state.borrow_mut().reorder_selection(true);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn send_selection_to_back(&self) -> usize {
        let count = self.state.borrow_mut().reorder_selection(false);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn rotate_selection(&self, degrees: f32) -> usize {
        let count = self.state.borrow_mut().rotate_selection_by(degrees);
        if count > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.area.queue_draw();
        }
        count
    }

    pub fn export_png(&self, path: &Path) -> Result<(), DocumentError> {
        self.export_raster(path, "png")
    }

    pub fn export_jpeg(&self, path: &Path) -> Result<(), DocumentError> {
        self.export_raster(path, "jpeg")
    }

    fn export_raster(&self, path: &Path, format: &str) -> Result<(), DocumentError> {
        let png = {
            let state = self.state.borrow();
            state.render_page_png(state.active_page)?
        };
        if format == "png" {
            fs::write(path, png)?;
            return Ok(());
        }
        let loader = PixbufLoader::new();
        loader
            .write(&png)
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        loader
            .close()
            .map_err(|error| DocumentError::Export(error.to_string()))?;
        let pixbuf = loader
            .pixbuf()
            .ok_or_else(|| DocumentError::Export("could not encode the page image".to_owned()))?;
        pixbuf
            .savev(
                path,
                format,
                &[(
                    "quality",
                    &self.state.borrow().runtime.jpeg_quality.to_string(),
                )],
            )
            .map_err(|error| DocumentError::Export(error.to_string()))
    }

    pub fn import_svg(&self, path: &Path) -> Result<(), DocumentError> {
        let svg = fs::read_to_string(path)?;
        let elements = import_svg_elements(&svg)?;
        let mut state = self.state.borrow_mut();
        if state.active_layer().locked {
            return Err(DocumentError::Invalid(
                "the active layer is locked".to_owned(),
            ));
        }
        for element in elements {
            state.add_element(element);
        }
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
        Ok(())
    }

    pub fn import_path(&self, path: &Path) -> Result<(), DocumentError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "svg" => self.import_svg(path),
            "pdf" => {
                let pages = self.import_pdf_as_pages(path)?;
                if pages == 0 {
                    self.import_media(path)
                } else {
                    Ok(())
                }
            }
            _ => self.import_media(path),
        }
    }

    pub fn render_page_to(
        &self,
        context: &Context,
        page_index: usize,
        width: f64,
        height: f64,
    ) -> Result<(), DocumentError> {
        self.state
            .borrow()
            .render_page(context, page_index, width, height)
    }

    fn schedule_autosave(&self) {
        schedule_autosave(&self.state);
    }

    fn world_center(&self) -> Point {
        self.state.borrow().screen_to_world(Point::new(
            self.area.width().max(1) as f32 / 2.0,
            self.area.height().max(1) as f32 / 2.0,
        ))
    }

    pub fn reset_view(&self) {
        let mut state = self.state.borrow_mut();
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        state.pan = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        state.zoom = state
            .runtime
            .startup_zoom
            .clamp(state.runtime.min_zoom, state.runtime.max_zoom);
        drop(state);
        self.emit_view_changed();
        self.area.queue_draw();
    }

    pub fn zoom_by(&self, factor: f32) {
        let center = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        let mut state = self.state.borrow_mut();
        state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
        let requested = state.zoom * factor;
        state.set_zoom_around(requested, center);
        drop(state);
        self.emit_view_changed();
        self.area.queue_draw();
    }

    pub fn animate_zoom_by(&self, factor: f32) -> u32 {
        let animate = self.state.borrow().runtime.animate_zoom;
        if !animate
            || gtk::Settings::default().is_none_or(|settings| !settings.is_gtk_enable_animations())
            || self.area.frame_clock().is_none()
        {
            self.zoom_by(factor);
            return self.zoom_percent();
        }
        let center = Point::new(
            self.area.width() as f32 / 2.0,
            self.area.height() as f32 / 2.0,
        );
        let (start_zoom, target_zoom, generation) = {
            let mut state = self.state.borrow_mut();
            state.view_animation_generation = state.view_animation_generation.wrapping_add(1);
            (
                state.zoom,
                (state.zoom * factor).clamp(state.runtime.min_zoom, state.runtime.max_zoom),
                state.view_animation_generation,
            )
        };
        let started = Instant::now();
        let state = self.state.clone();
        self.area.add_tick_callback(move |area, _clock| {
            let elapsed = started.elapsed().as_secs_f32();
            let progress = (elapsed / 0.18).min(1.0);
            let eased = 1.0 - (1.0 - progress).powi(3);
            {
                let mut state = state.borrow_mut();
                if state.view_animation_generation != generation {
                    return glib::ControlFlow::Break;
                }
                state.set_zoom_around(start_zoom + (target_zoom - start_zoom) * eased, center);
            }
            emit_view_changed(&state);
            area.queue_draw();
            if progress >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        (target_zoom * 100.0).round() as u32
    }

    pub fn zoom_percent(&self) -> u32 {
        self.state.borrow().zoom_percent()
    }

    pub fn connect_view_changed(&self, callback: impl Fn(u32) + 'static) {
        let callback = Rc::new(callback);
        self.state
            .borrow_mut()
            .view_listeners
            .push(callback.clone());
        callback(self.zoom_percent());
    }

    pub fn connect_text_loaded(&self, callback: impl Fn(String) + 'static) {
        self.state
            .borrow_mut()
            .text_listeners
            .push(Rc::new(callback));
    }

    pub fn pending_text(&self) -> String {
        self.state.borrow().pending_text.clone()
    }

    pub fn connect_busy(&self, callback: impl Fn(bool) + 'static) {
        self.state
            .borrow_mut()
            .busy_listeners
            .push(Rc::new(callback));
    }

    pub fn apply_night_paper(&self) {
        let mut state = self.state.borrow_mut();
        local::apply_night_paper(state.page_mut());
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        self.area.queue_draw();
    }

    pub fn align_selection(&self, mode: AlignMode) -> usize {
        let count = self.state.borrow_mut().align_selection(mode);
        if count > 0 {
            self.schedule_autosave();
            self.area.queue_draw();
        }
        count
    }

    pub fn copy_selection_svg(&self) -> Option<String> {
        self.state.borrow().selection_svg()
    }

    pub fn copy_selection_png(&self) -> Result<Option<Vec<u8>>, DocumentError> {
        self.state.borrow().selection_png()
    }

    pub fn export_layers_folder(&self, dir: &Path) -> Result<usize, DocumentError> {
        let (notebook, page_index) = {
            let state = self.state.borrow();
            (state.notebook.clone(), state.active_page)
        };
        fs::create_dir_all(dir)?;
        let page = notebook
            .pages
            .get(page_index)
            .ok_or_else(|| DocumentError::Invalid("the current page is missing".to_owned()))?;
        for (index, layer) in page.layers.iter().enumerate() {
            let name = format!("{:02} {}.svg", index + 1, layer.name);
            notebook.export_layer_svg(&dir.join(name), page_index, index)?;
        }
        Ok(page.layers.len())
    }

    pub fn export_notebook_folder(&self, dir: &Path) -> Result<(), DocumentError> {
        let notebook = self.state.borrow().notebook.clone();
        notebook.export_folder(dir)?;
        for (index, page) in notebook.pages.iter().enumerate() {
            let stem = format!("{:02} {}", index + 1, page.title);
            let png = {
                let state = self.state.borrow();
                state.render_page_png(index).ok()
            };
            if let Some(png) = png {
                let name = sanitize_export_name(&format!("{stem}.png"));
                fs::write(dir.join(name), png)?;
            }
        }
        Ok(())
    }

    pub fn todos(&self) -> Vec<local::TodoItem> {
        local::collect_todos(&self.state.borrow().notebook)
    }

    pub fn copy_page_link(&self) -> String {
        let state = self.state.borrow();
        local::page_link_href(state.page().id)
    }

    pub fn insert_page_link(&self) {
        let link = {
            let state = self.state.borrow();
            format!("[[{}]]", state.page().title)
        };
        self.set_text(link);
    }

    pub fn goto_page_link(&self, href: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let index = if let Some(id) = local::parse_page_link_href(href) {
            state.notebook.page_by_id(id)
        } else {
            local::parse_page_links(href)
                .into_iter()
                .find_map(|name| local::resolve_page_name(&state.notebook, &name))
        };
        let Some(index) = index else {
            return false;
        };
        state.active_page = index;
        state.active_layer = 0;
        state.begin_page_fade();
        drop(state);
        start_canvas_fx(&self.area, &self.state);
        self.area.queue_draw();
        true
    }

    pub fn notebook_color(&self) -> Option<Color> {
        self.state.borrow().notebook.color
    }

    pub fn set_notebook_color(&self, color: Color) {
        let mut state = self.state.borrow_mut();
        state.notebook.color = Some(color);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
    }

    pub fn cycle_notebook_color(&self) -> Color {
        let mut state = self.state.borrow_mut();
        let current = state.notebook.color.unwrap_or(Color::BLUE);
        let next = crate::library::NOTEBOOK_SWATCHES
            .iter()
            .position(|color| *color == current)
            .map(|index| crate::library::NOTEBOOK_SWATCHES[(index + 1) % 8])
            .unwrap_or(crate::library::NOTEBOOK_SWATCHES[0]);
        state.notebook.color = Some(next);
        state.dirty = true;
        drop(state);
        self.schedule_autosave();
        next
    }

    pub fn import_pdf_as_pages(&self, path: &Path) -> Result<usize, DocumentError> {
        const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
        if fs::metadata(path)?.len() > MAX_ASSET_BYTES {
            return Err(DocumentError::Invalid(
                "embedded files are limited to 64 MiB".to_owned(),
            ));
        }
        let bytes = fs::read(path)?;
        let dpi = self.state.borrow().runtime.pdf_dpi;
        let rasters = pdf::rasterize_pdf_pages(&bytes, dpi).unwrap_or_default();
        let page_count = rasters.len().max(pdf::count_pdf_pages(&bytes)).max(1);
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("PDF")
            .to_owned();
        let mut state = self.state.borrow_mut();
        let section = state.notebook.active_section;
        let mut created = 0;
        for index in 0..page_count {
            let mut page = NotebookPage::named(format!("{name} {}", index + 1));
            page.section_id = section;
            page.canvas.set_paper_size(PaperSize::A4);
            let asset_id = Uuid::new_v4();
            if let Some(png) = rasters.get(index) {
                if let Ok(pixbuf) = decode_pixbuf(png) {
                    let width = page.canvas.page_width;
                    let height = width * pixbuf.height() as f32 / pixbuf.width().max(1) as f32;
                    page.canvas.page_height = height;
                    state.image_cache.insert(asset_id, pixbuf);
                    state.notebook.assets.push(Asset {
                        id: asset_id,
                        name: format!("{name}-{}.png", index + 1),
                        media_type: "image/png".to_owned(),
                        data_base64: base64::engine::general_purpose::STANDARD.encode(png),
                    });
                    page.layers[0].elements.push(Element::Media(MediaElement {
                        id: Uuid::new_v4(),
                        asset_id,
                        kind: MediaKind::Image,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height,
                        },
                        alt_text: format!("{name} page {}", index + 1),
                        caption: format!("{name} page {}", index + 1),
                    }));
                }
            } else {
                state.notebook.assets.push(Asset {
                    id: asset_id,
                    name: format!("{name}.pdf"),
                    media_type: "application/pdf".to_owned(),
                    data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                });
                let width = page.canvas.page_width;
                let height = page.canvas.page_height;
                page.layers[0].elements.push(Element::Media(MediaElement {
                    id: Uuid::new_v4(),
                    asset_id,
                    kind: MediaKind::Pdf,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                    alt_text: format!("{name} page {}", index + 1),
                    caption: format!("{name} page {}", index + 1),
                }));
            }
            let id = page.id;
            state.notebook.pages.push(page);
            state.push_history(HistoryEntry::PageAdded { id, stored: None });
            created += 1;
            if rasters.is_empty() {
                break;
            }
        }
        if created > 0 {
            state.active_page = state.notebook.pages.len() - created;
            state.active_layer = 0;
            state.dirty = true;
        }
        drop(state);
        if created > 0 {
            self.schedule_autosave();
            start_canvas_fx(&self.area, &self.state);
            self.reset_view();
        }
        Ok(created)
    }

    pub fn start_audio_capture(&self) -> Result<(), DocumentError> {
        let mut state = self.state.borrow_mut();
        if state.audio_child.is_some() {
            return Ok(());
        }
        let Some((bin, args)) = pdf::record_command() else {
            return Err(DocumentError::Invalid(
                "no local recorder (pw-record, parecord, or arecord) is installed".to_owned(),
            ));
        };
        let path = std::env::temp_dir().join(format!("inkstone-{}.wav", Uuid::new_v4()));
        let mut command = std::process::Command::new(bin);
        command.args(args).arg(&path);
        let child = command
            .spawn()
            .map_err(|error| DocumentError::Invalid(format!("could not start {bin}: {error}")))?;
        state.audio_child = Some(child);
        state.audio_path = Some(path);
        Ok(())
    }

    pub fn stop_audio_capture(&self) -> Result<bool, DocumentError> {
        let (path, mut child) = {
            let mut state = self.state.borrow_mut();
            match (state.audio_path.take(), state.audio_child.take()) {
                (Some(path), Some(child)) => (path, child),
                _ => return Ok(false),
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        if path.exists() {
            self.import_media(&path)?;
            let _ = fs::remove_file(&path);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn is_recording_audio(&self) -> bool {
        self.state.borrow().audio_child.is_some()
    }

    pub fn render_page_thumbnail(
        &self,
        page_index: usize,
        width: i32,
        height: i32,
    ) -> Option<Pixbuf> {
        let png = self.state.borrow().render_page_png(page_index).ok()?;
        let loader = PixbufLoader::new();
        loader.write(&png).ok()?;
        loader.close().ok()?;
        let pixbuf = loader.pixbuf()?;
        Some(
            pixbuf
                .scale_simple(
                    width.max(1),
                    height.max(1),
                    gdk_pixbuf::InterpType::Bilinear,
                )
                .unwrap_or(pixbuf),
        )
    }

    fn emit_view_changed(&self) {
        emit_view_changed(&self.state);
    }
}

impl CanvasState {
    fn zoom_percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }

    fn flash_selection(&mut self) {
        if self.runtime.selection_flash {
            self.selection_flash = Some(Instant::now());
        }
    }

    fn begin_page_fade(&mut self) {
        self.replay = None;
        if self.runtime.page_fade {
            self.page_fade = Some(Instant::now());
        }
        if self.runtime.show_empty_hint && self.page().visible_elements().next().is_none() {
            self.empty_hint = Some(Instant::now());
        }
    }

    fn replay_duration(&self) -> f32 {
        local::replay_duration(&local::replay_timeline(self.page().visible_elements()))
    }

    fn replay_status(&self) -> ReplayStatus {
        let Some(replay) = self.replay.as_ref() else {
            return ReplayStatus {
                speed: self.replay_speed,
                ..ReplayStatus::idle()
            };
        };
        let duration = self.replay_duration();
        let elapsed = replay.seconds().clamp(0.0, duration.max(0.0));
        let finished = duration > 0.0 && elapsed >= duration - 0.0005;
        ReplayStatus {
            active: true,
            playing: replay.playing && !finished,
            finished,
            progress: if duration > 0.0 {
                (elapsed / duration).clamp(0.0, 1.0)
            } else {
                1.0
            },
            speed: replay.speed,
            elapsed_secs: elapsed,
            duration_secs: duration,
        }
    }

    fn start_replay(&mut self) -> bool {
        let duration = self.replay_duration();
        if duration <= 0.0 {
            return false;
        }
        self.selection.clear();
        self.replay_generation = self.replay_generation.wrapping_add(1);
        self.replay = Some(ReplayPlayback {
            elapsed_secs: 0.0,
            last_tick: Instant::now(),
            playing: true,
            speed: self.replay_speed,
            generation: self.replay_generation,
        });
        true
    }

    fn page(&self) -> &NotebookPage {
        &self.notebook.pages[self.active_page]
    }

    fn page_mut(&mut self) -> &mut NotebookPage {
        &mut self.notebook.pages[self.active_page]
    }

    fn active_layer(&self) -> &Layer {
        &self.page().layers[self.active_layer]
    }

    fn active_layer_mut(&mut self) -> &mut Layer {
        let page = self.active_page;
        let layer = self.active_layer;
        &mut self.notebook.pages[page].layers[layer]
    }

    fn rebuild_image_cache(&mut self) {
        self.image_cache.clear();
        for asset in &self.notebook.assets {
            if asset.media_type.starts_with("image/")
                && let Ok(bytes) = asset.decoded()
                && let Ok(pixbuf) = decode_pixbuf(&bytes)
            {
                self.image_cache.insert(asset.id, pixbuf);
            }
        }
    }

    fn hit_test(&self, point: Point) -> Option<Uuid> {
        self.page()
            .layers
            .iter()
            .rev()
            .filter(|layer| layer.visible && !layer.locked)
            .flat_map(|layer| layer.elements.iter().rev())
            .find(|element| element.bounds().expand(6.0 / self.zoom).contains(point))
            .map(Element::id)
    }

    fn capture_elements(&self, ids: &HashSet<Uuid>) -> Vec<ElementSlot> {
        let mut affected = ids.clone();
        for element in self.page().elements() {
            if let Element::Connector(connector) = element
                && [&connector.start, &connector.end]
                    .into_iter()
                    .filter_map(|endpoint| endpoint.attachment.as_ref())
                    .any(|attachment| ids.contains(&attachment.element_id))
            {
                affected.insert(connector.id);
            }
        }
        let mut slots = Vec::new();
        for layer in &self.page().layers {
            for (index, element) in layer.elements.iter().enumerate() {
                if affected.contains(&element.id()) {
                    slots.push(ElementSlot {
                        layer_id: layer.id,
                        index,
                        id: element.id(),
                        stored: Some(element.clone()),
                    });
                }
            }
        }
        slots
    }

    fn translate_selection(&mut self, delta: Point) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.translate(delta);
                    continue;
                }
                if let Element::Connector(connector) = element {
                    let mut changed = false;
                    if connector
                        .start
                        .attachment
                        .as_ref()
                        .is_some_and(|attachment| selected.contains(&attachment.element_id))
                    {
                        connector.start.point.x += delta.x;
                        connector.start.point.y += delta.y;
                        changed = true;
                    }
                    if connector
                        .end
                        .attachment
                        .as_ref()
                        .is_some_and(|attachment| selected.contains(&attachment.element_id))
                    {
                        connector.end.point.x += delta.x;
                        connector.end.point.y += delta.y;
                        changed = true;
                    }
                    if changed {
                        let midpoint_x = (connector.start.point.x + connector.end.point.x) / 2.0;
                        connector.route = vec![
                            Point::new(midpoint_x, connector.start.point.y),
                            Point::new(midpoint_x, connector.end.point.y),
                        ];
                    }
                }
            }
        }
    }

    fn delete_selection(&mut self) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let slots = self.capture_elements(&selected);
        let count = selected.len();
        for layer in &mut self.page_mut().layers {
            layer
                .elements
                .retain(|element| !selected.contains(&element.id()));
            for element in &mut layer.elements {
                if let Element::Connector(connector) = element {
                    for endpoint in [&mut connector.start, &mut connector.end] {
                        if endpoint
                            .attachment
                            .as_ref()
                            .is_some_and(|attachment| selected.contains(&attachment.element_id))
                        {
                            endpoint.attachment = None;
                        }
                    }
                }
            }
        }
        let page_id = self.page().id;
        self.selection.clear();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        count
    }

    fn duplicate_selection(&mut self) -> usize {
        if self.selection.is_empty() || self.active_layer().locked {
            return 0;
        }
        let selected = self.selection.clone();
        let originals: Vec<Element> = self
            .page()
            .elements()
            .filter(|element| selected.contains(&element.id()))
            .cloned()
            .collect();
        let id_map: HashMap<Uuid, Uuid> = originals
            .iter()
            .map(|element| (element.id(), Uuid::new_v4()))
            .collect();
        let mut copies = Vec::with_capacity(originals.len());
        for mut element in originals {
            let new_id = id_map[&element.id()];
            element.set_id(new_id);
            element.translate(Point::new(24.0, 24.0));
            if let Element::Connector(connector) = &mut element {
                for endpoint in [&mut connector.start, &mut connector.end] {
                    if let Some(attachment) = &mut endpoint.attachment
                        && let Some(new_target) = id_map.get(&attachment.element_id)
                    {
                        attachment.element_id = *new_target;
                    }
                }
            }
            copies.push(element);
        }
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let start_index = self.active_layer().elements.len();
        let mut slots = Vec::with_capacity(copies.len());
        self.selection.clear();
        for (offset, element) in copies.into_iter().enumerate() {
            let id = element.id();
            self.active_layer_mut().elements.push(element);
            self.selection.insert(id);
            slots.push(ElementSlot {
                layer_id,
                index: start_index + offset,
                id,
                stored: None,
            });
        }
        let count = slots.len();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        count
    }

    fn apply_element_slots(&mut self, page_id: Uuid, slots: &mut [ElementSlot]) {
        let Some(page_index) = self
            .notebook
            .pages
            .iter()
            .position(|page| page.id == page_id)
        else {
            return;
        };
        self.active_page = page_index;
        for slot in slots {
            let Some(layer_index) = self.notebook.pages[page_index]
                .layers
                .iter()
                .position(|layer| layer.id == slot.layer_id)
            else {
                continue;
            };
            let elements = &mut self.notebook.pages[page_index].layers[layer_index].elements;
            if let Some(current_index) = elements.iter().position(|element| element.id() == slot.id)
            {
                if let Some(stored) = &mut slot.stored {
                    std::mem::swap(&mut elements[current_index], stored);
                } else {
                    slot.stored = Some(elements.remove(current_index));
                }
            } else if let Some(stored) = slot.stored.take() {
                elements.insert(slot.index.min(elements.len()), stored);
            }
        }
        self.selection.clear();
    }

    fn restore_slots(&mut self, slots: &[ElementSlot]) {
        let page_index = self.active_page;
        for slot in slots {
            let Some(layer_index) = self.notebook.pages[page_index]
                .layers
                .iter()
                .position(|layer| layer.id == slot.layer_id)
            else {
                continue;
            };
            let Some(stored) = &slot.stored else {
                continue;
            };
            if let Some(element) = self.notebook.pages[page_index].layers[layer_index]
                .elements
                .iter_mut()
                .find(|element| element.id() == slot.id)
            {
                *element = stored.clone();
            }
        }
    }

    fn scale_selection(&mut self, origin: Point, scale_x: f32, scale_y: f32) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.scale_from(origin, scale_x, scale_y);
                }
            }
        }
    }

    fn rotate_selection(&mut self, center: Point, degrees: f32) {
        let selected = self.selection.clone();
        for layer in &mut self.page_mut().layers {
            for element in &mut layer.elements {
                if selected.contains(&element.id()) {
                    element.rotate_around(center, degrees);
                }
            }
        }
    }

    fn rotate_selection_by(&mut self, degrees: f32) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let before = self.capture_elements(&selected);
        let Some(bounds) = self.selection_bounds() else {
            return 0;
        };
        self.rotate_selection(bounds.center(), degrees);
        let page_id = self.page().id;
        let count = selected.len();
        self.push_history(HistoryEntry::ElementsChanged {
            page_id,
            slots: before,
        });
        self.dirty = true;
        self.flash_selection();
        count
    }

    fn shift_below(&mut self, threshold: f32, delta_y: f32) {
        for layer in &mut self.page_mut().layers {
            if layer.locked {
                continue;
            }
            for element in &mut layer.elements {
                if element.bounds().y >= threshold - 0.5 {
                    element.translate(Point::new(0.0, delta_y));
                }
            }
        }
    }

    fn selection_bounds(&self) -> Option<Rect> {
        self.page()
            .visible_elements()
            .filter(|element| self.selection.contains(&element.id()))
            .map(Element::bounds)
            .reduce(Rect::union)
    }

    fn hit_transform_handle(&self, world: Point) -> Option<TransformHandle> {
        if self.selection.is_empty() {
            return None;
        }
        let bounds = self.selection_bounds()?.expand(6.0 / self.zoom);
        let tolerance = 10.0 / self.zoom;
        let corners = [
            (
                Point::new(bounds.x, bounds.y),
                Point::new(bounds.x + bounds.width, bounds.y + bounds.height),
            ),
            (
                Point::new(bounds.x + bounds.width, bounds.y),
                Point::new(bounds.x, bounds.y + bounds.height),
            ),
            (
                Point::new(bounds.x, bounds.y + bounds.height),
                Point::new(bounds.x + bounds.width, bounds.y),
            ),
            (
                Point::new(bounds.x + bounds.width, bounds.y + bounds.height),
                Point::new(bounds.x, bounds.y),
            ),
        ];
        for (handle, origin) in corners {
            if handle.distance_to(world) <= tolerance {
                return Some(TransformHandle::Resize { origin });
            }
        }
        let rotate = Point::new(bounds.center().x, bounds.y - 22.0 / self.zoom);
        if rotate.distance_to(world) <= tolerance {
            return Some(TransformHandle::Rotate {
                center: bounds.center(),
            });
        }
        None
    }

    fn selection_json(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let elements: Vec<Element> = self
            .page()
            .elements()
            .filter(|element| self.selection.contains(&element.id()))
            .cloned()
            .collect();
        if elements.is_empty() {
            return None;
        }
        let mut asset_ids = HashSet::new();
        for element in &elements {
            if let Element::Media(media) = element {
                asset_ids.insert(media.asset_id);
            }
        }
        let assets = self
            .notebook
            .assets
            .iter()
            .filter(|asset| asset_ids.contains(&asset.id))
            .cloned()
            .collect();
        serde_json::to_string(&ClipboardPayload {
            format: CLIPBOARD_FORMAT.to_owned(),
            version: 1,
            elements,
            assets,
        })
        .ok()
    }

    fn paste_json(&mut self, json: &str) -> Result<usize, DocumentError> {
        if self.active_layer().locked {
            return Err(DocumentError::Invalid(
                "the active layer is locked".to_owned(),
            ));
        }
        let payload: ClipboardPayload = serde_json::from_str(json).map_err(|_| {
            DocumentError::Invalid("clipboard does not contain Inkstone objects".to_owned())
        })?;
        if payload.format != CLIPBOARD_FORMAT || payload.elements.is_empty() {
            return Err(DocumentError::Invalid(
                "clipboard does not contain Inkstone objects".to_owned(),
            ));
        }
        let mut asset_map = HashMap::new();
        for asset in payload.assets {
            let new_id = Uuid::new_v4();
            asset_map.insert(asset.id, new_id);
            self.notebook.assets.push(Asset {
                id: new_id,
                ..asset
            });
            if let Ok(bytes) = self.notebook.assets.last().unwrap().decoded()
                && let Ok(pixbuf) = decode_pixbuf(&bytes)
            {
                self.image_cache.insert(new_id, pixbuf);
            }
        }
        let id_map: HashMap<Uuid, Uuid> = payload
            .elements
            .iter()
            .map(|element| (element.id(), Uuid::new_v4()))
            .collect();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let start_index = self.active_layer().elements.len();
        let mut slots = Vec::new();
        self.selection.clear();
        for (offset, mut element) in payload.elements.into_iter().enumerate() {
            let new_id = id_map[&element.id()];
            element.set_id(new_id);
            element.translate(Point::new(24.0, 24.0));
            if let Element::Connector(connector) = &mut element {
                for endpoint in [&mut connector.start, &mut connector.end] {
                    if let Some(attachment) = &mut endpoint.attachment
                        && let Some(new_target) = id_map.get(&attachment.element_id)
                    {
                        attachment.element_id = *new_target;
                    }
                }
            }
            if let Element::Media(media) = &mut element
                && let Some(new_asset) = asset_map.get(&media.asset_id)
            {
                media.asset_id = *new_asset;
            }
            self.selection.insert(new_id);
            self.active_layer_mut().elements.push(element);
            slots.push(ElementSlot {
                layer_id,
                index: start_index + offset,
                id: new_id,
                stored: None,
            });
        }
        let count = slots.len();
        self.push_history(HistoryEntry::ElementsChanged { page_id, slots });
        self.dirty = true;
        self.flash_selection();
        Ok(count)
    }

    fn reorder_selection(&mut self, to_front: bool) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selected = self.selection.clone();
        let page_id = self.page().id;
        let layer_id = self.active_layer().id;
        let stored = self.active_layer().elements.clone();
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for element in self.active_layer_mut().elements.drain(..) {
            if selected.contains(&element.id()) {
                moved.push(element);
            } else {
                kept.push(element);
            }
        }
        let count = moved.len();
        if count == 0 {
            self.active_layer_mut().elements = kept;
            return 0;
        }
        self.active_layer_mut().elements = if to_front {
            kept.extend(moved);
            kept
        } else {
            moved.extend(kept);
            moved
        };
        self.push_history(HistoryEntry::LayerReordered {
            page_id,
            layer_id,
            stored,
        });
