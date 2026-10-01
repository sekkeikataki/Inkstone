use inkstone::library::{
    GUIDE_NAME, Library, Session, file_stem_title, relative_label, sanitize_stem,
    unique_inkstone_path,
};
use tempfile::tempdir;

#[test]
fn empty_library_seeds_personal_and_work_notebooks() {
    let root = tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();

    assert!(root.path().join(GUIDE_NAME).exists());
    assert!(root.path().join("Personal").is_dir());
    assert!(root.path().join("Work").is_dir());
    let notebooks = library.notebooks_flat();
    assert_eq!(notebooks.len(), 1);
    assert_eq!(notebooks[0].title, "My Notebook");
    assert_eq!(
        notebooks[0].path,
        root.path().join("Personal/My Notebook.inkstone")
    );
}

#[test]
fn nested_folders_are_categories_and_inkstone_files_are_notebooks() {
    let root = tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let projects = library
        .create_category(&root.path().join("Work"), "Projects")
        .unwrap();
    library.create_notebook(&projects, "Motor control").unwrap();
    std::fs::write(root.path().join("scratch.txt"), "ignore").unwrap();
    std::fs::write(root.path().join("Personal/hidden.inkstone.tmp"), "{}").unwrap();

    let mut library = library;
    library.rescan().unwrap();

    let labels: Vec<_> = library
        .category_paths()
        .into_iter()
        .map(|(label, _)| label)
        .collect();
    assert!(labels.contains(&"Work / Projects".to_owned()));
    let titles: Vec<_> = library
        .notebooks_flat()
        .into_iter()
        .map(|notebook| notebook.title.clone())
        .collect();
    assert!(titles.contains(&"My Notebook".to_owned()));
    assert!(titles.contains(&"Motor control".to_owned()));
    assert_eq!(
        library.category_label_for(&projects.join("Motor control.inkstone")),
        "Work / Projects"
    );
}

#[test]
fn file_names_stay_readable_and_unique() {
    assert_eq!(sanitize_stem("Work / Q3: notes*"), "Work Q3 notes");
    assert_eq!(sanitize_stem("..."), "Notebook");
    assert_eq!(
        file_stem_title(std::path::Path::new("Motor control.inkstone")),
        "Motor control"
    );
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("Journal.inkstone"), "{}").unwrap();
    let first = unique_inkstone_path(dir.path(), "Journal", None);
    assert_eq!(first.file_name().unwrap(), "Journal 2.inkstone");
    let keep = dir.path().join("Journal.inkstone");
    let same = unique_inkstone_path(dir.path(), "Journal", Some(&keep));
    assert_eq!(same, keep);
    assert_eq!(
        relative_label(dir.path(), &dir.path().join("Work").join("Labs")),
        Some("Work / Labs".to_owned())
    );
}

#[test]
fn library_search_finds_notes_across_notebooks() {
    let root = tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    let work = root.path().join("Work");
    let path = library.create_notebook(&work, "Motors").unwrap();
    let mut notebook = inkstone::notebook::Notebook::load(&path).unwrap();
    notebook.pages[0].layers[0]
        .elements
        .push(inkstone::document::Element::Text(
            inkstone::document::TextNote::plain(
                inkstone::document::Point::new(8.0, 20.0),
                "winding resistance",
                16.0,
                inkstone::document::Color::INK,
            ),
        ));
    notebook.save(&path).unwrap();
    let mut library = library;
    library.rescan().unwrap();
    let hits = library.search("winding");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].notebook_title, "Motors");
    let moved = library
        .move_notebook(&path, &root.path().join("Personal"))
        .unwrap();
    assert!(moved.starts_with(root.path().join("Personal")));
    assert!(!path.exists());
}

#[test]
fn session_preferences_fill_in_when_old_files_omit_them() {
    let session: Session =
        serde_json::from_str(r#"{"last_notebook":"/tmp/notes.inkstone"}"#).unwrap();
    assert_eq!(
        session.last_notebook.as_deref(),
        Some(std::path::Path::new("/tmp/notes.inkstone"))
    );
    assert!(session.preferences.restore_last_notebook);
    assert!(session.preferences.ignore_touch);
    assert!(!session.preferences.stabilizer);
    assert_eq!(session.preferences.replay_speed, 1.0);
    assert_eq!(session.preferences.default_width_mm, 0.5);
    assert_eq!(session.preferences.table_cols, 4);
    assert!(session.preferences.tool_shortcuts);
    assert_eq!(
        session.preferences.theme,
        inkstone::library::ThemePref::System
    );
    assert_eq!(
        session.preferences.default_paper,
        inkstone::document::PaperSize::Infinite
    );
    let round_trip = serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(session, round_trip);
}
