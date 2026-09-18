use crate::spreadsheet::formula::{ErrorKind, Value, format_general, parse_number_like};

pub fn format_value(value: &Value, format: &str) -> String {
    if let Value::Error(kind) = value {
        return kind.to_string();
    }
    let format = format.trim();
    if format.is_empty() || format.eq_ignore_ascii_case("General") || format == "@" {
        return value.display();
    }
    if matches!(value, Value::Bool(_) | Value::Text(_))
        && !is_date_format(format)
        && !format.contains('0')
    {
        return value.display();
    }
    let number = match value.as_number() {
        Ok(number) => number,
        Err(kind) => return kind.to_string(),
    };
    if is_percent_format(format) {
        return apply_numeric_format(number * 100.0, format.trim_end_matches('%')) + "%";
    }
    if is_date_format(format) {
        return format_date(number, format);
    }
    if is_scientific(format) {
        return format_scientific(number, format);
    }
    apply_numeric_format(number, format)
}

pub fn format_cell(value: &Value, format: &str) -> String {
    format_value(value, format)
}

fn is_percent_format(format: &str) -> bool {
    format.contains('%')
}

fn is_scientific(format: &str) -> bool {
    format.to_ascii_uppercase().contains('E')
}

fn is_date_format(format: &str) -> bool {
    let lower = format.to_ascii_lowercase();
    (lower.contains('y') || lower.contains('d') || lower.contains("mmm") || lower.contains("hh"))
        && !lower.contains('#')
}

fn apply_numeric_format(number: f64, format: &str) -> String {
    let negative = number < 0.0;
    let abs = number.abs();
    let decimals = decimal_places(format);
    let rounded = if decimals >= 0 {
        let factor = 10f64.powi(decimals);
        (abs * factor).round() / factor
    } else {
        abs
    };
    let mut body = if format.contains('#') || format.contains(',') {
        with_thousands(rounded, decimals.max(0) as usize)
    } else if decimals >= 0 {
        format!("{:.*}", decimals as usize, rounded)
    } else {
        format_general(rounded)
    };
    if format.contains('$') && !body.starts_with('$') {
        body = format!("${body}");
    }
    if negative { format!("-{body}") } else { body }
}

fn decimal_places(format: &str) -> i32 {
    let Some((_, frac)) = format.split_once('.') else {
        if format.contains('0') || format.contains('#') {
            return 0;
        }
        return -1;
    };
    frac.chars()
        .take_while(|ch| *ch == '0' || *ch == '#')
        .count() as i32
}

fn with_thousands(value: f64, decimals: usize) -> String {
    let formatted = format!("{value:.decimals$}");
    let (int, frac) = formatted
        .split_once('.')
        .map_or((formatted.as_str(), None), |(a, b)| (a, Some(b)));
    let mut grouped = String::new();
    for (index, ch) in int.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let int = grouped.chars().rev().collect::<String>();
    match frac {
        Some(frac) => format!("{int}.{frac}"),
        None => int,
    }
}

fn format_scientific(number: f64, format: &str) -> String {
    let decimals = decimal_places(format).max(0) as usize;
    format!("{number:.decimals$E}")
}

fn format_date(serial: f64, format: &str) -> String {
    let Some((year, month, day)) = super::formula::serial_ymd(serial.floor() as i64) else {
        return ErrorKind::Num.to_string();
    };
    let lower = format.to_ascii_lowercase();
    if lower.contains("yyyy")
        || lower.contains("mmm")
        || lower.contains("dd")
        || lower.contains('y')
    {
        let mut out = format.to_owned();
        out = replace_ci(&out, "yyyy", &format!("{year:04}"));
        out = replace_ci(&out, "yy", &format!("{:02}", year % 100));
        out = replace_ci(&out, "mm", &format!("{month:02}"));
        out = replace_ci(&out, "m", &month.to_string());
        out = replace_ci(&out, "dd", &format!("{day:02}"));
        out = replace_ci(&out, "d", &day.to_string());
        return out;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn replace_ci(input: &str, needle: &str, replacement: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::new();
    let mut index = 0;
    let bytes = lower.as_bytes();
    let needle_bytes = needle.as_bytes();
    while index < input.len() {
        if bytes[index..].starts_with(needle_bytes) {
            out.push_str(replacement);
            index += needle.len();
        } else {
            out.push(input[index..].chars().next().unwrap());
            index += input[index..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

pub fn looks_like_number(input: &str) -> bool {
    parse_number_like(input).is_some()
}
