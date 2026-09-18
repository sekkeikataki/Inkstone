use inkstone::document::{
    CanvasSettings, Element, PT_PER_MM, PageLayout, PaperSize, Point, Rect, ShapeKind, Stroke,
    StrokeKind, StrokePoint, StrokeStyle, mm_to_pt, pt_to_mm,
};
use inkstone::local::{
    AlignMode, ISO_DATETIME, PageTemplate, align_bounds, constrain_to_square, dimension_label,
    evaluate_equation, parse_page_link_href, parse_page_links, replay_duration, replay_progress,
    replay_timeline, snap_to_iso_angle, snap_to_ruler, split_stroke, stabilize_point,
    stroke_path_length, stroke_prefix, stroke_to_shape,
};
use inkstone::notebook::Notebook;
use uuid::Uuid;

#[test]
fn calculate_completes_simple_equations() {
    assert_eq!(evaluate_equation("2+2=").as_deref(), Some("2+2= 4"));
    assert_eq!(evaluate_equation("(3+1)*2=").as_deref(), Some("(3+1)*2= 8"));
    assert_eq!(evaluate_equation("10/4=").as_deref(), Some("10/4= 2.5"));
    assert!(evaluate_equation("hello=").is_none());
    assert!(evaluate_equation("2+2").is_none());
}

#[test]
fn ruler_snaps_to_the_dominant_axis() {
    let origin = Point::new(10.0, 10.0);
    assert_eq!(
        snap_to_ruler(origin, Point::new(40.0, 12.0)),
        Point::new(40.0, 10.0)
    );
    assert_eq!(
        snap_to_ruler(origin, Point::new(11.0, 50.0)),
        Point::new(10.0, 50.0)
    );
}

#[test]
fn tidy_rectangle_ink_becomes_a_shape() {
    let stroke = Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: StrokeStyle::default(),
        points: vec![
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
            StrokePoint::new(Point::new(40.0, 0.0), 1.0),
            StrokePoint::new(Point::new(80.0, 0.0), 1.0),
            StrokePoint::new(Point::new(80.0, 40.0), 1.0),
            StrokePoint::new(Point::new(80.0, 80.0), 1.0),
            StrokePoint::new(Point::new(40.0, 80.0), 1.0),
            StrokePoint::new(Point::new(0.0, 80.0), 1.0),
            StrokePoint::new(Point::new(0.0, 40.0), 1.0),
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
        ],
    };
    let shape = stroke_to_shape(&stroke).expect("closed rectangle should convert");
    assert_eq!(shape.kind, inkstone::document::ShapeKind::Rectangle);
}

#[test]
fn millimetres_round_trip_through_postscript_points() {
    let pt = mm_to_pt(210.0);
    assert!((pt - 595.276).abs() < 0.01);
    assert!((pt_to_mm(pt) - 210.0).abs() < 0.001);
    assert!((5.0 * PT_PER_MM - mm_to_pt(5.0)).abs() < f32::EPSILON);
}

#[test]
fn default_notebook_uses_iso_216_a4_and_a_5mm_grid() {
    let canvas = CanvasSettings::default();
    assert_eq!(canvas.layout, PageLayout::Infinite);
    assert_eq!(canvas.paper_size(), PaperSize::Infinite);
    assert!((pt_to_mm(canvas.page_width) - 210.0).abs() < 0.02);
    assert!((pt_to_mm(canvas.page_height) - 297.0).abs() < 0.02);
    assert!((pt_to_mm(canvas.grid_spacing) - 5.0).abs() < 0.02);
    let mut sheet = canvas;
    sheet.set_paper_size(PaperSize::A4);
    assert_eq!(sheet.layout, PageLayout::Fixed);
    assert_eq!(sheet.paper_size(), PaperSize::A4);
}

#[test]
fn iso_angle_snaps_to_fifteen_degrees() {
    let origin = Point::new(0.0, 0.0);
    let snapped = snap_to_iso_angle(origin, Point::new(10.0, 1.0));
    assert!((snapped.y - 0.0).abs() < 0.2);
    let diagonal = snap_to_iso_angle(origin, Point::new(10.0, 10.0));
    assert!((diagonal.x - diagonal.y).abs() < 0.05);
}

#[test]
fn shift_keeps_rectangles_square() {
    let origin = Point::new(0.0, 0.0);
    assert_eq!(
        constrain_to_square(origin, Point::new(40.0, 10.0)),
        Point::new(40.0, 40.0)
    );
}

#[test]
fn dimension_label_uses_millimetres() {
    let start = Point::new(0.0, 0.0);
    let end = Point::new(mm_to_pt(40.0), 0.0);
    assert_eq!(dimension_label(start, end), "40.0 mm");
}

