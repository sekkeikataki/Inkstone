                .as_ref()
                .map(|value| value.element_id.to_string())
                .unwrap_or_default();
            let end = connector
                .end
                .attachment
                .as_ref()
                .map(|value| value.element_id.to_string())
                .unwrap_or_default();
            format!(
                "<polyline data-inkstone-id=\"{}\" data-kind=\"connector\" \
                 data-start-element=\"{}\" data-end-element=\"{}\" points=\"{}\" \
                 fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{}/>\n",
                connector.id,
                start,
                end,
                points,
                connector.style.color.svg(),
                connector.style.width,
                dash_attr(connector.style.dashed)
            )
        }
        Element::Shape(shape) => shape_svg(shape),
        Element::Media(media) => format!(
            "<g data-inkstone-id=\"{}\" data-kind=\"{:?}\" data-asset-id=\"{}\">\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#eeeeee\" \
             stroke=\"#555555\" stroke-width=\"1\"/>\
             <text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"14\" \
             fill=\"#333333\">{}</text></g>\n",
            media.id,
            media.kind,
            media.asset_id,
            media.bounds.x,
            media.bounds.y,
            media.bounds.width,
            media.bounds.height,
            media.bounds.x + 12.0,
            media.bounds.y + 24.0,
            escape_xml(if media.caption.is_empty() {
                &media.alt_text
            } else {
                &media.caption
            })
        ),
        Element::Table(table) => {
            let mut svg = format!(
                "<g data-inkstone-id=\"{}\" data-kind=\"table\">\
                 <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" \
                 stroke=\"#555555\" stroke-width=\"1\"/>",
                table.id, table.bounds.x, table.bounds.y, table.bounds.width, table.bounds.height
            );
            let cell_w = table.bounds.width / table.columns.max(1) as f32;
            let cell_h = table.bounds.height / table.rows.max(1) as f32;
            for column in 1..table.columns {
                let x = table.bounds.x + cell_w * column as f32;
                svg.push_str(&format!(
                    "<line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\" stroke=\"#888888\"/>",
                    table.bounds.y,
                    table.bounds.y + table.bounds.height
                ));
            }
            for row in 1..table.rows {
                let y = table.bounds.y + cell_h * row as f32;
                svg.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"#888888\"/>",
                    table.bounds.x,
                    table.bounds.x + table.bounds.width
                ));
            }
            for (index, cell) in table.cells.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let column = (index as u32) % table.columns;
                let row = (index as u32) / table.columns;
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"12\" \
                     fill=\"#222222\">{}</text>",
                    table.bounds.x + cell_w * column as f32 + 6.0,
                    table.bounds.y + cell_h * row as f32 + cell_h * 0.65,
                    escape_xml(cell)
                ));
            }
            svg.push_str("</g>\n");
            svg
        }
        Element::Tag(tag) => {
            let (width, height) = tag.size();
            format!(
                "<g data-inkstone-id=\"{}\" data-kind=\"tag\">\
                 <rect x=\"{}\" y=\"{}\" width=\"{width}\" height=\"{height}\" rx=\"8\" \
                 fill=\"{}\" fill-opacity=\"0.16\" stroke=\"{}\"/>\
                 <text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"13\" \
                 fill=\"{}\">{} {}</text></g>\n",
                tag.id,
                tag.origin.x,
                tag.origin.y,
                tag.kind.color().svg(),
                tag.kind.color().svg(),
                tag.origin.x + 10.0,
                tag.origin.y + 19.0,
                tag.kind.color().svg(),
                escape_xml(tag.kind.label()),
                escape_xml(&tag.note)
            )
        }
    }
}

