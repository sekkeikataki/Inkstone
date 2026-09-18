use inkstone::document::Point;
use inkstone::notebook::{Layer, LayerType, Notebook};
use inkstone::spreadsheet::{CellAddress, SpreadsheetLayer};
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn excel_layer_round_trips_through_notebook_save_and_load() {
    let mut notebook = Notebook::default();
    let mut layer = Layer::excel("Budget");
    if let Some(spreadsheet) = layer.spreadsheet.as_mut() {
        spreadsheet.set_raw(CellAddress { row: 0, col: 0 }, "Revenue");
        spreadsheet.set_raw(CellAddress { row: 0, col: 1 }, "100");
        spreadsheet.set_raw(CellAddress { row: 1, col: 0 }, "Costs");
        spreadsheet.set_raw(CellAddress { row: 1, col: 1 }, "40");
        spreadsheet.set_raw(CellAddress { row: 2, col: 0 }, "Profit");
        spreadsheet.set_raw(CellAddress { row: 2, col: 1 }, "=B1-B2");
    }
    notebook.pages[0].layers.push(layer);

    let directory = tempdir().unwrap();
    let path = directory.path().join("budget.inkstone");
    notebook.save(&path).unwrap();
    let loaded = Notebook::load(&path).unwrap();

    assert_eq!(loaded.pages[0].layers.len(), 2);
    let excel = loaded.pages[0]
        .layers
        .iter()
        .find(|layer| layer.is_excel())
        .expect("excel layer");
    assert_eq!(excel.layer_type, LayerType::Excel);
    let spreadsheet = excel.spreadsheet.as_ref().unwrap();
    assert_eq!(
        spreadsheet.display_text(CellAddress { row: 2, col: 1 }),
        "60"
    );
}

#[test]
fn excel_layer_search_includes_cell_values() {
    let mut notebook = Notebook::default();
    let mut layer = Layer::excel("Inventory");
    if let Some(spreadsheet) = layer.spreadsheet.as_mut() {
        spreadsheet.set_raw(CellAddress { row: 0, col: 0 }, "widget count");
    }
    notebook.pages[0].layers.push(layer);

    let hits = notebook.search("widget");
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.contains("widget"));
}

#[test]
fn spreadsheet_exports_multiple_sheets_to_xlsx() {
    let mut spreadsheet = SpreadsheetLayer::default();
    spreadsheet.set_raw(CellAddress { row: 0, col: 0 }, "Item");
    spreadsheet.set_raw(CellAddress { row: 0, col: 1 }, "15");
    spreadsheet.add_sheet("Summary");
    spreadsheet
        .active_sheet_mut()
        .cells
        .insert("A1".to_owned(), Default::default());
    spreadsheet
        .active_sheet_mut()
        .cells
        .get_mut("A1")
        .unwrap()
        .value = "Total".to_owned();

    let directory = tempdir().unwrap();
    let export_path = directory.path().join("sheet.xlsx");
    spreadsheet.export_xlsx(&export_path).unwrap();

    let imported = SpreadsheetLayer::from_xlsx(&export_path).unwrap();
    assert_eq!(imported.sheets.len(), 2);
    assert_eq!(imported.sheets[1].name, "Summary");
    assert_eq!(
        imported.get_raw(CellAddress { row: 0, col: 0 }),
        Some("Item")
    );
}

#[test]
fn canvas_layer_rejects_embedded_spreadsheet_payload() {
    let mut notebook = Notebook::default();
    notebook.pages[0].layers.push(Layer {
        id: Uuid::new_v4(),
        name: "Broken".to_owned(),
        visible: true,
        locked: false,
        layer_type: LayerType::Canvas,
        elements: Vec::new(),
        spreadsheet: Some(SpreadsheetLayer::default()),
    });

    assert!(
        notebook
            .validate()
            .unwrap_err()
            .to_string()
            .contains("must not include spreadsheet data")
    );
}

#[test]
fn spreadsheet_hit_test_selects_cells() {
    let spreadsheet = SpreadsheetLayer::default();
    let origin = spreadsheet.origin;
    let inside = Point::new(origin.x + 50.0, origin.y + 40.0);
    assert_eq!(
        spreadsheet.hit_test(inside),
        Some(CellAddress { row: 0, col: 0 })
    );
}
