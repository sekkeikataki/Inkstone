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
        dashed: false,
    }
}

fn sample_document() -> Document {
    let shape_id = Uuid::new_v4();
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
        Element::Text({
            let mut note = TextNote::plain(Point::new(20.0, 24.0), "Drive motor", 18.0, Color::INK);
            note.max_width = Some(240.0);
            note
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
fn missing_stroke_dash_and_pattern_deserialize_with_defaults() {
    let json = r#"{
        "format": "inkstone.document",
        "version": 1,
        "title": "legacy",
        "canvas": {
            "background": {"red": 0.98, "green": 0.98, "blue": 0.97, "alpha": 1.0},
            "grid_spacing": 24.0,
            "grid_visible": false
        },
        "elements": []
    }"#;
    let document: Document = serde_json::from_str(json).unwrap();
    assert_eq!(
        document.canvas.pattern(),
        inkstone::document::BackgroundPattern::None
    );
    assert_eq!(
        document.canvas.layout,
        inkstone::document::PageLayout::Infinite
    );
}

#[test]
fn elements_scale_and_rotate_around_a_center() {
    let mut stroke = Element::Stroke(Stroke {
        id: Uuid::new_v4(),
        kind: StrokeKind::Pen,
        style: style(),
        points: vec![
            StrokePoint::new(Point::new(0.0, 0.0), 1.0),
            StrokePoint::new(Point::new(10.0, 0.0), 1.0),
        ],
    });
    stroke.scale_from(Point::new(0.0, 0.0), 2.0, 1.0);
    let Element::Stroke(scaled) = &stroke else {
        panic!("expected stroke");
    };
    assert_eq!(scaled.points[1].x, 20.0);
    stroke.rotate_around(Point::new(0.0, 0.0), 90.0);
    let Element::Stroke(rotated) = stroke else {
        panic!("expected stroke");
    };
    assert!((rotated.points[1].x).abs() < 0.01);
    assert!((rotated.points[1].y - 20.0).abs() < 0.01);
}

#[test]
fn svg_import_reads_polylines_rects_and_text() {
    let svg = "<svg>\
        <polyline points=\"0,0 10,0 10,10\" stroke=\"rgb(17,34,51)\" stroke-width=\"3\"/>\
        <rect x=\"2\" y=\"4\" width=\"8\" height=\"6\" stroke=\"rgb(10,20,30)\" fill=\"none\"/>\
        <text x=\"5\" y=\"12\" font-size=\"16\" fill=\"rgb(0,0,0)\">hello</text>\
    </svg>";
    let elements = inkstone::document::import_svg_elements(svg).unwrap();
    assert_eq!(elements.len(), 3);
    assert!(matches!(elements[0], Element::Stroke(_)));
    assert!(matches!(elements[1], Element::Shape(_)));
    assert!(matches!(&elements[2], Element::Text(text) if text.text == "hello"));
}

#[test]
fn color_parse_understands_hex_and_css() {
    assert_eq!(Color::parse("#fff").unwrap(), Color::rgb(1.0, 1.0, 1.0));
    let parsed = Color::parse("rgba(31,97,224,0.5)").unwrap();
    assert!((parsed.red - 31.0 / 255.0).abs() < 0.001);
    assert!((parsed.alpha - 0.5).abs() < 0.001);
}