fn shape_svg(shape: &Shape) -> String {
    let b = if shape.kind.uses_drag_bounds() {
        shape.bounds
    } else {
        shape.bounds.normalized()
    };
    let stroke = shape.style.color.svg();
    let fill = shape
        .fill
        .map(Color::svg)
        .unwrap_or_else(|| "none".to_owned());
    let common = format!(
        "data-inkstone-id=\"{}\" data-kind=\"{:?}\" fill=\"{}\" stroke=\"{}\" \
         stroke-width=\"{}\"{}",
        shape.id,
        shape.kind,
        fill,
        stroke,
        shape.style.width,
        dash_attr(shape.style.dashed)
    );
    let cy = b.center().y;
    let geometry = match shape.kind {
        ShapeKind::Rectangle | ShapeKind::Beam => format!(
            "<rect {common} x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            b.x, b.y, b.width, b.height
        ),
        ShapeKind::Ellipse | ShapeKind::Motor | ShapeKind::Gear | ShapeKind::Bearing => format!(
            "<ellipse {common} cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>",
            b.center().x,
            b.center().y,
            b.width.abs() / 2.0,
            b.height.abs() / 2.0
        ),
        ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Dimension => {
            let start = b.start();
            let end = b.end();
            let mut extra = String::new();
            if shape.kind == ShapeKind::Arrow || shape.kind == ShapeKind::Dimension {
                extra.push_str(&arrow_head_svg(start, end, b));
            }
            format!(
                "<path {common} d=\"M {} {} L {} {}{extra}\" fill=\"none\"/>",
                start.x, start.y, end.x, end.y
            )
        }
        ShapeKind::Triangle => format!(
            "<polygon {common} points=\"{} {},{} {},{} {}\"/>",
            b.center().x,
            b.y,
            b.x + b.width,
            b.y + b.height,
            b.x,
            b.y + b.height
        ),
        ShapeKind::Capacitor => {
            let x1 = b.x + b.width * 0.42;
            let x2 = b.x + b.width * 0.58;
            format!(
                "<g {common}><path d=\"M {} {} H {} M {} {} H {} \
                 M {} {} V {} M {} {} V {}\" fill=\"none\"/></g>",
                b.x,
                cy,
                x1,
                x2,
                cy,
                b.x + b.width,
                x1,
                b.y,
                b.y + b.height,
                x2,
                b.y,
                b.y + b.height
            )
        }
        ShapeKind::Ground => format!(
            "<g {common}><path d=\"M {} {} V {} M {} {} H {} M {} {} H {} M {} {} H {}\" \
             fill=\"none\"/></g>",
            b.center().x,
            b.y,
            b.y + b.height * 0.45,
            b.x,
            b.y + b.height * 0.45,
            b.x + b.width,
            b.x + b.width * 0.18,
            b.y + b.height * 0.68,
            b.x + b.width * 0.82,
            b.x + b.width * 0.36,
            b.y + b.height * 0.9,
            b.x + b.width * 0.64
        ),
        ShapeKind::Resistor => {
            let x1 = b.x + b.width * 0.22;
            let x2 = b.x + b.width * 0.78;
            let y1 = b.y + b.height * 0.28;
            let y2 = b.y + b.height * 0.72;
            format!(
                "<g {common}><path d=\"M {0} {1} H {2} M {3} {1} H {4}\" fill=\"none\"/>\
                 <rect x=\"{2}\" y=\"{5}\" width=\"{6}\" height=\"{7}\" fill=\"none\"/></g>",
                b.x,
                cy,
                x1,
                x2,
                b.x + b.width,
                y1,
                (x2 - x1).abs(),
                (y2 - y1).abs()
            )
        }
        ShapeKind::Spring => zigzag_svg(&common, b),
        ShapeKind::Diode => {
            let mid = b.x + b.width * 0.62;
            format!(
                "<g {common}><path d=\"M {0} {1} H {2} M {3} {1} H {4} M {2} {5} L {3} {1} L {2} {6} Z \
                 M {3} {5} V {6}\" fill=\"none\"/></g>",
                b.x,
                cy,
                b.x + b.width * 0.28,
                mid,
                b.x + b.width,
                b.y + b.height * 0.18,
                b.y + b.height * 0.82
            )
        }
        ShapeKind::Inductor => inductor_svg(&common, b),
        ShapeKind::Switch => format!(
            "<g {common}><path d=\"M {0} {1} H {2} M {3} {1} H {4} M {2} {1} L {5} {6}\" fill=\"none\"/>\
             <circle cx=\"{2}\" cy=\"{1}\" r=\"2.4\" fill=\"none\"/>\
             <circle cx=\"{3}\" cy=\"{1}\" r=\"2.4\" fill=\"none\"/></g>",
            b.x,
            cy,
            b.x + b.width * 0.28,
            b.x + b.width * 0.72,
            b.x + b.width,
            b.x + b.width * 0.62,
            b.y + b.height * 0.18
        ),
        ShapeKind::Fuse => {
            let x1 = b.x + b.width * 0.28;
            let x2 = b.x + b.width * 0.72;
            format!(
                "<g {common}><path d=\"M {0} {1} H {2} M {3} {1} H {4} M {2} {1} H {3}\" fill=\"none\"/>\
                 <rect x=\"{2}\" y=\"{5}\" width=\"{6}\" height=\"{7}\" fill=\"none\"/></g>",
                b.x,
                cy,
                x1,
                x2,
                b.x + b.width,
                b.y + b.height * 0.32,
                (x2 - x1).abs(),
                b.height * 0.36
            )
        }
        ShapeKind::Battery => {
            let x1 = b.x + b.width * 0.42;
            let x2 = b.x + b.width * 0.58;
            format!(
                "<g {common}><path d=\"M {0} {1} H {2} M {3} {1} H {4} \
                 M {2} {5} V {6} M {3} {7} V {8}\" fill=\"none\"/></g>",
                b.x,
                cy,
                x1,
                x2,
                b.x + b.width,
                b.y + b.height * 0.12,
                b.y + b.height * 0.88,
                b.y + b.height * 0.28,
                b.y + b.height * 0.72
            )
        }
        ShapeKind::Lamp => {
            let cx = b.center().x;
            let cy = b.center().y;
            let r = b.width.abs().min(b.height.abs()) / 2.0;
            format!(
                "<g {common}><circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"none\"/>\
                 <path d=\"M {0} {1} L {2} {3} M {0} {3} L {2} {1}\" fill=\"none\"/></g>",
                cx - r * 0.62,
                cy - r * 0.62,
                cx + r * 0.62,
                cy + r * 0.62
            )
        }
        ShapeKind::Transformer => {
            let left = inductor_svg(
                &common,
                Rect {
                    x: b.x,
                    y: b.y,
                    width: b.width * 0.42,
                    height: b.height,
                },
            );
            let right = inductor_svg(
                &common,
                Rect {
                    x: b.x + b.width * 0.58,
                    y: b.y,
                    width: b.width * 0.42,
                    height: b.height,
                },
            );
            format!(
                "<g {common}>{left}{right}<path d=\"M {0} {1} V {2} M {3} {1} V {2}\" fill=\"none\"/></g>",
                b.x + b.width * 0.46,
                b.y + b.height * 0.18,
                b.y + b.height * 0.82,
                b.x + b.width * 0.54
            )
        }
        ShapeKind::AndGate | ShapeKind::NandGate => {
            logic_and_svg(&common, b, shape.kind == ShapeKind::NandGate)
        }
        ShapeKind::OrGate | ShapeKind::NorGate | ShapeKind::XorGate => {
            logic_or_svg(&common, b, shape.kind)
        }
        ShapeKind::NotGate => {
            let tip = b.x + b.width * 0.72;
            format!(
                "<g {common}><path d=\"M {0} {1} L {2} {3} L {0} {4} Z\" fill=\"none\"/>\
                 <circle cx=\"{5}\" cy=\"{3}\" r=\"{6}\" fill=\"none\"/>\
                 <path d=\"M {5} {3} H {7}\" fill=\"none\"/></g>",
                b.x,
                b.y,
                tip,
                cy,
                b.y + b.height,
                b.x + b.width * 0.82,
                b.width.abs() * 0.08,
                b.x + b.width
            )
        }
        ShapeKind::SurfaceFinish => format!(
            "<g {common}><path d=\"M {0} {1} L {2} {3} L {4} {5}\" fill=\"none\"/></g>",
            b.x,
            b.y + b.height * 0.62,
            b.x + b.width * 0.32,
            b.y + b.height,
            b.x + b.width,
            b.y
        ),
        ShapeKind::ThirdAngle => {
            let cx = b.center().x;
            let cy = b.center().y;
            let r = b.width.abs().min(b.height.abs()) * 0.38;
            format!(
                "<g {common}><circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"none\"/>\
                 <circle cx=\"{cx}\" cy=\"{cy}\" r=\"{0}\" fill=\"none\"/>\
                 <path d=\"M {1} {2} L {3} {4} L {5} {4} L {6} {2} Z\" fill=\"none\"/></g>",
                r * 0.42,
                b.x + b.width * 0.18,
                b.y,
                b.x + b.width * 0.32,
                b.y + b.height * 0.18,
                b.x + b.width * 0.68,
                b.x + b.width * 0.82
            )
        }
    };
    let label = if shape.label.is_empty() {
        String::new()
    } else {
        format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"14\" \
             text-anchor=\"middle\" fill=\"{}\">{}</text>",
            b.center().x,
            b.y.min(b.y + b.height) + b.height.abs() + 18.0,
            stroke,
            escape_xml(&shape.label)
        )
    };
    format!("{geometry}{label}\n")
}

