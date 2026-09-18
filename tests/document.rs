use inkstone::document::{
    Anchor, Attachment, Color, Connector, Document, Element, Endpoint, Point, Rect, Shape,
    ShapeKind, Stroke, StrokeKind, StrokePoint, StrokeStyle, TextNote,
};
use tempfile::tempdir;
use uuid::Uuid;

fn style() -> StrokeStyle {
    StrokeStyle {
        color: Color::BLUE,
        width: 2.5,
    }
}

fn sample_document() -> Document {
    let shape_id = Uuid::new_v4();
    let text_id = Uuid::new_v4();
    let stroke_id = Uuid::new_v4();
    let connector_id = Uuid::new_v4();
    let mut document = Document {
        title: "Motor control sketch".to_owned(),
        ..Document::default()
    };
    document.elements = vec![
        Element::Shape(Shape {
            id: shape_id,
            kind: ShapeKind::Motor,
            bounds: Rect {
                x: 20.0,
                y: 40.0,
                width: 100.0,
                height: 100.0,
            },
            rotation_degrees: 0.0,
            style: style(),
            fill: None,
            label: "M1".to_owned(),
        }),
        Element::Text(TextNote {
            id: text_id,
            origin: Point::new(20.0, 24.0),
            text: "Drive motor".to_owned(),
            font_size: 18.0,
            color: Color::INK,
            max_width: Some(240.0),
        }),
        Element::Stroke(Stroke {
            id: stroke_id,
            kind: StrokeKind::Pen,
            style: style(),
            points: vec![
                StrokePoint::new(Point::new(-20.0, 20.0), 0.35),
                StrokePoint::new(Point::new(-10.0, 30.0), 0.8),
            ],
        }),
        Element::Connector(Connector {
            id: connector_id,
            start: Endpoint {
                point: Point::new(20.0, 90.0),
                attachment: Some(Attachment {
                    element_id: shape_id,
                    anchor: Anchor::West,
                }),
            },
            end: Endpoint {
                point: Point::new(-20.0, 90.0),
                attachment: Some(Attachment {
                    element_id: stroke_id,
                    anchor: Anchor::End,
                }),
            },
            route: vec![Point::new(0.0, 90.0)],
            style: style(),
            label: "shaft".to_owned(),
        }),
    ];
    document
}

#[test]
fn structured_document_round_trips_without_losing_relationships() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("motor.inkstone");
    let document = sample_document();

    document.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();

    assert_eq!(loaded, document);
    let Element::Connector(connector) = &loaded.elements[3] else {
        panic!("connector was not preserved");
    };
    assert!(connector.start.attachment.is_some());
    assert!(connector.end.attachment.is_some());
}

#[test]
fn dangling_connector_relationship_is_rejected() {
    let mut document = sample_document();
    let Element::Connector(connector) = &mut document.elements[3] else {
        unreachable!();
    };
    connector.start.attachment.as_mut().unwrap().element_id = Uuid::new_v4();

    let error = document.validate().unwrap_err().to_string();

    assert!(error.contains("references missing element"));
}

#[test]
fn snapping_records_element_and_semantic_anchor() {
    let document = sample_document();
    let expected_id = document.elements[0].id();

    let endpoint = document.snap_endpoint(Point::new(18.0, 90.0), 8.0);

    assert_eq!(endpoint.point, Point::new(20.0, 90.0));
    let attachment = endpoint.attachment.unwrap();
    assert_eq!(attachment.element_id, expected_id);
    assert_eq!(attachment.anchor, Anchor::West);
}

#[test]
fn svg_export_contains_open_ids_text_and_connector_links() {
    let document = sample_document();
    let shape_id = document.elements[0].id().to_string();
    let svg = document.to_svg();

    assert!(svg.contains("data-inkstone-format=\"inkstone.document\""));
    assert!(svg.contains("Drive motor"));
    assert!(svg.contains(&format!("data-start-element=\"{shape_id}\"")));
    assert!(svg.contains("data-kind=\"connector\""));
}

#[test]
fn bounds_and_view_intersection_work_with_negative_canvas_coordinates() {
    let element = Element::Stroke(Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: style(),
        points: vec![
            StrokePoint::new(Point::new(-120.0, -60.0), 1.0),
            StrokePoint::new(Point::new(-80.0, -20.0), 1.0),
        ],
    });

    assert!(element.bounds().intersects(Rect {
        x: -100.0,
        y: -100.0,
        width: 100.0,
        height: 100.0,
    }));
    assert!(!element.bounds().intersects(Rect {
        x: 100.0,
        y: 100.0,
        width: 50.0,
        height: 50.0,
    }));
}

#[test]
fn stroke_eraser_splits_ink_instead_of_deleting_the_whole_stroke() {
    let stroke = Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: style(),
        points: vec![
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
            StrokePoint::new(Point::new(40.0, 0.0), 1.0),
            StrokePoint::new(Point::new(80.0, 0.0), 1.0),
        ],
    };
    let fragments = stroke.erase_disk(Point::new(40.0, 0.0), 8.0);
    assert_eq!(fragments.len(), 2);
    assert!(fragments[0].points.iter().all(|point| point.x < 36.0));
    assert!(fragments[1].points.iter().all(|point| point.x > 44.0));
}

#[test]
fn shapes_and_media_scale_from_a_handle_pivot() {
    let mut shape = Element::Shape(Shape {
        id: Uuid::new_v4(),
        kind: ShapeKind::Rectangle,
        bounds: Rect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
        },
        rotation_degrees: 0.0,
        style: style(),
        fill: None,
        label: String::new(),
    });
    shape.scale_about(Point::new(10.0, 10.0), 2.0, 2.0);
    let Element::Shape(shape) = shape else {
        unreachable!();
    };
    assert!((shape.bounds.width - 80.0).abs() < 0.01);
    assert!((shape.bounds.height - 40.0).abs() < 0.01);
}
