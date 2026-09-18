use crate::document::{
    Anchor, Attachment, CanvasSettings, Document, DocumentError, Element, Endpoint, FORMAT_NAME,
    MediaKind, Point, Rect,
};
use crate::spreadsheet::SpreadsheetLayer;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub const NOTEBOOK_FORMAT: &str = "inkstone.notebook";
pub const NOTEBOOK_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Asset {
    pub id: Uuid,
    pub name: String,
    pub media_type: String,
    pub data_base64: String,
}

impl Asset {
    pub fn decoded(&self) -> Result<Vec<u8>, DocumentError> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.data_base64)
            .map_err(|error| {
                DocumentError::Invalid(format!("asset {} has invalid base64: {error}", self.id))
            })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerType {
    #[default]
    Canvas,
    Excel,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Layer {
    pub id: Uuid,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub layer_type: LayerType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spreadsheet: Option<SpreadsheetLayer>,
}

impl Layer {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            visible: true,
            locked: false,
            layer_type: LayerType::Canvas,
            elements: Vec::new(),
            spreadsheet: None,
        }
    }

    pub fn excel(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: Uuid::new_v4(),
            name: name.clone(),
            visible: true,
            locked: false,
            layer_type: LayerType::Excel,
            elements: Vec::new(),
            spreadsheet: Some(SpreadsheetLayer {
                sheets: vec![crate::spreadsheet::Sheet {
                    name,
                    ..Default::default()
                }],
                ..SpreadsheetLayer::default()
            }),
        }
    }

    pub fn is_excel(&self) -> bool {
        self.layer_type == LayerType::Excel
    }

    pub fn is_canvas(&self) -> bool {
        self.layer_type == LayerType::Canvas
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NotebookPage {
    pub id: Uuid,
    pub title: String,
    pub canvas: CanvasSettings,
    pub layers: Vec<Layer>,
}

impl NotebookPage {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: name.into(),
            canvas: CanvasSettings::default(),
            layers: vec![Layer::named("Notes")],
        }
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.layers.iter().flat_map(|layer| layer.elements.iter())
    }

    pub fn visible_elements(&self) -> impl Iterator<Item = &Element> {
        self.layers
            .iter()
            .filter(|layer| layer.visible && layer.is_canvas())
            .flat_map(|layer| layer.elements.iter())
    }

    pub fn visible_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(|layer| layer.visible)
    }

    pub fn element_mut(&mut self, id: Uuid) -> Option<&mut Element> {
        self.layers
            .iter_mut()
            .flat_map(|layer| layer.elements.iter_mut())
            .find(|element| element.id() == id)
    }

    pub fn content_bounds(&self) -> Option<Rect> {
        let element_bounds = self
            .visible_elements()
            .map(Element::bounds)
            .reduce(Rect::union);
        let spreadsheet_bounds = self
            .visible_layers()
            .filter_map(|layer| layer.spreadsheet.as_ref())
            .map(SpreadsheetLayer::content_bounds)
            .reduce(Rect::union);
        match (element_bounds, spreadsheet_bounds) {
            (Some(left), Some(right)) => Some(left.union(right)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        }
    }

    pub fn snap_endpoint(&self, target: Point, max_distance: f32) -> Endpoint {
        let mut best: Option<(f32, Uuid, Anchor, Point)> = None;
        for element in self.visible_elements() {
            if matches!(element, Element::Connector(_)) {
                continue;
            }
            for (anchor, point) in element.anchors() {
                let distance = target.distance_to(point);
                if distance <= max_distance
                    && best
                        .as_ref()
                        .is_none_or(|(best_distance, ..)| distance < *best_distance)
                {
                    best = Some((distance, element.id(), anchor, point));
                }
            }
        }
        if let Some((_, element_id, anchor, point)) = best {
            Endpoint {
                point,
                attachment: Some(Attachment { element_id, anchor }),
            }
        } else {
            Endpoint {
                point: target,
                attachment: None,
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Notebook {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub pages: Vec<NotebookPage>,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub page_index: usize,
    pub page_title: String,
    pub layer_id: Uuid,
    pub element_id: Uuid,
    pub snippet: String,
}

impl Default for Notebook {
    fn default() -> Self {
        Self {
            format: NOTEBOOK_FORMAT.to_owned(),
            version: NOTEBOOK_VERSION,
            title: "Untitled notebook".to_owned(),
            pages: vec![NotebookPage::named("Page 1")],
            assets: Vec::new(),
        }
    }
}

impl Notebook {
    pub fn load(path: &Path) -> Result<Self, DocumentError> {
        let bytes = fs::read(path)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let format = value
            .get("format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let notebook = match format {
            NOTEBOOK_FORMAT => serde_json::from_value(value)?,
            FORMAT_NAME => Self::from_legacy(serde_json::from_value(value)?),
            other => return Err(DocumentError::Unsupported(other.to_owned())),
        };
        notebook.validate()?;
        Ok(notebook)
    }

    pub fn save(&self, path: &Path) -> Result<(), DocumentError> {
        self.validate()?;
        let temporary = path.with_extension("inkstone.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn from_legacy(document: Document) -> Self {
        let Document {
            title,
            canvas,
            elements,
            ..
        } = document;
        Self {
            format: NOTEBOOK_FORMAT.to_owned(),
            version: NOTEBOOK_VERSION,
            title: title.clone(),
            pages: vec![NotebookPage {
                id: Uuid::new_v4(),
                title,
                canvas,
                layers: vec![Layer {
                    id: Uuid::new_v4(),
                    name: "Imported notes".to_owned(),
                    visible: true,
                    locked: false,
                    layer_type: LayerType::Canvas,
                    elements,
                    spreadsheet: None,
                }],
            }],
            assets: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.format != NOTEBOOK_FORMAT || self.version != NOTEBOOK_VERSION {
            return Err(DocumentError::Unsupported(format!(
                "{} version {}",
                self.format, self.version
            )));
        }
        if self.pages.is_empty() {
            return Err(DocumentError::Invalid(
                "a notebook must contain at least one page".to_owned(),
            ));
        }

        let mut structural_ids = HashSet::new();
        let mut asset_ids = HashSet::new();
        for asset in &self.assets {
            if !asset_ids.insert(asset.id) || !structural_ids.insert(asset.id) {
                return Err(DocumentError::Invalid(format!(
                    "duplicate asset id {}",
                    asset.id
                )));
            }
            if asset.name.trim().is_empty() || asset.media_type.trim().is_empty() {
                return Err(DocumentError::Invalid(format!(
                    "asset {} is missing metadata",
                    asset.id
                )));
            }
            asset.decoded()?;
        }

        for page in &self.pages {
            if !structural_ids.insert(page.id) || page.layers.is_empty() {
                return Err(DocumentError::Invalid(format!(
                    "page {} is duplicated or has no layers",
                    page.id
                )));
            }
            if !page.canvas.background.is_valid()
                || !page.canvas.grid_spacing.is_finite()
                || page.canvas.grid_spacing <= 0.0
            {
                return Err(DocumentError::Invalid(format!(
                    "page {} has invalid canvas settings",
                    page.id
                )));
            }

            let mut page_element_ids = HashSet::new();
            for layer in &page.layers {
                if !structural_ids.insert(layer.id) {
                    return Err(DocumentError::Invalid(format!(
                        "duplicate layer id {}",
                        layer.id
                    )));
                }
                match layer.layer_type {
                    LayerType::Canvas => {
                        if layer.spreadsheet.is_some() {
                            return Err(DocumentError::Invalid(format!(
                                "canvas layer {} must not include spreadsheet data",
                                layer.id
                            )));
                        }
                    }
                    LayerType::Excel => {
                        let Some(spreadsheet) = &layer.spreadsheet else {
                            return Err(DocumentError::Invalid(format!(
                                "excel layer {} is missing spreadsheet data",
                                layer.id
                            )));
                        };
                        if !layer.elements.is_empty() {
                            return Err(DocumentError::Invalid(format!(
                                "excel layer {} must not contain canvas elements",
                                layer.id
                            )));
                        }
                        spreadsheet.validate()?;
                    }
                }
                for element in &layer.elements {
                    if !page_element_ids.insert(element.id())
                        || !structural_ids.insert(element.id())
                    {
                        return Err(DocumentError::Invalid(format!(
                            "duplicate element id {}",
                            element.id()
                        )));
                    }
                    Document::validate_element(element)?;
                    if let Element::Media(media) = element
                        && !asset_ids.contains(&media.asset_id)
                    {
                        return Err(DocumentError::Invalid(format!(
                            "media {} references missing asset {}",
                            media.id, media.asset_id
                        )));
                    }
                }
            }
            for element in page.elements() {
                if let Element::Connector(connector) = element {
                    for endpoint in [&connector.start, &connector.end] {
                        if let Some(attachment) = &endpoint.attachment
                            && !page_element_ids.contains(&attachment.element_id)
                        {
                            return Err(DocumentError::Invalid(format!(
                                "connector {} references missing page element {}",
                                connector.id, attachment.element_id
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (page_index, page) in self.pages.iter().enumerate() {
            for layer in &page.layers {
                for element in &layer.elements {
                    let text = element.searchable_text();
                    if text.to_lowercase().contains(&needle) {
                        hits.push(SearchHit {
                            page_index,
                            page_title: page.title.clone(),
                            layer_id: layer.id,
                            element_id: element.id(),
                            snippet: text.chars().take(80).collect(),
                        });
                    }
                }
                if let Some(spreadsheet) = &layer.spreadsheet {
                    let text = spreadsheet.searchable_text();
                    if text.to_lowercase().contains(&needle) {
                        hits.push(SearchHit {
                            page_index,
                            page_title: page.title.clone(),
                            layer_id: layer.id,
                            element_id: layer.id,
                            snippet: text.chars().take(80).collect(),
                        });
                    }
                }
            }
        }
        hits
    }

    pub fn asset(&self, id: Uuid) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.id == id)
    }

    pub fn export_svg(&self, path: &Path, page_index: usize) -> Result<(), DocumentError> {
        self.validate()?;
        let page = self.pages.get(page_index).ok_or_else(|| {
            DocumentError::Invalid(format!("page index {page_index} is out of range"))
        })?;
        fs::write(path, self.page_svg(page))?;
        Ok(())
    }

    fn page_svg(&self, page: &NotebookPage) -> String {
        let bounds = page
            .content_bounds()
            .unwrap_or(Rect {
                x: -640.0,
                y: -360.0,
                width: 1280.0,
                height: 720.0,
            })
            .expand(32.0);
        let mut svg = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" \
             data-inkstone-format=\"{}\" data-inkstone-version=\"{}\" \
             data-page-id=\"{}\">\n\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            NOTEBOOK_FORMAT,
            NOTEBOOK_VERSION,
            page.id,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            page.canvas.background.svg()
        );
        for layer in page.visible_layers() {
            if let Some(spreadsheet) = &layer.spreadsheet {
                svg.push_str(&spreadsheet.to_svg());
            }
        }
        for element in page.visible_elements() {
            match element {
                Element::Media(media) if media.kind == MediaKind::Image => {
                    if let Some(asset) = self.asset(media.asset_id) {
                        svg.push_str(&format!(
                            "<image data-inkstone-id=\"{}\" data-asset-id=\"{}\" \
                             x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                             preserveAspectRatio=\"xMidYMid meet\" href=\"data:{};base64,{}\"/>\n",
                            media.id,
                            media.asset_id,
                            media.bounds.x,
                            media.bounds.y,
                            media.bounds.width,
                            media.bounds.height,
                            crate::document::escape_xml(&asset.media_type),
                            asset.data_base64
                        ));
                    }
                }
                _ => svg.push_str(&crate::document::element_svg(element)),
            }
        }
        svg.push_str("</svg>\n");
        svg
    }
}