fn arrow_head_svg(start: Point, end: Point, bounds: Rect) -> String {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let size = (bounds.width.abs().hypot(bounds.height.abs()) * 0.18).clamp(8.0, 22.0);
    format!(
        " M {} {} L {} {} M {} {} L {} {}",
        end.x,
        end.y,
        end.x - size * (angle - 0.45).cos(),
        end.y - size * (angle - 0.45).sin(),
        end.x,
        end.y,
        end.x - size * (angle + 0.45).cos(),
        end.y - size * (angle + 0.45).sin()
    )
}

fn zigzag_svg(common: &str, b: Rect) -> String {
    let mut path = format!("M {} {}", b.x, b.center().y);
    for index in 0..=8 {
        let x = b.x + b.width * (index as f32 + 1.0) / 10.0;
        let y = if index % 2 == 0 {
            b.y + b.height * 0.2
        } else {
            b.y + b.height * 0.8
        };
        path.push_str(&format!(" L {x} {y}"));
    }
    path.push_str(&format!(" L {} {}", b.x + b.width, b.center().y));
    format!("<path {common} d=\"{path}\" fill=\"none\"/>")
}

fn inductor_svg(common: &str, b: Rect) -> String {
    let cy = b.center().y;
    let radius = b.width.abs() / 10.0;
    let mut path = format!("M {} {}", b.x, cy);
    path.push_str(&format!(" H {}", b.x + b.width * 0.18));
    for index in 0..4 {
        let cx = b.x + b.width * (0.26 + index as f32 * 0.14);
        path.push_str(&format!(
            " A {radius} {radius} 0 0 1 {} {}",
            cx + radius,
            cy
        ));
    }
    path.push_str(&format!(" H {}", b.x + b.width));
    format!("<path {common} d=\"{path}\" fill=\"none\"/>")
}

