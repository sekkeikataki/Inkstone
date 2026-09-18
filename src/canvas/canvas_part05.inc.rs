            let _ = context.stroke();
            for index in 0..8 {
                let angle = index as f64 * std::f64::consts::TAU / 8.0;
                let inner = box_bounds.width.min(box_bounds.height) as f64 * 0.36;
                let outer = box_bounds.width.min(box_bounds.height) as f64 * 0.54;
                context.move_to(
                    box_bounds.center().x as f64 + inner * angle.cos(),
                    box_bounds.center().y as f64 + inner * angle.sin(),
                );
                context.line_to(
                    box_bounds.center().x as f64 + outer * angle.cos(),
                    box_bounds.center().y as f64 + outer * angle.sin(),
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Bearing => {
            ellipse_path(context, box_bounds);
            let _ = context.stroke();
            context.arc(
                box_bounds.center().x as f64,
                box_bounds.center().y as f64,
                box_bounds.width.min(box_bounds.height) as f64 * 0.18,
                0.0,
                std::f64::consts::TAU,
            );
            let _ = context.stroke();
            for index in 0..6 {
                let angle = index as f64 * std::f64::consts::TAU / 6.0;
                context.arc(
                    box_bounds.center().x as f64
                        + box_bounds.width.min(box_bounds.height) as f64 * 0.34 * angle.cos(),
                    box_bounds.center().y as f64
                        + box_bounds.width.min(box_bounds.height) as f64 * 0.34 * angle.sin(),
                    box_bounds.width.min(box_bounds.height) as f64 * 0.06,
                    0.0,
                    std::f64::consts::TAU,
                );
            }
            let _ = context.stroke();
        }
        ShapeKind::Beam => {
            context.rectangle(
                box_bounds.x as f64,
                box_bounds.y as f64,
                box_bounds.width as f64,
                box_bounds.height as f64,
            );
            let _ = context.stroke();
            let mut x = box_bounds.x - box_bounds.height;
            while x < box_bounds.x + box_bounds.width {
                context.move_to(
                    x.max(box_bounds.x) as f64,
                    (box_bounds.y + box_bounds.height) as f64,
                );
                context.line_to(
                    (x + box_bounds.height).min(box_bounds.x + box_bounds.width) as f64,
                    box_bounds.y as f64,
                );
                x += box_bounds.height * 0.65;
            }
            let _ = context.stroke();
        }
        ShapeKind::Lamp => draw_iec_lamp(context, box_bounds),
        ShapeKind::Transformer => draw_iec_transformer(context, box_bounds),
        ShapeKind::AndGate | ShapeKind::NandGate => {
            draw_logic_and(context, box_bounds, shape.kind == ShapeKind::NandGate);
        }
        ShapeKind::OrGate | ShapeKind::NorGate | ShapeKind::XorGate => {
            draw_logic_or(context, box_bounds, shape.kind);
        }
        ShapeKind::NotGate => draw_logic_not(context, box_bounds),
        ShapeKind::SurfaceFinish => {
            context.move_to(
                box_bounds.x as f64,
                (box_bounds.y + box_bounds.height * 0.62) as f64,
            );
            context.line_to(
                (box_bounds.x + box_bounds.width * 0.32) as f64,
                (box_bounds.y + box_bounds.height) as f64,
            );
            context.line_to(
                (box_bounds.x + box_bounds.width) as f64,
                box_bounds.y as f64,
            );
            let _ = context.stroke();
        }
        ShapeKind::ThirdAngle => draw_third_angle(context, box_bounds),
    }
    let _ = context.restore();

    if !shape.label.is_empty() {
        draw_label(
            context,
            Point::new(
                box_bounds.center().x,
                box_bounds.y.min(box_bounds.y + box_bounds.height) + box_bounds.height + 16.0,
            ),
            &shape.label,
            shape.style.color,
        );
    }
}

fn draw_arrow_head(context: &Context, start: Point, end: Point, bounds: Rect) {
    let angle = (end.y - start.y).atan2(end.x - start.x);
    let size = (bounds.width.abs().hypot(bounds.height.abs()) * 0.18).clamp(8.0, 22.0);
    context.move_to(end.x as f64, end.y as f64);
    context.line_to(
        (end.x - size * (angle - 0.45).cos()) as f64,
        (end.y - size * (angle - 0.45).sin()) as f64,
    );
    context.move_to(end.x as f64, end.y as f64);
    context.line_to(
        (end.x - size * (angle + 0.45).cos()) as f64,
        (end.y - size * (angle + 0.45).sin()) as f64,
    );
    let _ = context.stroke();
}