#[test]
fn iso_datetime_is_en_28601() {
    assert_eq!(ISO_DATETIME, "%Y-%m-%d %H:%M");
}

#[test]
fn line_drag_keeps_direction() {
    let bounds = Rect::from_drag(Point::new(80.0, 20.0), Point::new(10.0, 50.0));
    assert!(bounds.width < 0.0);
    assert_eq!(bounds.start(), Point::new(80.0, 20.0));
    assert_eq!(bounds.end(), Point::new(10.0, 50.0));
    assert!(ShapeKind::Line.uses_drag_bounds());
    assert!(!ShapeKind::Resistor.uses_drag_bounds());
}

#[test]
fn eraser_splits_a_stroke_and_drops_tiny_fragments() {
    let stroke = Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: StrokeStyle::default(),
        points: vec![
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
            StrokePoint::new(Point::new(10.0, 0.0), 1.0),
            StrokePoint::new(Point::new(20.0, 0.0), 1.0),
            StrokePoint::new(Point::new(30.0, 0.0), 1.0),
            StrokePoint::new(Point::new(40.0, 0.0), 1.0),
        ],
    };
    let pieces = split_stroke(&stroke, Point::new(20.0, 0.0), 2.0).expect("hit");
    assert_eq!(pieces.len(), 2);
    assert!(split_stroke(&stroke, Point::new(200.0, 200.0), 2.0).is_none());
}

#[test]
fn lazy_ink_lags_behind_the_pointer() {
    let last = Point::new(0.0, 0.0);
    let mixed = stabilize_point(last, Point::new(100.0, 0.0), 0.5);
    assert!(mixed.x > 0.0 && mixed.x < 100.0);
}

#[test]
fn align_and_page_links_are_local() {
    let bounds = [
        Rect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 10.0,
        },
        Rect {
            x: 40.0,
            y: 30.0,
            width: 10.0,
            height: 10.0,
        },
    ];
    let deltas = align_bounds(&bounds, AlignMode::Left);
    assert!((deltas[0].x).abs() < f32::EPSILON);
    assert!((deltas[1].x + 30.0).abs() < f32::EPSILON);
    assert_eq!(
        parse_page_links("See [[Motor]] and [[Lab log]]"),
        vec!["Motor".to_owned(), "Lab log".to_owned()]
    );
    let id = Uuid::new_v4();
    assert_eq!(
        parse_page_link_href(&format!("inkstone:page:{id}")),
        Some(id)
    );
}

#[test]
fn extra_templates_and_symbol_groups_exist() {
    assert!(PageTemplate::ALL.contains(&PageTemplate::Cornell));
    assert!(PageTemplate::ALL.contains(&PageTemplate::TitleBlock));
    assert_eq!(ShapeKind::Lamp.group(), "IEC 60617");
    assert_eq!(ShapeKind::AndGate.group(), "Logic");
    assert_eq!(ShapeKind::ThirdAngle.group(), "ISO 128/129");
    let mut notebook = Notebook::default();
    inkstone::local::apply_template(&mut notebook.pages[0], PageTemplate::Cornell);
    assert!(
        notebook.pages[0]
            .elements()
            .any(|element| matches!(element, Element::Text(text) if text.text.contains("Cornell")))
    );
    let mut todo_book = Notebook::default();
    inkstone::local::apply_template(&mut todo_book.pages[0], PageTemplate::ToDo);
    let todos = inkstone::local::collect_todos(&todo_book);
    assert!(todos.iter().any(|item| item.text == "First task"));
}

#[test]
fn ink_replay_grows_strokes_along_their_path() {
    let stroke = Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: StrokeStyle::default(),
        points: vec![
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
            StrokePoint::new(Point::new(100.0, 0.0), 0.5),
        ],
    };
    assert!((stroke_path_length(&stroke) - 100.0).abs() < f32::EPSILON);
    let mid = stroke_prefix(&stroke, 0.5);
    assert_eq!(mid.points.len(), 2);
    assert!((mid.points[1].x - 50.0).abs() < 0.01);
    assert!((mid.points[1].pressure - 0.75).abs() < 0.01);
    let start = stroke_prefix(&stroke, 0.0);
    assert_eq!(start.points.len(), 1);
    let text = Element::Text(inkstone::document::TextNote::plain(
        Point::new(0.0, 0.0),
        "Note",
        18.0,
        inkstone::document::Color::INK,
    ));
    let events = replay_timeline([&Element::Stroke(stroke.clone()), &text]);
    assert_eq!(events.len(), 2);
    assert!(replay_duration(&events) > events[0].duration);
    assert!(replay_progress(0.0, &events[0], true).is_some());
    assert!(replay_progress(events[0].start - 0.01, &events[1], false).is_none());
    assert_eq!(
        replay_progress(events[1].start, &events[1], false),
        Some(1.0)
    );
}
