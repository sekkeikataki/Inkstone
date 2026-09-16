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
}