fn logic_and_svg(common: &str, b: Rect, nand: bool) -> String {
    let mid = b.x + b.width * 0.55;
    let bubble = b.x + b.width * 0.82;
    let cy = b.center().y;
    let bubble_r = b.width.abs() * 0.08;
    let extra = if nand {
        format!(
            "<circle cx=\"{bubble}\" cy=\"{cy}\" r=\"{bubble_r}\" fill=\"none\"/>\
             <path d=\"M {0} {cy} H {1}\" fill=\"none\"/>",
            bubble + bubble_r,
            b.x + b.width
        )
    } else {
        format!(
            "<path d=\"M {mid} {cy} H {}\" fill=\"none\"/>",
            b.x + b.width
        )
    };
    format!(
        "<g {common}><path d=\"M {0} {1} H {mid} A {2} {3} 0 0 1 {mid} {4} H {0} Z\" fill=\"none\"/>\
         {extra}</g>",
        b.x,
        b.y,
        b.width * 0.35,
        b.height / 2.0,
        b.y + b.height
    )
}

fn logic_or_svg(common: &str, b: Rect, kind: ShapeKind) -> String {
    let cy = b.center().y;
    let tip = b.x + b.width * 0.78;
    let xor = if kind == ShapeKind::XorGate {
        format!(
            "<path d=\"M {0} {1} Q {2} {cy} {0} {3}\" fill=\"none\"/>",
            b.x + b.width * 0.08,
            b.y,
            b.x + b.width * 0.22,
            b.y + b.height
        )
    } else {
        String::new()
    };
    let bubble = if matches!(kind, ShapeKind::NorGate) {
        let cx = b.x + b.width * 0.86;
        let r = b.width.abs() * 0.07;
        format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"none\"/>")
    } else {
        format!(
            "<path d=\"M {tip} {cy} H {}\" fill=\"none\"/>",
            b.x + b.width
        )
    };
    format!(
        "<g {common}><path d=\"M {0} {1} Q {2} {cy} {0} {3} Q {4} {5} {tip} {cy} Q {4} {6} {0} {1}\" fill=\"none\"/>\
         {xor}{bubble}</g>",
        b.x,
        b.y,
        b.x + b.width * 0.18,
        b.y + b.height,
        b.x + b.width * 0.42,
        b.y + b.height * 0.78,
        b.y + b.height * 0.22
    )
}