fn draw_zigzag(context: &Context, bounds: Rect) {
    context.move_to(bounds.x as f64, bounds.center().y as f64);
    for index in 0..=8 {
        let x = bounds.x + bounds.width * (index as f32 + 1.0) / 10.0;
        let y = if index % 2 == 0 {
            bounds.y + bounds.height * 0.2
        } else {
            bounds.y + bounds.height * 0.8
        };
        context.line_to(x as f64, y as f64);
    }
    context.line_to((bounds.x + bounds.width) as f64, bounds.center().y as f64);
    let _ = context.stroke();
}

fn draw_iec_resistor(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.22;
    let x2 = bounds.x + bounds.width * 0.78;
    let y1 = bounds.y + bounds.height * 0.28;
    let y2 = bounds.y + bounds.height * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.rectangle(x1 as f64, y1 as f64, (x2 - x1) as f64, (y2 - y1) as f64);
    let _ = context.stroke();
}

fn draw_iec_diode(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.62;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, (bounds.y + bounds.height * 0.18) as f64);
    context.line_to(x2 as f64, cy as f64);
    context.line_to(x1 as f64, (bounds.y + bounds.height * 0.82) as f64);
    context.close_path();
    let _ = context.stroke();
    context.move_to(x2 as f64, (bounds.y + bounds.height * 0.18) as f64);
    context.line_to(x2 as f64, (bounds.y + bounds.height * 0.82) as f64);
    let _ = context.stroke();
}

fn draw_iec_inductor(context: &Context, bounds: Rect) {
    let cy = bounds.center().y as f64;
    let radius = (bounds.width.abs() / 10.0) as f64;
    context.move_to(bounds.x as f64, cy);
    context.line_to((bounds.x + bounds.width * 0.18) as f64, cy);
    for index in 0..4 {
        let cx = (bounds.x + bounds.width * (0.26 + index as f32 * 0.14)) as f64;
        context.arc(cx, cy, radius, std::f64::consts::PI, 0.0);
    }
    context.line_to((bounds.x + bounds.width) as f64, cy);
    let _ = context.stroke();
}

fn draw_iec_switch(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, cy as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.62) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    let _ = context.stroke();
    context.arc(x1 as f64, cy as f64, 2.4, 0.0, std::f64::consts::TAU);
    let _ = context.stroke();
    context.arc(x2 as f64, cy as f64, 2.4, 0.0, std::f64::consts::TAU);
    let _ = context.stroke();
}

fn draw_iec_fuse(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.28;
    let x2 = bounds.x + bounds.width * 0.72;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.rectangle(
        x1 as f64,
        (bounds.y + bounds.height * 0.32) as f64,
        (x2 - x1) as f64,
        (bounds.height * 0.36) as f64,
    );
    let _ = context.stroke();
}

fn draw_iec_battery(context: &Context, bounds: Rect) {
    let cy = bounds.center().y;
    let x1 = bounds.x + bounds.width * 0.42;
    let x2 = bounds.x + bounds.width * 0.58;
    context.move_to(bounds.x as f64, cy as f64);
    context.line_to(x1 as f64, cy as f64);
    context.move_to(x2 as f64, cy as f64);
    context.line_to((bounds.x + bounds.width) as f64, cy as f64);
    context.move_to(x1 as f64, (bounds.y + bounds.height * 0.12) as f64);
    context.line_to(x1 as f64, (bounds.y + bounds.height * 0.88) as f64);
    context.move_to(x2 as f64, (bounds.y + bounds.height * 0.28) as f64);
    context.line_to(x2 as f64, (bounds.y + bounds.height * 0.72) as f64);
    let _ = context.stroke();
}

fn draw_iec_lamp(context: &Context, bounds: Rect) {
    let r = bounds.width.min(bounds.height) / 2.0;
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        r as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.move_to(
        (bounds.center().x - r * 0.62) as f64,
        (bounds.center().y - r * 0.62) as f64,
    );
    context.line_to(
        (bounds.center().x + r * 0.62) as f64,
        (bounds.center().y + r * 0.62) as f64,
    );
    context.move_to(
        (bounds.center().x - r * 0.62) as f64,
        (bounds.center().y + r * 0.62) as f64,
    );
    context.line_to(
        (bounds.center().x + r * 0.62) as f64,
        (bounds.center().y - r * 0.62) as f64,
    );
    let _ = context.stroke();
}

fn draw_iec_transformer(context: &Context, bounds: Rect) {
    let left = Rect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width * 0.42,
        height: bounds.height,
    };
    let right = Rect {
        x: bounds.x + bounds.width * 0.58,
        y: bounds.y,
        width: bounds.width * 0.42,
        height: bounds.height,
    };
    draw_iec_inductor(context, left);
    draw_iec_inductor(context, right);
    context.move_to(
        (bounds.x + bounds.width * 0.46) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.46) as f64,
        (bounds.y + bounds.height * 0.82) as f64,
    );
    context.move_to(
        (bounds.x + bounds.width * 0.54) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.54) as f64,
        (bounds.y + bounds.height * 0.82) as f64,
    );
    let _ = context.stroke();
}

