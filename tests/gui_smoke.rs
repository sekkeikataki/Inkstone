use base64::Engine;
use inkstone::canvas::Canvas;
use inkstone::notebook::Notebook;
use tempfile::tempdir;

#[test]
#[ignore = "requires a graphical display; run under xvfb-run"]
fn native_media_notebook_and_pdf_flow() {
    gtk4::init().expect("GTK must initialize");
    let directory = tempdir().unwrap();
    let image_path = directory.path().join("pixel.png");
    let image = base64::engine::general_purpose::STANDARD
        .decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
        )
        .unwrap();
    std::fs::write(&image_path, image).unwrap();

    let canvas = Canvas::new();
    canvas.import_media(&image_path).unwrap();
    canvas.add_page();
    assert_eq!(canvas.element_count(), 0);

    let notebook_path = directory.path().join("media.inkstone");
    let pdf_path = directory.path().join("media.pdf");
    canvas.save(&notebook_path).unwrap();
    canvas.export_pdf(&pdf_path).unwrap();

    let notebook = Notebook::load(&notebook_path).unwrap();
    assert_eq!(notebook.pages.len(), 2);
    assert_eq!(notebook.assets.len(), 1);
    assert!(std::fs::metadata(pdf_path).unwrap().len() > 100);

    let canvas = Canvas::new();
    canvas.add_spreadsheet_layer();
    assert_eq!(
        canvas.active_layer_kind(),
        inkstone::spreadsheet::LayerKind::Excel
    );
    canvas.set_sheet_formula("10".into());
    canvas.commit_sheet_formula();
    assert!(canvas.goto_sheet_address("B1"));
    canvas.set_sheet_formula("=A1*2".into());
    canvas.commit_sheet_formula();
    assert_eq!(canvas.sheet_value(), "20");
    canvas.add_sheet_cols();
    canvas.add_sheet_rows();
    canvas.add_workbook_sheet();

    let sheet_path = directory.path().join("budget.inkstone");
    let svg_path = directory.path().join("budget.svg");
    let sheet_pdf = directory.path().join("budget.pdf");
    canvas.save(&sheet_path).unwrap();
    canvas.export_svg(&svg_path).unwrap();
    canvas.export_pdf(&sheet_pdf).unwrap();
    let loaded = Notebook::load(&sheet_path).unwrap();
    assert_eq!(
        loaded.pages[0].layers[1].kind,
        inkstone::spreadsheet::LayerKind::Excel
    );
    let book = loaded.pages[0].layers[1].spreadsheet.as_ref().unwrap();
    assert_eq!(book.sheets[0].display_cols(), 15);
    assert_eq!(book.sheets[0].display_rows(), 15);
    assert_eq!(book.sheets.len(), 2);
    let svg = std::fs::read_to_string(svg_path).unwrap();
    assert!(svg.contains("data-kind=\"spreadsheet\""));
    assert!(svg.contains("20"));
    assert!(std::fs::metadata(sheet_pdf).unwrap().len() > 100);
}

#[test]
#[ignore = "requires a graphical display; run under xvfb-run"]
fn pdf_pages_import_as_annotatable_canvas_backgrounds() {
    gtk4::init().expect("GTK must initialize");
    let directory = tempdir().unwrap();
    let pdf_path = directory.path().join("page.pdf");
    std::fs::write(
        &pdf_path,
        b"%PDF-1.1\n1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj\n2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj\n3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \ntrailer<< /Root 1 0 R /Size 4 >>\nstartxref\n190\n%%EOF\n",
    )
    .unwrap();

    let canvas = Canvas::new();
    canvas.import_media(&pdf_path).unwrap();
    let notebook_path = directory.path().join("annotated.inkstone");
    canvas.save(&notebook_path).unwrap();
    let loaded = Notebook::load(&notebook_path).unwrap();
    let background = &loaded.pages[0].layers[0];
    assert!(background.locked);
    assert_eq!(background.elements.len(), 1);
    match &background.elements[0] {
        inkstone::document::Element::Media(media) => {
            assert_eq!(media.kind, inkstone::document::MediaKind::Image);
        }
        other => panic!("expected rasterized PDF image, got {other:?}"),
    }
    assert!(loaded.pages[0].layers.iter().any(|layer| !layer.locked));
}