pub(crate) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn dash_attr(dashed: bool) -> &'static str {
    if dashed {
        " stroke-dasharray=\"8 6\""
    } else {
        ""
    }
}

pub fn import_svg_elements(svg: &str) -> Result<Vec<Element>, DocumentError> {
    let mut elements = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if rest.starts_with("<!--") {
            rest = rest.split_once("-->").map(|(_, tail)| tail).unwrap_or("");
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.split_once('>').map(|(_, tail)| tail).unwrap_or("");
            continue;
        }
        if rest.starts_with("</") {
            rest = rest.split_once('>').map(|(_, tail)| tail).unwrap_or("");
            continue;
        }
        let name_end = rest[1..]
            .find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
            .unwrap_or(0)
            + 1;
        let name = rest[1..name_end].trim().to_ascii_lowercase();
        let (tag, after) = if name == "text" {
            let close = rest.find("</text>").unwrap_or(rest.len());
            let end = (close + "</text>".len()).min(rest.len());
            (&rest[..end], &rest[end..])
        } else {
            let close = rest.find('>').unwrap_or(rest.len().saturating_sub(1));
            let end = (close + 1).min(rest.len());
            (&rest[..end], &rest[end..])
        };
        rest = after;
        match name.as_str() {
            "polyline" | "polygon" => {
                if let Some(element) = svg_polyline(tag, name == "polygon") {
                    elements.push(element);
                }
            }
            "line" => {
                if let Some(element) = svg_line(tag) {
                    elements.push(element);
                }
            }
            "rect" => {
                if let Some(element) = svg_rect(tag) {
                    elements.push(element);
                }
            }
            "ellipse" | "circle" => {
                if let Some(element) = svg_ellipse(tag, name == "circle") {
                    elements.push(element);
                }
            }
            "text" => {
                if let Some(element) = svg_text(tag) {
                    elements.push(element);
                }
            }
            "path" => {
                if let Some(element) = svg_path(tag) {
                    elements.push(element);
                }
            }
            _ => {}
        }
    }
    if elements.is_empty() {
        return Err(DocumentError::Invalid(
            "the SVG file did not contain drawable shapes, strokes, or text".to_owned(),
        ));
    }
    Ok(elements)
}

fn svg_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(index) = tag.find(&needle) {
            let start = index + needle.len();
            let end = tag[start..].find(quote)?;
            return Some(&tag[start..start + end]);
        }
    }
    None
}

fn svg_f32(tag: &str, name: &str) -> Option<f32> {
    svg_attr(tag, name)?.trim().parse().ok()
}

fn svg_style(tag: &str) -> StrokeStyle {
    let color = svg_attr(tag, "stroke")
        .and_then(Color::parse)
        .or_else(|| svg_attr(tag, "fill").and_then(Color::parse))
        .unwrap_or(Color::INK);
    let width = svg_f32(tag, "stroke-width")
        .unwrap_or(2.0)
        .max(MIN_STROKE_WIDTH);
    StrokeStyle {
        color,
        width: width.min(MAX_STROKE_WIDTH),
        dashed: svg_attr(tag, "stroke-dasharray")
            .is_some_and(|value| !value.trim().is_empty() && value.trim() != "none"),
    }
}