fn draw_logic_and(context: &Context, bounds: Rect, nand: bool) {
    let mid = bounds.x + bounds.width * 0.55;
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.line_to(mid as f64, bounds.y as f64);
    context.arc(
        mid as f64,
        bounds.center().y as f64,
        (bounds.height / 2.0) as f64,
        -std::f64::consts::FRAC_PI_2,
        std::f64::consts::FRAC_PI_2,
    );
    context.line_to(bounds.x as f64, (bounds.y + bounds.height) as f64);
    context.close_path();
    let _ = context.stroke();
    if nand {
        context.arc(
            (bounds.x + bounds.width * 0.88) as f64,
            bounds.center().y as f64,
            (bounds.width * 0.07) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.stroke();
    }
}

fn draw_logic_or(context: &Context, bounds: Rect, kind: ShapeKind) {
    let cy = bounds.center().y;
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.curve_to(
        (bounds.x + bounds.width * 0.22) as f64,
        bounds.y as f64,
        (bounds.x + bounds.width * 0.55) as f64,
        (bounds.y + bounds.height * 0.08) as f64,
        (bounds.x + bounds.width * 0.82) as f64,
        cy as f64,
    );
    context.curve_to(
        (bounds.x + bounds.width * 0.55) as f64,
        (bounds.y + bounds.height * 0.92) as f64,
        (bounds.x + bounds.width * 0.22) as f64,
        (bounds.y + bounds.height) as f64,
        bounds.x as f64,
        (bounds.y + bounds.height) as f64,
    );
    context.curve_to(
        (bounds.x + bounds.width * 0.18) as f64,
        cy as f64,
        (bounds.x + bounds.width * 0.18) as f64,
        cy as f64,
        bounds.x as f64,
        bounds.y as f64,
    );
    let _ = context.stroke();
    if kind == ShapeKind::XorGate {
        context.move_to((bounds.x + bounds.width * 0.08) as f64, bounds.y as f64);
        context.curve_to(
            (bounds.x + bounds.width * 0.26) as f64,
            cy as f64,
            (bounds.x + bounds.width * 0.26) as f64,
            cy as f64,
            (bounds.x + bounds.width * 0.08) as f64,
            (bounds.y + bounds.height) as f64,
        );
        let _ = context.stroke();
    }
    if kind == ShapeKind::NorGate {
        context.arc(
            (bounds.x + bounds.width * 0.9) as f64,
            cy as f64,
            (bounds.width * 0.07) as f64,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = context.stroke();
    }
}

fn draw_logic_not(context: &Context, bounds: Rect) {
    context.move_to(bounds.x as f64, bounds.y as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.72) as f64,
        bounds.center().y as f64,
    );
    context.line_to(bounds.x as f64, (bounds.y + bounds.height) as f64);
    context.close_path();
    let _ = context.stroke();
    context.arc(
        (bounds.x + bounds.width * 0.84) as f64,
        bounds.center().y as f64,
        (bounds.width * 0.08) as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
}

fn draw_third_angle(context: &Context, bounds: Rect) {
    let r = bounds.width.min(bounds.height) * 0.38;
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        r as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.arc(
        bounds.center().x as f64,
        bounds.center().y as f64,
        (r * 0.42) as f64,
        0.0,
        std::f64::consts::TAU,
    );
    let _ = context.stroke();
    context.move_to((bounds.x + bounds.width * 0.18) as f64, bounds.y as f64);
    context.line_to(
        (bounds.x + bounds.width * 0.32) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to(
        (bounds.x + bounds.width * 0.68) as f64,
        (bounds.y + bounds.height * 0.18) as f64,
    );
    context.line_to((bounds.x + bounds.width * 0.82) as f64, bounds.y as f64);
    context.close_path();
    let _ = context.stroke();
}

fn sanitize_export_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        "export.png".to_owned()
    } else {
        trimmed
    }
}

fn ellipse_path(context: &Context, bounds: Rect) {
    let _ = context.save();
    context.translate(bounds.center().x as f64, bounds.center().y as f64);
    context.scale(bounds.width as f64 / 2.0, bounds.height as f64 / 2.0);
    context.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    let _ = context.restore();
}

fn fill_and_stroke(context: &Context, fill: Option<Color>, stroke: Color) {
    if let Some(fill) = fill {
        set_source(context, fill);
        let _ = context.fill_preserve();
        set_source(context, stroke);
    }
    let _ = context.stroke();
}

fn draw_label(context: &Context, origin: Point, label: &str, color: Color) {
    set_source(context, color);
    context.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    context.set_font_size(14.0);
    let width = context
        .text_extents(label)
        .map(|value| value.width())
        .unwrap_or(0.0);
    context.move_to(origin.x as f64 - width / 2.0, origin.y as f64);
    let _ = context.show_text(label);
}

fn set_source(context: &Context, color: Color) {
    context.set_source_rgba(
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        color.alpha as f64,
    );
}
