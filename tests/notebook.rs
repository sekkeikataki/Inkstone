use base64::Engine;
use inkstone::document::{
    Color, Document, Element, MediaElement, MediaKind, Point, Rect, TextNote,
};
use inkstone::notebook::{Asset, Layer, NOTEBOOK_FORMAT, NOTEBOOK_VERSION, Notebook, NotebookPage};
use tempfile::tempdir;
use uuid::Uuid;

fn text(value: &str, x: f32) -> Element {
    Element::Text(TextNote {
        id: Uuid::new_v4(),
        origin: Point::new(x, 20.0),
        text: value.to_owned(),
        font_size: 18.0,
        color: Color::INK,
        max_width: Some(320.0),
    })
}

#[test]
fn notebook_round_trip_preserves_pages_layers_and_search() {
    let mut notebook = Notebook::default();
    notebook.pages[0].layers[0]
        .elements
        .push(text("motor winding", 10.0));
    notebook.pages.push(NotebookPage {
        id: Uuid::new_v4(),
        title: "Calculations".to_owned(),
        canvas: Default::default(),
        layers: vec![Layer {
            id: Uuid::new_v4(),
            name: "Equations".to_owned(),
            visible: true,
            locked: true,
            elements: vec![text("torque curve", 40.0)],
        }],
    });
    let directory = tempdir().unwrap();
    let path = directory.path().join("project.inkstone");

    notebook.save(&path).unwrap();
    let loaded = Notebook::load(&path).unwrap();

    assert_eq!(loaded, notebook);
    let hits = loaded.search("TORQUE");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page_title, "Calculations");
    assert_eq!(hits[0].snippet, "torque curve");
}

#[test]
fn legacy_v1_document_migrates_to_a_v2_notebook() {
    let mut document = Document {
        title: "Legacy sketch".to_owned(),
        ..Document::default()
    };
    document.elements.push(text("old note", 0.0));
    let directory = tempdir().unwrap();
    let path = directory.path().join("legacy.inkstone");
    document.save(&path).unwrap();

    let notebook = Notebook::load(&path).unwrap();

    assert_eq!(notebook.format, NOTEBOOK_FORMAT);
    assert_eq!(notebook.version, NOTEBOOK_VERSION);
    assert_eq!(notebook.pages.len(), 1);
    assert_eq!(notebook.pages[0].layers[0].elements.len(), 1);
}

#[test]
fn embedded_image_is_validated_and_exported_as_data_uri() {
    let mut notebook = Notebook::default();
    let asset_id = Uuid::new_v4();
    let bytes = b"test image bytes";
    notebook.assets.push(Asset {
        id: asset_id,
        name: "diagram.png".to_owned(),
        media_type: "image/png".to_owned(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    notebook.pages[0].layers[0]
        .elements
        .push(Element::Media(MediaElement {
            id: Uuid::new_v4(),
            asset_id,
            kind: MediaKind::Image,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 100.0,
            },
            alt_text: "wiring diagram".to_owned(),
            caption: "Panel wiring".to_owned(),
        }));
    let directory = tempdir().unwrap();
    let path = directory.path().join("page.svg");

    notebook.export_svg(&path, 0).unwrap();
    let svg = std::fs::read_to_string(path).unwrap();

    assert!(svg.contains("data:image/png;base64,"));
    assert!(svg.contains(&asset_id.to_string()));
}

#[test]
fn missing_media_asset_is_rejected() {
    let mut notebook = Notebook::default();
    notebook.pages[0].layers[0]
        .elements
        .push(Element::Media(MediaElement {
            id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            kind: MediaKind::Pdf,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 180.0,
            },
            alt_text: "manual".to_owned(),
            caption: "service manual".to_owned(),
        }));

    assert!(
        notebook
            .validate()
            .unwrap_err()
            .to_string()
            .contains("references missing asset")
    );
}