fn svg_id(tag: &str) -> Uuid {
    svg_attr(tag, "data-inkstone-id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn parse_svg_points(value: &str) -> Vec<Point> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            current.push(ch);
        } else if !current.is_empty() {
            if let Ok(number) = current.parse::<f32>() {
                numbers.push(number);
            }
            current.clear();
        }
    }
    if !current.is_empty()
        && let Ok(number) = current.parse::<f32>()
    {
        numbers.push(number);
    }
    numbers
        .chunks(2)
        .filter_map(|pair| (pair.len() == 2).then_some(Point::new(pair[0], pair[1])))
        .collect()
}

fn svg_polyline(tag: &str, closed: bool) -> Option<Element> {
    let mut points = parse_svg_points(svg_attr(tag, "points")?);
    if points.len() < 2 {
        return None;
    }
    if closed && points.first() != points.last() {
        points.push(points[0]);
    }
    Some(Element::Stroke(Stroke {
        id: svg_id(tag),
        kind: StrokeKind::Pen,
        style: svg_style(tag),
        points: points
            .into_iter()
            .map(|point| StrokePoint::new(point, 1.0))
            .collect(),
    }))
}

fn svg_line(tag: &str) -> Option<Element> {
    let start = Point::new(svg_f32(tag, "x1")?, svg_f32(tag, "y1")?);
    let end = Point::new(svg_f32(tag, "x2")?, svg_f32(tag, "y2")?);
    Some(Element::Shape(Shape {
        id: svg_id(tag),
        kind: ShapeKind::Line,
        bounds: Rect::from_points(start, end),
        rotation_degrees: 0.0,
        style: svg_style(tag),
        fill: None,
        label: String::new(),
    }))
}

fn svg_rect(tag: &str) -> Option<Element> {
    let bounds = Rect {
        x: svg_f32(tag, "x").unwrap_or(0.0),
        y: svg_f32(tag, "y").unwrap_or(0.0),
        width: svg_f32(tag, "width")?,
        height: svg_f32(tag, "height")?,
    };
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return None;
    }
    Some(Element::Shape(Shape {
        id: svg_id(tag),
        kind: ShapeKind::Rectangle,
        bounds,
        rotation_degrees: 0.0,
        style: svg_style(tag),
        fill: svg_attr(tag, "fill")
            .filter(|value| *value != "none")
            .and_then(Color::parse),
        label: String::new(),
    }))
}

fn svg_ellipse(tag: &str, circle: bool) -> Option<Element> {
    let cx = svg_f32(tag, "cx")?;
    let cy = svg_f32(tag, "cy")?;
    let (rx, ry) = if circle {
        let r = svg_f32(tag, "r")?;
        (r, r)
    } else {
        (svg_f32(tag, "rx")?, svg_f32(tag, "ry")?)
    };
    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    Some(Element::Shape(Shape {
        id: svg_id(tag),
        kind: ShapeKind::Ellipse,
        bounds: Rect {
            x: cx - rx,
            y: cy - ry,
            width: rx * 2.0,
            height: ry * 2.0,
        },
        rotation_degrees: 0.0,
        style: svg_style(tag),
        fill: svg_attr(tag, "fill")
            .filter(|value| *value != "none")
            .and_then(Color::parse),
        label: String::new(),
    }))
}

fn svg_text(tag: &str) -> Option<Element> {
    let inner = tag.split_once('>')?.1;
    let text = inner
        .split("</text>")
        .next()?
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_owned();
    if text.is_empty() {
        return None;
    }
    let font_size = svg_f32(tag, "font-size").unwrap_or(18.0).max(4.0);
    let mut note = TextNote::plain(
        Point::new(
            svg_f32(tag, "x").unwrap_or(0.0),
            svg_f32(tag, "y").unwrap_or(0.0),
        ),
        text,
        font_size,
        svg_attr(tag, "fill")
            .and_then(Color::parse)
            .unwrap_or(Color::INK),
    );
    note.id = svg_id(tag);
    Some(Element::Text(note))
}

fn svg_path(tag: &str) -> Option<Element> {
    let data = svg_attr(tag, "d")?;
    let points = parse_svg_points(data);
    if points.len() < 2 {
        return None;
    }
    Some(Element::Stroke(Stroke {
        id: svg_id(tag),
        kind: StrokeKind::Pen,
        style: svg_style(tag),
        points: points
            .into_iter()
            .map(|point| StrokePoint::new(point, 1.0))
            .collect(),
    }))
}
