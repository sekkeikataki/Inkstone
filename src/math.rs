//! Typeset a practical subset of LaTeX math (the dialect Gemini and other chat apps copy)
//! into positioned glyphs, fraction bars, radicals, and stretchy delimiters.
//!
//! Notes keep the original `$...$` / `$$...$$` source. Rendering is derived from it, so the
//! notebook file stays plain text and an equation can still be edited as LaTeX.

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Roman,
    Italic,
    Bold,
    BoldItalic,
    Sans,
    SansBold,
    SansItalic,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Glyph {
        x: f32,
        y: f32,
        size: f32,
        text: String,
        face: Face,
    },
    Rule {
        x: f32,
        y: f32,
        width: f32,
        weight: f32,
    },
    Path {
        points: Vec<(f32, f32)>,
        weight: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Distance from the first line's baseline, positive downward.
    pub baseline: f32,
    pub ascent: f32,
    pub descent: f32,
    pub width: f32,
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub width: f32,
    pub ascent: f32,
    pub height: f32,
}

pub struct NoteStyle<'a> {
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub prefix: &'a str,
}

pub trait Measurer {
    fn measure(&self, text: &str, size: f32, face: Face) -> f32;
}

#[derive(Clone, Copy, Debug)]
pub struct Estimate;

impl Measurer for Estimate {
    fn measure(&self, text: &str, size: f32, face: Face) -> f32 {
        let italic = matches!(face, Face::Italic | Face::BoldItalic | Face::SansItalic);
        let factor = if italic { 1.05 } else { 1.0 };
        text.chars().map(|ch| em_width(ch) * size * factor).sum()
    }
}

fn em_width(ch: char) -> f32 {
    match ch {
        ' ' | '\u{00a0}' => 0.36,
        'i' | 'l' | 'j' | 'I' | 'ì' | 'í' | 'ï' | '\'' | '′' | '|' | '·' | '⋅' | '.' | ','
        | ':' | ';' | '!' => 0.36,
        'f' | 't' | 'r' => 0.44,
        'm' | 'w' | 'M' | 'W' => 0.96,
        '0'..='9' => 0.64,
        c if c.is_ascii_uppercase() => 0.78,
        c if c.is_ascii_lowercase() => 0.64,
        '←' | '→' | '↔' | '⇐' | '⇒' | '⇔' | '↦' => 1.05,
        '∑' | '∏' | '∫' | '∮' => 0.85,
        '≤' | '≥' | '≠' | '≈' | '≡' | '∼' => 0.9,
        _ => 0.68,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteList {
    None,
    Bullet,
    Numbered,
    Checklist,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PasteBlock {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub list: PasteList,
    pub checked: bool,
    pub scale: f32,
}

/// True when `text` contains LaTeX delimiters or a bare math command worth typesetting.
pub fn has_math(text: &str) -> bool {
    if has_math_fence(text) {
        return true;
    }
    let chars: Vec<char> = text.chars().collect();
    if !protected_spans(&chars).is_empty() {
        return true;
    }
    chars.iter().enumerate().any(|(index, ch)| {
        *ch == '\\'
            && command_name_at(&chars, index)
                .is_some_and(|name| is_math_command(name.trim_end_matches('*')))
    })
}

pub fn layout_note(text: &str, style: NoteStyle<'_>, measure: &dyn Measurer) -> Layout {
    let font_size = if style.font_size.is_finite() && style.font_size > 0.0 {
        style.font_size
    } else {
        16.0
    };
    let promoted = promote_bare_latex(text);
    let visual = tokenize(&promoted);
    let laid: Vec<BoxLayout> = visual
        .iter()
        .map(|line| layout_visual_line(line, font_size, &style, measure))
        .collect();
    if laid.is_empty() {
        return Layout {
            lines: Vec::new(),
            width: font_size,
            ascent: font_size,
            height: font_size * 1.25,
        };
    }
    let mut baseline = 0.0;
    let mut lines = Vec::with_capacity(laid.len());
    let mut width = 0.0_f32;
    for (index, child) in laid.iter().enumerate() {
        let mut ops = child.ops.clone();
        let mut descent = child.descent;
        if style.underline && child.width > 0.0 {
            let weight = (font_size * 0.06).max(0.8);
            let y = descent + font_size * 0.08;
            ops.push(Op::Rule {
                x: 0.0,
                y,
                width: child.width,
                weight,
            });
            descent = y + weight;
        }
        lines.push(Line {
            baseline,
            ascent: child.ascent,
            descent,
            width: child.width,
            ops,
        });
        width = width.max(child.width);
        if index + 1 < laid.len() {
            baseline += descent + laid[index + 1].ascent;
        }
    }
    let ascent = lines[0].ascent;
    let height = ascent
        + lines
            .last()
            .map(|line| line.baseline + line.descent)
            .unwrap_or(0.0);
    Layout {
        lines,
        width,
        ascent,
        height: height.max(font_size),
    }
}

/// Split a Gemini-style Markdown clipboard into notes when it contains math.
/// Prose pastes return `None` so ordinary text keeps the single-note paste path.
pub fn pasted_blocks(input: &str) -> Option<Vec<PasteBlock>> {
    if !has_math(input) {
        return None;
    }
    let blocks = segment_markdown(input);
    if blocks.is_empty() {
        None
    } else {
        Some(blocks)
    }
}

fn has_math_fence(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        let Some(lang) = trimmed.strip_prefix("```") else {
            return false;
        };
        if trimmed.chars().filter(|ch| *ch == '`').count() != 3 {
            return false;
        }
        matches!(
            lang.trim().to_ascii_lowercase().as_str(),
            "math" | "latex" | "tex"
        )
    })
}

fn segment_markdown(input: &str) -> Vec<PasteBlock> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if is_rule(trimmed) {
            index += 1;
            continue;
        }
        if let Some(block) = take_fence(&lines, &mut index) {
            blocks.push(block);
            continue;
        }
        if let Some(block) = take_display(&lines, &mut index) {
            blocks.push(block);
            continue;
        }
        if let Some(block) = take_environment(&lines, &mut index) {
            blocks.push(block);
            continue;
        }
        if let Some((level, rest)) = heading(trimmed) {
            let (text, _) = clean_inline(rest);
            if !text.is_empty() {
                blocks.push(PasteBlock {
                    text,
                    bold: true,
                    italic: false,
                    list: PasteList::None,
                    checked: false,
                    scale: heading_scale(level),
                });
            }
            index += 1;
            continue;
        }
        if let Some(item) = list_item(trimmed) {
            let (text, bold) = clean_inline(item.text);
            if !text.is_empty() {
                blocks.push(PasteBlock {
                    text,
                    bold,
                    italic: false,
                    list: item.list,
                    checked: item.checked,
                    scale: 1.0,
                });
            }
            index += 1;
            while index < lines.len() {
                let next = lines[index];
                let next_trim = next.trim();
                if next_trim.is_empty()
                    || is_special_line(next_trim)
                    || !next.starts_with("  ") && !next.starts_with('\t')
                {
                    break;
                }
                let (more, _) = clean_inline(next_trim);
                if !more.is_empty()
                    && let Some(last) = blocks.last_mut()
                {
                    last.text.push('\n');
                    last.text.push_str(&more);
                }
                index += 1;
            }
            continue;
        }
        let mut paragraph = Vec::new();
        let mut bold = false;
        while index < lines.len() {
            let line = lines[index].trim();
            if line.is_empty() || is_special_line(line) {
                break;
            }
            let unquoted = line.strip_prefix("> ").unwrap_or(line).trim();
            let (text, line_bold) = clean_inline(unquoted);
            bold |= line_bold;
            if !text.is_empty() {
                paragraph.push(text);
            }
            index += 1;
        }
        if !paragraph.is_empty() {
            blocks.push(PasteBlock {
                text: paragraph.join("\n"),
                bold,
                italic: false,
                list: PasteList::None,
                checked: false,
                scale: 1.0,
            });
        }
    }
    blocks
}

fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.55,
        2 => 1.32,
        3 => 1.16,
        _ => 1.05,
    }
}

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3
        && (line.chars().all(|ch| ch == '-')
            || line.chars().all(|ch| ch == '*')
            || line.chars().all(|ch| ch == '_'))
}

fn is_special_line(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("$$")
        || line.starts_with("\\[")
        || line.starts_with("```")
        || line.contains("\\begin{")
        || heading(line).is_some()
        || list_item(line).is_some()
        || is_rule(line)
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = line[hashes..].strip_prefix([' ', '\t'])?;
    Some((hashes as u8, rest.trim()))
}

struct ListItem<'a> {
    list: PasteList,
    checked: bool,
    text: &'a str,
}

fn list_item(line: &str) -> Option<ListItem<'_>> {
    let (list, rest) = if let Some(rest) = line
        .strip_prefix("* ")
        .or_else(|| line.strip_prefix("- "))
        .or_else(|| line.strip_prefix("+ "))
    {
        (PasteList::Bullet, rest)
    } else {
        let rest = numbered_rest(line)?;
        (PasteList::Numbered, rest)
    };
    if let Some(text) = rest.strip_prefix("[ ] ") {
        return Some(ListItem {
            list: PasteList::Checklist,
            checked: false,
            text,
        });
    }
    if let Some(text) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        return Some(ListItem {
            list: PasteList::Checklist,
            checked: true,
            text,
        });
    }
    Some(ListItem {
        list,
        checked: false,
        text: rest,
    })
}

fn numbered_rest(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    line[digits..]
        .strip_prefix(". ")
        .or_else(|| line[digits..].strip_prefix(") "))
}

fn take_fence(lines: &[&str], index: &mut usize) -> Option<PasteBlock> {
    let trimmed = lines[*index].trim();
    let lang = trimmed.strip_prefix("```")?;
    if trimmed.chars().filter(|ch| *ch == '`').count() != 3 {
        return None;
    }
    let lang = lang.trim().to_ascii_lowercase();
    let mut body = String::new();
    *index += 1;
    while *index < lines.len() {
        if lines[*index].trim().starts_with("```") {
            *index += 1;
            break;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(lines[*index].trim());
        *index += 1;
    }
    let math_lang = matches!(lang.as_str(), "math" | "latex" | "tex");
    if math_lang || has_math(&body) {
        Some(display_block(body))
    } else if body.trim().is_empty() {
        None
    } else {
        Some(PasteBlock {
            text: body,
            bold: false,
            italic: false,
            list: PasteList::None,
            checked: false,
            scale: 1.0,
        })
    }
}

fn take_display(lines: &[&str], index: &mut usize) -> Option<PasteBlock> {
    let trimmed = lines[*index].trim();
    if let Some(body) = trimmed.strip_prefix("$$") {
        if let Some(body) = body.strip_suffix("$$")
            && (trimmed.len() > 4 || body.is_empty())
        {
            *index += 1;
            let body = body.trim();
            if body.is_empty() {
                return None;
            }
            return Some(display_block(body.to_owned()));
        }
        if trimmed == "$$" || trimmed.starts_with("$$") {
            let mut body = String::new();
            let first = trimmed.trim_start_matches('$').trim();
            if !first.is_empty() {
                body.push_str(first);
            }
            *index += 1;
            while *index < lines.len() {
                let line = lines[*index].trim();
                *index += 1;
                if line == "$$" {
                    break;
                }
                if let Some(rest) = line.strip_suffix("$$") {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        if !body.is_empty() {
                            body.push('\n');
                        }
                        body.push_str(rest);
                    }
                    break;
                }
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(line);
            }
            if body.trim().is_empty() {
                return None;
            }
            return Some(display_block(body));
        }
    }
    if let Some(body) = trimmed.strip_prefix("\\[") {
        if let Some(body) = body.strip_suffix("\\]") {
            *index += 1;
            let body = body.trim();
            if body.is_empty() {
                return None;
            }
            return Some(display_block(body.to_owned()));
        }
        let mut body = String::new();
        let first = trimmed.trim_start_matches("\\[").trim();
        if !first.is_empty() {
            body.push_str(first);
        }
        *index += 1;
        while *index < lines.len() {
            let line = lines[*index].trim();
            *index += 1;
            if let Some(rest) = line.strip_suffix("\\]") {
                let rest = rest.trim();
                if !rest.is_empty() {
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(rest);
                }
                break;
            }
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
        if body.trim().is_empty() {
            return None;
        }
        return Some(display_block(body));
    }
    None
}

fn display_block(body: String) -> PasteBlock {
    let body = body.trim().to_owned();
    let text = if body.contains('\n') {
        format!("$$\n{body}\n$$")
    } else {
        format!("$${body}$$")
    };
    PasteBlock {
        text,
        bold: false,
        italic: false,
        list: PasteList::None,
        checked: false,
        scale: 1.0,
    }
}

fn take_environment(lines: &[&str], index: &mut usize) -> Option<PasteBlock> {
    let trimmed = lines[*index].trim();
    if !trimmed.contains("\\begin{") {
        return None;
    }
    let mut chunk = trimmed.to_owned();
    *index += 1;
    if !environment_closed(&chunk) {
        while *index < lines.len() {
            chunk.push('\n');
            chunk.push_str(lines[*index].trim_end());
            *index += 1;
            if environment_closed(&chunk) {
                break;
            }
        }
    }
    if chunk.trim().is_empty() {
        None
    } else {
        Some(PasteBlock {
            text: chunk,
            bold: false,
            italic: false,
            list: PasteList::None,
            checked: false,
            scale: 1.0,
        })
    }
}

fn environment_closed(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut open = 0_i32;
    while index < chars.len() {
        if starts_with(&chars, index, "\\begin{") {
            open += 1;
            index += "\\begin{".chars().count();
            continue;
        }
        if starts_with(&chars, index, "\\end{") {
            open -= 1;
            index += "\\end{".chars().count();
            if open <= 0 {
                return true;
            }
            continue;
        }
        index += 1;
    }
    false
}

fn clean_inline(input: &str) -> (String, bool) {
    let chars: Vec<char> = input.chars().collect();
    let spans = protected_spans(&chars);
    let trimmed = input.trim();
    let bold = trimmed.starts_with("**")
        && trimmed.ends_with("**")
        && trimmed.len() > 4
        && !trimmed[2..trimmed.len() - 2].contains("**");
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if let Some((start, end)) = spans
            .iter()
            .copied()
            .find(|(start, end)| index >= *start && index < *end)
            && index == start
        {
            out.extend(chars[start..end].iter());
            index = end;
            continue;
        }
        if chars[index] == '`'
            && let Some(end) = chars[index + 1..].iter().position(|ch| *ch == '`')
        {
            out.extend(chars[index + 1..index + 1 + end].iter());
            index = index + end + 2;
            continue;
        }
        if chars[index] == '!'
            && chars.get(index + 1) == Some(&'[')
            && let Some((label, next)) = parse_link(&chars, index + 1)
        {
            out.push_str(&label);
            index = next;
            continue;
        }
        if chars[index] == '['
            && let Some((label, next)) = parse_link(&chars, index)
        {
            out.push_str(&label);
            index = next;
            continue;
        }
        if chars[index] == '*'
            && chars.get(index + 1) == Some(&'*')
            && let Some(rel) = find_pattern(&chars[index + 2..], &['*', '*'])
        {
            out.extend(chars[index + 2..index + 2 + rel].iter());
            index = index + 2 + rel + 2;
            continue;
        }
        if chars[index] == '_'
            && chars.get(index + 1) == Some(&'_')
            && let Some(rel) = find_pattern(&chars[index + 2..], &['_', '_'])
        {
            out.extend(chars[index + 2..index + 2 + rel].iter());
            index = index + 2 + rel + 2;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    (out.trim().to_owned(), bold)
}

fn parse_link(chars: &[char], index: usize) -> Option<(String, usize)> {
    if chars.get(index) != Some(&'[') {
        return None;
    }
    let close = chars[index + 1..].iter().position(|ch| *ch == ']')?;
    let label: String = chars[index + 1..index + 1 + close].iter().collect();
    let after = index + 1 + close + 1;
    if chars.get(after) != Some(&'(') {
        return None;
    }
    let end = chars[after + 1..].iter().position(|ch| *ch == ')')?;
    if label.is_empty() {
        return None;
    }
    Some((label, after + 1 + end + 1))
}

fn find_pattern(chars: &[char], pattern: &[char]) -> Option<usize> {
    if pattern.is_empty() || chars.len() < pattern.len() {
        return None;
    }
    chars
        .windows(pattern.len())
        .position(|window| window == pattern)
}

#[derive(Clone, Debug)]
enum Piece {
    Prose(String),
    Math { latex: String, display: bool },
}

#[derive(Clone, Debug)]
struct VLine {
    pieces: Vec<Piece>,
}

struct Tok {
    chars: Vec<char>,
    index: usize,
    prose: String,
    pieces: Vec<Piece>,
    lines: Vec<VLine>,
}

impl Tok {
    fn flush_prose(&mut self) {
        if !self.prose.is_empty() {
            self.pieces
                .push(Piece::Prose(std::mem::take(&mut self.prose)));
        }
    }

    fn flush_line(&mut self) {
        self.flush_prose();
        self.lines.push(VLine {
            pieces: std::mem::take(&mut self.pieces),
        });
    }

    fn push_display(&mut self, latex: String) {
        self.flush_prose();
        if !self.pieces.is_empty() {
            self.flush_line();
        }
        let latex = latex.trim().to_owned();
        if !latex.is_empty() {
            self.lines.push(VLine {
                pieces: vec![Piece::Math {
                    latex,
                    display: true,
                }],
            });
        }
        if self.chars.get(self.index) == Some(&'\n') {
            self.index += 1;
        }
    }
}

fn tokenize(text: &str) -> Vec<VLine> {
    let mut tok = Tok {
        chars: text.chars().collect(),
        index: 0,
        prose: String::new(),
        pieces: Vec::new(),
        lines: Vec::new(),
    };
    while tok.index < tok.chars.len() {
        if at_line_start(&tok.chars, tok.index)
            && starts_with(&tok.chars, tok.index, "```")
            && let Some((body, end)) = read_fence(&tok.chars, tok.index)
        {
            let opener: String = tok.chars[tok.index..end].iter().collect();
            let lang = opener
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches('`')
                .trim()
                .to_ascii_lowercase();
            tok.index = end;
            if matches!(lang.as_str(), "math" | "latex" | "tex") || has_math(&body) {
                tok.push_display(body);
            } else {
                for (offset, line) in body.split('\n').enumerate() {
                    if offset > 0 {
                        tok.flush_line();
                    }
                    tok.prose.push_str(line);
                }
            }
            continue;
        }
        if !escaped(&tok.chars, tok.index)
            && starts_with(&tok.chars, tok.index, "$$")
            && let Some(end) = find_pattern(&tok.chars[tok.index + 2..], &['$', '$'])
        {
            let from = tok.index + 2;
            let to = from + end;
            let body: String = tok.chars[from..to].iter().collect();
            tok.index = to + 2;
            tok.push_display(body);
            continue;
        }
        if !escaped(&tok.chars, tok.index)
            && starts_with(&tok.chars, tok.index, "\\[")
            && let Some(end) = find_pattern(&tok.chars[tok.index + 2..], &['\\', ']'])
        {
            let from = tok.index + 2;
            let to = from + end;
            let body: String = tok.chars[from..to].iter().collect();
            tok.index = to + 2;
            tok.push_display(body);
            continue;
        }
        if !escaped(&tok.chars, tok.index)
            && starts_with(&tok.chars, tok.index, "\\begin{")
            && let Some(end) = environment_end(&tok.chars, tok.index)
        {
            let body: String = tok.chars[tok.index..end].iter().collect();
            tok.index = end;
            tok.push_display(body);
            continue;
        }
        if !escaped(&tok.chars, tok.index)
            && tok.chars[tok.index] == '$'
            && let Some(end) = inline_math_end(&tok.chars, tok.index)
        {
            tok.flush_prose();
            let latex: String = tok.chars[tok.index + 1..end - 1].iter().collect();
            tok.pieces.push(Piece::Math {
                latex,
                display: false,
            });
            tok.index = end;
            continue;
        }
        if !escaped(&tok.chars, tok.index)
            && starts_with(&tok.chars, tok.index, "\\(")
            && let Some(end) = find_pattern(&tok.chars[tok.index + 2..], &['\\', ')'])
        {
            tok.flush_prose();
            let from = tok.index + 2;
            let to = from + end;
            let latex: String = tok.chars[from..to].iter().collect();
            tok.pieces.push(Piece::Math {
                latex,
                display: false,
            });
            tok.index = to + 2;
            continue;
        }
        let ch = tok.chars[tok.index];
        tok.index += 1;
        if ch == '\n' {
            tok.flush_line();
        } else if ch != '\r' {
            tok.prose.push(ch);
        }
    }
    tok.flush_line();
    while tok.lines.last().is_some_and(|line| line.pieces.is_empty()) {
        tok.lines.pop();
    }
    while tok.lines.first().is_some_and(|line| line.pieces.is_empty()) {
        tok.lines.remove(0);
    }
    tok.lines
}

fn read_fence(chars: &[char], start: usize) -> Option<(String, usize)> {
    let end = fence_close_index(chars, start)?;
    let mut body_start = start;
    while body_start < end && chars[body_start] != '\n' {
        body_start += 1;
    }
    if body_start < end && chars[body_start] == '\n' {
        body_start += 1;
    }
    let mut body_end = body_start;
    let mut cursor = body_start;
    while cursor < end {
        if at_line_start(chars, cursor) && starts_with(chars, cursor, "```") {
            body_end = cursor;
            break;
        }
        cursor += 1;
    }
    let body: String = chars[body_start..body_end].iter().collect();
    Some((body.trim_end_matches(['\n', '\r']).to_owned(), end))
}

fn fence_close_index(chars: &[char], start: usize) -> Option<usize> {
    if !starts_with(chars, start, "```") {
        return None;
    }
    let mut index = start + 3;
    while index < chars.len() && chars[index] != '\n' {
        index += 1;
    }
    if index < chars.len() && chars[index] == '\n' {
        index += 1;
    }
    while index < chars.len() {
        if at_line_start(chars, index) && starts_with(chars, index, "```") {
            index += 3;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            if index < chars.len() && chars[index] == '\n' {
                index += 1;
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

fn at_line_start(chars: &[char], index: usize) -> bool {
    index == 0 || chars[index - 1] == '\n'
}

fn starts_with(chars: &[char], index: usize, text: &str) -> bool {
    let pattern: Vec<char> = text.chars().collect();
    if index + pattern.len() > chars.len() {
        return false;
    }
    chars[index..index + pattern.len()] == pattern[..]
}

fn escaped(chars: &[char], index: usize) -> bool {
    let mut count = 0;
    let mut cursor = index;
    while cursor > 0 && chars[cursor - 1] == '\\' {
        count += 1;
        cursor -= 1;
    }
    count % 2 == 1
}

fn inline_math_end(chars: &[char], index: usize) -> Option<usize> {
    if chars.get(index) != Some(&'$') || escaped(chars, index) {
        return None;
    }
    if chars.get(index + 1) == Some(&'$')
        || chars.get(index + 1).is_none_or(|ch| ch.is_whitespace())
    {
        return None;
    }
    let mut cursor = index + 1;
    while cursor < chars.len() {
        if chars[cursor] == '\\' {
            cursor += 2;
            continue;
        }
        if chars[cursor] == '\n' {
            return None;
        }
        if chars[cursor] == '$' {
            if chars[cursor - 1].is_whitespace() {
                return None;
            }
            let body: String = chars[index + 1..cursor].iter().collect();
            if looks_like_math(&body) {
                return Some(cursor + 1);
            }
            return None;
        }
        cursor += 1;
    }
    None
}

fn looks_like_math(body: &str) -> bool {
    let body = body.trim();
    if body.is_empty() || body.chars().count() > 800 {
        return false;
    }
    if body.contains('\\') || body.contains('^') || body.contains('_') {
        return true;
    }
    let pure_number = body
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '.' | ' '));
    if pure_number {
        return false;
    }
    let letters = body.chars().filter(|ch| ch.is_alphabetic()).count();
    if letters > 0 && letters <= 12 && body.chars().any(|ch| "+-=<>/".contains(ch)) {
        return true;
    }
    letters > 0 && letters <= 2 && body.chars().count() <= 4
}

fn environment_end(chars: &[char], start: usize) -> Option<usize> {
    if !starts_with(chars, start, "\\begin{") {
        return None;
    }
    let name_start = start + "\\begin{".chars().count();
    let name_len = chars[name_start..].iter().position(|ch| *ch == '}')?;
    let name: String = chars[name_start..name_start + name_len].iter().collect();
    let token: Vec<char> = format!("\\end{{{name}}}").chars().collect();
    let mut depth = 1_i32;
    let mut index = name_start + name_len + 1;
    while index < chars.len() {
        if starts_with(chars, index, "\\begin{") {
            depth += 1;
        }
        if chars[index..].starts_with(&token) {
            depth -= 1;
            if depth == 0 {
                return Some(index + token.len());
            }
        }
        index += 1;
    }
    None
}

fn protected_spans(chars: &[char]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if escaped(chars, index) {
            index += 1;
            continue;
        }
        if starts_with(chars, index, "$$")
            && let Some(end) = find_pattern(&chars[index + 2..], &['$', '$'])
        {
            let close = index + 2 + end + 2;
            spans.push((index, close));
            index = close;
            continue;
        }
        if starts_with(chars, index, "\\[")
            && let Some(end) = find_pattern(&chars[index + 2..], &['\\', ']'])
        {
            let close = index + 2 + end + 2;
            spans.push((index, close));
            index = close;
            continue;
        }
        if starts_with(chars, index, "\\(")
            && let Some(end) = find_pattern(&chars[index + 2..], &['\\', ')'])
        {
            let close = index + 2 + end + 2;
            spans.push((index, close));
            index = close;
            continue;
        }
        if starts_with(chars, index, "\\begin{")
            && let Some(end) = environment_end(chars, index)
        {
            spans.push((index, end));
            index = end;
            continue;
        }
        if chars[index] == '$'
            && let Some(end) = inline_math_end(chars, index)
        {
            spans.push((index, end));
            index = end;
            continue;
        }
        index += 1;
    }
    spans
}

fn promote_bare_latex(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let protected = protected_spans(&chars);
    let mut wraps: Vec<(usize, usize)> = Vec::new();
    let mut deleted = HashSet::new();
    let mut index = 0;
    while index < chars.len() {
        if let Some((_, end)) = span_at(&protected, index) {
            index = end;
            continue;
        }
        if is_promotable(&chars, index) {
            let mut end = command_end(&chars, index);
            end = expand_forward(&chars, end, &protected);
            let mut start = expand_back(&chars, index, &protected);
            if let Some((_, prev)) = wraps.last() {
                start = start.max(*prev);
            }
            end = end.max(start);
            if let Some((span_start, _)) = span_at(&protected, end.saturating_sub(1))
                && end > span_start
            {
                end = span_start;
            }
            if end > start {
                if chars.get(end) == Some(&'$') && inline_math_end(&chars, end).is_none() {
                    deleted.insert(end);
                }
                wraps.push((start, end));
                index = end;
                continue;
            }
        }
        index += 1;
    }
    let wraps = merge_wraps(wraps, &chars);
    let mut out = String::new();
    let mut index = 0;
    let mut wrap_index = 0;
    while index < chars.len() {
        if wrap_index < wraps.len() && index == wraps[wrap_index].0 {
            out.push('$');
            while index < wraps[wrap_index].1 {
                if !deleted.contains(&index) {
                    out.push(chars[index]);
                }
                index += 1;
            }
            out.push('$');
            wrap_index += 1;
            continue;
        }
        if deleted.contains(&index) {
            index += 1;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn span_at(spans: &[(usize, usize)], index: usize) -> Option<(usize, usize)> {
    spans
        .iter()
        .copied()
        .find(|(start, end)| index >= *start && index < *end)
}

fn merge_wraps(wraps: Vec<(usize, usize)>, chars: &[char]) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in wraps {
        if let Some((_, prev_end)) = merged.last_mut()
            && *prev_end <= start
            && chars[*prev_end..start].iter().all(|ch| ch.is_whitespace())
        {
            *prev_end = end;
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn is_promotable(chars: &[char], index: usize) -> bool {
    let Some(name) = command_name_at(chars, index) else {
        return false;
    };
    let base = name.trim_end_matches('*');
    !matches!(base, "begin" | "end" | "left" | "right") && is_math_command(base)
}

fn command_name_at(chars: &[char], index: usize) -> Option<String> {
    if chars.get(index) != Some(&'\\') {
        return None;
    }
    let mut cursor = index + 1;
    let mut name = String::new();
    while chars.get(cursor).is_some_and(|ch| ch.is_ascii_alphabetic()) {
        name.push(chars[cursor]);
        cursor += 1;
    }
    if name.is_empty() {
        return None;
    }
    if chars.get(cursor) == Some(&'*') {
        name.push('*');
    }
    Some(name)
}

fn command_end(chars: &[char], start: usize) -> usize {
    let Some(name) = command_name_at(chars, start) else {
        return start + 1;
    };
    let mut index = start + 1 + name.chars().count();
    let base = name.trim_end_matches('*');
    if base == "sqrt" {
        index = skip_spaces(chars, index);
        if chars.get(index) == Some(&'[') {
            index = skip_bracket(chars, index);
        }
    }
    let groups = match base {
        "frac" | "dfrac" | "tfrac" | "cfrac" | "binom" | "dbinom" | "tbinom" => 2,
        "sqrt" | "text" | "mbox" | "textrm" | "textbf" | "operatorname" | "mathrm" | "mathbf"
        | "mathit" | "boldsymbol" | "mathbb" | "mathcal" | "mathsf" | "overline" | "underline"
        | "overrightarrow" | "overleftarrow" | "hat" | "bar" | "vec" | "dot" | "ddot" | "tilde"
        | "widehat" | "widetilde" | "color" | "not" => 1,
        _ => 0,
    };
    for _ in 0..groups {
        index = skip_spaces(chars, index);
        index = consume_tex_arg(chars, index);
    }
    index
}

fn consume_tex_arg(chars: &[char], index: usize) -> usize {
    let index = skip_spaces(chars, index);
    match chars.get(index).copied() {
        Some('{') => skip_braced(chars, index),
        Some('\\') => command_end(chars, index),
        Some(_) => index + 1,
        None => index,
    }
}

fn skip_spaces(chars: &[char], mut index: usize) -> usize {
    while chars
        .get(index)
        .is_some_and(|ch| ch.is_whitespace() && *ch != '\n')
    {
        index += 1;
    }
    index
}

fn skip_braced(chars: &[char], index: usize) -> usize {
    if chars.get(index) != Some(&'{') {
        return index;
    }
    let mut depth = 0_i32;
    let mut cursor = index;
    while cursor < chars.len() {
        if chars[cursor] == '\\' && cursor + 1 < chars.len() {
            cursor += 2;
            continue;
        }
        if chars[cursor] == '{' {
            depth += 1;
        } else if chars[cursor] == '}' {
            depth -= 1;
            cursor += 1;
            if depth == 0 {
                return cursor;
            }
            continue;
        }
        cursor += 1;
    }
    chars.len()
}

fn skip_bracket(chars: &[char], index: usize) -> usize {
    let mut cursor = index;
    while cursor < chars.len() && chars[cursor] != ']' {
        cursor += 1;
    }
    (cursor + 1).min(chars.len())
}

fn expand_forward(chars: &[char], mut end: usize, protected: &[(usize, usize)]) -> usize {
    loop {
        let mut cursor = end;
        while chars
            .get(cursor)
            .is_some_and(|ch| ch.is_whitespace() && *ch != '\n')
        {
            cursor += 1;
        }
        if cursor >= chars.len()
            || span_at(protected, cursor).is_some()
            || chars[cursor] == '$'
            || chars[cursor] == '\n'
        {
            break;
        }
        if chars[cursor] == '^' || chars[cursor] == '_' {
            end = consume_script_span(chars, cursor);
            continue;
        }
        if chars[cursor] == '\\' && is_promotable(chars, cursor) {
            end = command_end(chars, cursor);
            continue;
        }
        if is_mathish(chars[cursor]) || single_letter(chars, cursor) {
            end = consume_math_run(chars, cursor, protected);
            if end == cursor {
                break;
            }
            continue;
        }
        break;
    }
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    end
}

fn consume_script_span(chars: &[char], index: usize) -> usize {
    let mut cursor = index + 1;
    cursor = skip_spaces(chars, cursor);
    match chars.get(cursor).copied() {
        Some('{') => skip_braced(chars, cursor),
        Some('\\') => command_end(chars, cursor),
        Some(_) => cursor + 1,
        None => cursor,
    }
}

fn consume_math_run(chars: &[char], start: usize, protected: &[(usize, usize)]) -> usize {
    let mut cursor = start;
    while cursor < chars.len() {
        if span_at(protected, cursor).is_some() || chars[cursor] == '$' || chars[cursor] == '\n' {
            break;
        }
        if chars[cursor].is_whitespace() {
            let mut next = cursor;
            while chars
                .get(next)
                .is_some_and(|ch| ch.is_whitespace() && *ch != '\n')
            {
                next += 1;
            }
            if next < chars.len()
                && (is_mathish(chars[next])
                    || single_letter(chars, next)
                    || is_promotable(chars, next))
            {
                cursor = next;
                continue;
            }
            break;
        }
        if is_mathish(chars[cursor]) {
            cursor += 1;
            continue;
        }
        if single_letter(chars, cursor) {
            cursor += 1;
            continue;
        }
        if is_promotable(chars, cursor) {
            cursor = command_end(chars, cursor);
            continue;
        }
        break;
    }
    cursor
}

fn expand_back(chars: &[char], command: usize, protected: &[(usize, usize)]) -> usize {
    let mut start = command;
    loop {
        let mut cursor = start;
        while cursor > 0 && chars[cursor - 1].is_whitespace() && chars[cursor - 1] != '\n' {
            cursor -= 1;
        }
        if cursor == 0 || chars[cursor - 1] == '\n' || chars[cursor - 1] == '$' {
            break;
        }
        if span_at(protected, cursor - 1).is_some() {
            break;
        }
        if chars[cursor - 1].is_alphabetic() {
            let mut run = cursor;
            while run > 0 && chars[run - 1].is_alphabetic() {
                run -= 1;
            }
            if cursor - run == 1 {
                start = run;
                continue;
            }
            break;
        }
        if is_mathish(chars[cursor - 1]) {
            let mut run = cursor;
            while run > 0 && is_mathish(chars[run - 1]) && span_at(protected, run - 1).is_none() {
                run -= 1;
            }
            if run == start {
                break;
            }
            start = run;
            continue;
        }
        break;
    }
    while start < command && chars[start].is_whitespace() {
        start += 1;
    }
    start
}

fn single_letter(chars: &[char], index: usize) -> bool {
    chars.get(index).is_some_and(|ch| ch.is_alphabetic())
        && !chars.get(index + 1).is_some_and(|ch| ch.is_alphabetic())
        && !chars
            .get(index.wrapping_sub(1))
            .is_some_and(|ch| index > 0 && ch.is_alphabetic())
}

fn is_mathish(ch: char) -> bool {
    ch.is_ascii_digit()
        || matches!(
            ch,
            '+' | '-'
                | '='
                | '/'
                | '*'
                | '^'
                | '_'
                | ','
                | '.'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '|'
                | '\''
                | '′'
                | '<'
                | '>'
                | '~'
        )
}

fn is_math_command(name: &str) -> bool {
    structural_command(name) || symbol(name).is_some()
}

fn structural_command(name: &str) -> bool {
    matches!(
        name,
        "frac"
            | "dfrac"
            | "tfrac"
            | "cfrac"
            | "binom"
            | "dbinom"
            | "tbinom"
            | "sqrt"
            | "text"
            | "mbox"
            | "textrm"
            | "textbf"
            | "operatorname"
            | "mathrm"
            | "mathbf"
            | "mathit"
            | "boldsymbol"
            | "mathbb"
            | "mathcal"
            | "mathsf"
            | "overline"
            | "underline"
            | "overrightarrow"
            | "overleftarrow"
            | "overbrace"
            | "hat"
            | "bar"
            | "vec"
            | "dot"
            | "ddot"
            | "tilde"
            | "widehat"
            | "widetilde"
            | "left"
            | "right"
            | "begin"
            | "end"
            | "quad"
            | "qquad"
            | "limits"
            | "nolimits"
            | "displaystyle"
            | "textstyle"
            | "scriptstyle"
            | "scriptscriptstyle"
            | "color"
            | "not"
            | "big"
            | "Big"
            | "bigg"
            | "Bigg"
            | "bigl"
            | "bigr"
            | "Bigl"
            | "Bigr"
            | "biggl"
            | "biggr"
            | "Biggl"
            | "Biggr"
            | "middle"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomKind {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

struct Sym {
    text: &'static str,
    kind: AtomKind,
    limits: bool,
    roman: bool,
}

fn symbol(name: &str) -> Option<Sym> {
    let sym = |text, kind, limits, roman| Sym {
        text,
        kind,
        limits,
        roman,
    };
    Some(match name {
        "alpha" => sym("α", AtomKind::Ord, false, false),
        "beta" => sym("β", AtomKind::Ord, false, false),
        "gamma" => sym("γ", AtomKind::Ord, false, false),
        "delta" => sym("δ", AtomKind::Ord, false, false),
        "epsilon" => sym("ϵ", AtomKind::Ord, false, false),
        "varepsilon" => sym("ε", AtomKind::Ord, false, false),
        "zeta" => sym("ζ", AtomKind::Ord, false, false),
        "eta" => sym("η", AtomKind::Ord, false, false),
        "theta" => sym("θ", AtomKind::Ord, false, false),
        "vartheta" => sym("ϑ", AtomKind::Ord, false, false),
        "iota" => sym("ι", AtomKind::Ord, false, false),
        "kappa" => sym("κ", AtomKind::Ord, false, false),
        "lambda" => sym("λ", AtomKind::Ord, false, false),
        "mu" => sym("μ", AtomKind::Ord, false, false),
        "nu" => sym("ν", AtomKind::Ord, false, false),
        "xi" => sym("ξ", AtomKind::Ord, false, false),
        "pi" => sym("π", AtomKind::Ord, false, false),
        "varpi" => sym("ϖ", AtomKind::Ord, false, false),
        "rho" => sym("ρ", AtomKind::Ord, false, false),
        "varrho" => sym("ϱ", AtomKind::Ord, false, false),
        "sigma" => sym("σ", AtomKind::Ord, false, false),
        "varsigma" => sym("ς", AtomKind::Ord, false, false),
        "tau" => sym("τ", AtomKind::Ord, false, false),
        "upsilon" => sym("υ", AtomKind::Ord, false, false),
        "phi" => sym("φ", AtomKind::Ord, false, false),
        "varphi" => sym("ϕ", AtomKind::Ord, false, false),
        "chi" => sym("χ", AtomKind::Ord, false, false),
        "psi" => sym("ψ", AtomKind::Ord, false, false),
        "omega" => sym("ω", AtomKind::Ord, false, false),
        "Gamma" => sym("Γ", AtomKind::Ord, false, true),
        "Delta" => sym("Δ", AtomKind::Ord, false, true),
        "Theta" => sym("Θ", AtomKind::Ord, false, true),
        "Lambda" => sym("Λ", AtomKind::Ord, false, true),
        "Xi" => sym("Ξ", AtomKind::Ord, false, true),
        "Pi" => sym("Π", AtomKind::Ord, false, true),
        "Sigma" => sym("Σ", AtomKind::Ord, false, true),
        "Upsilon" => sym("Υ", AtomKind::Ord, false, true),
        "Phi" => sym("Φ", AtomKind::Ord, false, true),
        "Psi" => sym("Ψ", AtomKind::Ord, false, true),
        "Omega" => sym("Ω", AtomKind::Ord, false, true),
        "cdot" | "cdotp" => sym("⋅", AtomKind::Bin, false, true),
        "times" => sym("×", AtomKind::Bin, false, true),
        "div" => sym("÷", AtomKind::Bin, false, true),
        "pm" => sym("±", AtomKind::Bin, false, true),
        "mp" => sym("∓", AtomKind::Bin, false, true),
        "ast" => sym("∗", AtomKind::Bin, false, true),
        "star" => sym("⋆", AtomKind::Bin, false, true),
        "circ" => sym("∘", AtomKind::Bin, false, true),
        "bullet" => sym("•", AtomKind::Bin, false, true),
        "oplus" => sym("⊕", AtomKind::Bin, false, true),
        "ominus" => sym("⊖", AtomKind::Bin, false, true),
        "otimes" => sym("⊗", AtomKind::Bin, false, true),
        "odot" => sym("⊙", AtomKind::Bin, false, true),
        "cap" => sym("∩", AtomKind::Bin, false, true),
        "cup" => sym("∪", AtomKind::Bin, false, true),
        "wedge" | "land" => sym("∧", AtomKind::Bin, false, true),
        "vee" | "lor" => sym("∨", AtomKind::Bin, false, true),
        "setminus" => sym("∖", AtomKind::Bin, false, true),
        "leq" | "le" => sym("≤", AtomKind::Rel, false, true),
        "geq" | "ge" => sym("≥", AtomKind::Rel, false, true),
        "neq" | "ne" => sym("≠", AtomKind::Rel, false, true),
        "approx" => sym("≈", AtomKind::Rel, false, true),
        "equiv" => sym("≡", AtomKind::Rel, false, true),
        "sim" => sym("∼", AtomKind::Rel, false, true),
        "simeq" => sym("≃", AtomKind::Rel, false, true),
        "cong" => sym("≅", AtomKind::Rel, false, true),
        "propto" => sym("∝", AtomKind::Rel, false, true),
        "ll" => sym("≪", AtomKind::Rel, false, true),
        "gg" => sym("≫", AtomKind::Rel, false, true),
        "subset" => sym("⊂", AtomKind::Rel, false, true),
        "supset" => sym("⊃", AtomKind::Rel, false, true),
        "subseteq" => sym("⊆", AtomKind::Rel, false, true),
        "supseteq" => sym("⊇", AtomKind::Rel, false, true),
        "in" => sym("∈", AtomKind::Rel, false, true),
        "notin" => sym("∉", AtomKind::Rel, false, true),
        "to" | "rightarrow" => sym("→", AtomKind::Rel, false, true),
        "leftarrow" => sym("←", AtomKind::Rel, false, true),
        "leftrightarrow" => sym("↔", AtomKind::Rel, false, true),
        "Rightarrow" | "implies" => sym("⇒", AtomKind::Rel, false, true),
        "Leftarrow" => sym("⇐", AtomKind::Rel, false, true),
        "Leftrightarrow" | "iff" => sym("⇔", AtomKind::Rel, false, true),
        "mapsto" => sym("↦", AtomKind::Rel, false, true),
        "perp" => sym("⊥", AtomKind::Rel, false, true),
        "parallel" => sym("∥", AtomKind::Rel, false, true),
        "mid" => sym("∣", AtomKind::Rel, false, true),
        "infty" => sym("∞", AtomKind::Ord, false, true),
        "partial" => sym("∂", AtomKind::Ord, false, true),
        "nabla" => sym("∇", AtomKind::Ord, false, true),
        "forall" => sym("∀", AtomKind::Ord, false, true),
        "exists" => sym("∃", AtomKind::Ord, false, true),
        "emptyset" | "varnothing" => sym("∅", AtomKind::Ord, false, true),
        "therefore" => sym("∴", AtomKind::Rel, false, true),
        "because" => sym("∵", AtomKind::Rel, false, true),
        "angle" => sym("∠", AtomKind::Ord, false, true),
        "triangle" => sym("△", AtomKind::Ord, false, true),
        "degree" => sym("°", AtomKind::Ord, false, true),
        "prime" => sym("′", AtomKind::Ord, false, true),
        "ldots" | "dots" => sym("…", AtomKind::Ord, false, true),
        "cdots" => sym("⋯", AtomKind::Ord, false, true),
        "vdots" => sym("⋮", AtomKind::Ord, false, true),
        "ddots" => sym("⋱", AtomKind::Ord, false, true),
        "hbar" => sym("ℏ", AtomKind::Ord, false, true),
        "ell" => sym("ℓ", AtomKind::Ord, false, true),
        "Re" => sym("ℜ", AtomKind::Ord, false, true),
        "Im" => sym("ℑ", AtomKind::Ord, false, true),
        "aleph" => sym("ℵ", AtomKind::Ord, false, true),
        "wp" => sym("℘", AtomKind::Ord, false, true),
        "neg" | "lnot" => sym("¬", AtomKind::Ord, false, true),
        "surd" => sym("√", AtomKind::Ord, false, true),
        "langle" => sym("⟨", AtomKind::Open, false, true),
        "rangle" => sym("⟩", AtomKind::Close, false, true),
        "lfloor" => sym("⌊", AtomKind::Open, false, true),
        "rfloor" => sym("⌋", AtomKind::Close, false, true),
        "lceil" => sym("⌈", AtomKind::Open, false, true),
        "rceil" => sym("⌉", AtomKind::Close, false, true),
        "vert" => sym("|", AtomKind::Ord, false, true),
        "Vert" => sym("‖", AtomKind::Ord, false, true),
        "sum" => sym("∑", AtomKind::Op, true, true),
        "prod" => sym("∏", AtomKind::Op, true, true),
        "coprod" => sym("∐", AtomKind::Op, true, true),
        "int" => sym("∫", AtomKind::Op, false, true),
        "iint" => sym("∫∫", AtomKind::Op, false, true),
        "iiint" => sym("∫∫∫", AtomKind::Op, false, true),
        "oint" => sym("∮", AtomKind::Op, false, true),
        "bigcup" => sym("⋃", AtomKind::Op, true, true),
        "bigcap" => sym("⋂", AtomKind::Op, true, true),
        "bigoplus" => sym("⨁", AtomKind::Op, true, true),
        "sin" => sym("sin", AtomKind::Op, false, true),
        "cos" => sym("cos", AtomKind::Op, false, true),
        "tan" => sym("tan", AtomKind::Op, false, true),
        "cot" => sym("cot", AtomKind::Op, false, true),
        "sec" => sym("sec", AtomKind::Op, false, true),
        "csc" => sym("csc", AtomKind::Op, false, true),
        "arcsin" => sym("arcsin", AtomKind::Op, false, true),
        "arccos" => sym("arccos", AtomKind::Op, false, true),
        "arctan" => sym("arctan", AtomKind::Op, false, true),
        "sinh" => sym("sinh", AtomKind::Op, false, true),
        "cosh" => sym("cosh", AtomKind::Op, false, true),
        "tanh" => sym("tanh", AtomKind::Op, false, true),
        "log" => sym("log", AtomKind::Op, false, true),
        "ln" => sym("ln", AtomKind::Op, false, true),
        "lg" => sym("lg", AtomKind::Op, false, true),
        "exp" => sym("exp", AtomKind::Op, false, true),
        "det" => sym("det", AtomKind::Op, false, true),
        "dim" => sym("dim", AtomKind::Op, false, true),
        "arg" => sym("arg", AtomKind::Op, false, true),
        "gcd" => sym("gcd", AtomKind::Op, false, true),
        "deg" => sym("deg", AtomKind::Op, false, true),
        "lim" => sym("lim", AtomKind::Op, true, true),
        "limsup" => sym("lim sup", AtomKind::Op, true, true),
        "liminf" => sym("lim inf", AtomKind::Op, true, true),
        "sup" => sym("sup", AtomKind::Op, true, true),
        "inf" => sym("inf", AtomKind::Op, true, true),
        "max" => sym("max", AtomKind::Op, true, true),
        "min" => sym("min", AtomKind::Op, true, true),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DelimKind {
    None,
    Paren,
    Bracket,
    Brace,
    Bar,
    DoubleBar,
    Angle,
    Floor,
    Ceil,
}

#[derive(Clone, Copy, Debug)]
enum AccentKind {
    Hat,
    Bar,
    Vec,
    Dot,
    Ddot,
    Tilde,
    Arrow,
}

#[derive(Clone, Debug)]
enum Node {
    Empty,
    Chars {
        text: String,
        face: Face,
        kind: AtomKind,
        limits: bool,
    },
    Space(f32),
    Frac {
        num: Box<Node>,
        den: Box<Node>,
    },
    Sqrt {
        index: Option<Box<Node>>,
        body: Box<Node>,
    },
    Script {
        base: Box<Node>,
        sup: Option<Box<Node>>,
        sub: Option<Box<Node>>,
    },
    Row(Vec<Node>),
    Lines(Vec<Node>),
    Matrix {
        left: DelimKind,
        right: DelimKind,
        rows: Vec<Vec<Node>>,
    },
    Delimited {
        left: DelimKind,
        right: DelimKind,
        body: Box<Node>,
    },
    Accent {
        body: Box<Node>,
        accent: AccentKind,
    },
    Over {
        body: Box<Node>,
        over: bool,
    },
}

#[derive(Clone, Copy, Debug)]
struct Style {
    letters: Face,
    text_mode: bool,
}

impl Style {
    fn math() -> Self {
        Self {
            letters: Face::Italic,
            text_mode: false,
        }
    }

    fn roman() -> Self {
        Self {
            letters: Face::Roman,
            text_mode: false,
        }
    }

    fn bold() -> Self {
        Self {
            letters: Face::Bold,
            text_mode: false,
        }
    }

    fn bold_italic() -> Self {
        Self {
            letters: Face::BoldItalic,
            text_mode: false,
        }
    }

    fn sans_text(bold: bool) -> Self {
        Self {
            letters: if bold { Face::SansBold } else { Face::Sans },
            text_mode: true,
        }
    }
}

struct Parser<'a> {
    chars: &'a [char],
    index: usize,
    style: Style,
}

fn parse_math(input: &str) -> Node {
    let chars: Vec<char> = input.chars().collect();
    let mut parser = Parser {
        chars: &chars,
        index: 0,
        style: Style::math(),
    };
    parser.parse_lines()
}

fn parse_display(input: &str) -> Node {
    let mut normalized = String::new();
    for ch in input.chars() {
        if ch == '\n' {
            normalized.push_str(" \\\\ ");
        } else {
            normalized.push(ch);
        }
    }
    parse_math(&normalized)
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += 1;
        Some(ch)
    }

    fn skip_spaces(&mut self) {
        if self.style.text_mode {
            return;
        }
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    fn at_line_break(&self) -> bool {
        self.peek() == Some('\\') && self.peek_next() == Some('\\')
    }

    fn at_right(&self) -> bool {
        self.at_named("right")
    }

    fn at_named(&self, name: &str) -> bool {
        if self.peek() != Some('\\') {
            return false;
        }
        let rest: String = self.chars[self.index + 1..].iter().collect();
        rest.starts_with(name)
            && rest
                .chars()
                .nth(name.chars().count())
                .is_none_or(|ch| !ch.is_ascii_alphabetic())
    }

    fn parse_lines(&mut self) -> Node {
        let mut lines = Vec::new();
        loop {
            lines.push(self.parse_row());
            if self.consume_line_break() {
                continue;
            }
            break;
        }
        let mut lines: Vec<Node> = lines
            .into_iter()
            .filter(|node| !node_is_blank(node))
            .collect();
        match lines.len() {
            0 => Node::Empty,
            1 => lines.pop().unwrap(),
            _ => Node::Lines(lines),
        }
    }

    fn consume_line_break(&mut self) -> bool {
        if !self.at_line_break() {
            return false;
        }
        self.bump();
        self.bump();
        self.skip_spaces();
        if self.peek() == Some('[') {
            while self.peek().is_some() && self.peek() != Some(']') {
                self.bump();
            }
            if self.peek() == Some(']') {
                self.bump();
            }
        }
        true
    }

    fn parse_row(&mut self) -> Node {
        let mut atoms = Vec::new();
        loop {
            self.skip_spaces();
            if self.peek().is_none()
                || self.peek() == Some('}')
                || self.at_line_break()
                || self.at_right()
            {
                break;
            }
            let atom = self.parse_atom();
            push_atom(&mut atoms, atom);
        }
        Node::Row(atoms)
    }

    fn parse_atom(&mut self) -> Node {
        let nucleus = self.parse_nucleus();
        self.attach_scripts(nucleus)
    }

    fn parse_nucleus(&mut self) -> Node {
        self.skip_spaces();
        match self.peek() {
            None | Some('}') => Node::Empty,
            Some('{') => self.parse_group(),
            Some('\\') => {
                self.bump();
                self.parse_command()
            }
            Some('&') => {
                self.bump();
                Node::Space(0.9)
            }
            Some('~') => {
                self.bump();
                Node::Space(0.33)
            }
            Some(_) => self.parse_char_run(),
        }
    }

    fn parse_group(&mut self) -> Node {
        if self.peek() == Some('{') {
            self.bump();
        }
        let node = self.parse_lines();
        if self.peek() == Some('}') {
            self.bump();
        }
        node
    }

    fn parse_script(&mut self) -> Node {
        self.skip_spaces();
        if self.peek() == Some('{') {
            self.parse_group()
        } else {
            self.parse_nucleus()
        }
    }

    fn attach_scripts(&mut self, base: Node) -> Node {
        if matches!(base, Node::Empty) {
            return base;
        }
        let mut sup = None;
        let mut sub = None;
        let mut primes = String::new();
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('^') if sup.is_none() => {
                    self.bump();
                    sup = Some(self.parse_script());
                }
                Some('_') if sub.is_none() => {
                    self.bump();
                    sub = Some(self.parse_script());
                }
                Some('\'' | '′') => {
                    self.bump();
                    primes.push('′');
                }
                _ => break,
            }
        }
        if !primes.is_empty() {
            let prime = Node::Chars {
                text: primes,
                face: Face::Roman,
                kind: AtomKind::Ord,
                limits: false,
            };
            sup = Some(match sup {
                Some(existing) => Node::Row(vec![prime, existing]),
                None => prime,
            });
        }
        if sup.is_none() && sub.is_none() {
            base
        } else {
            Node::Script {
                base: Box::new(base),
                sup: sup.map(Box::new),
                sub: sub.map(Box::new),
            }
        }
    }

    fn parse_char_run(&mut self) -> Node {
        let Some(ch) = self.bump() else {
            return Node::Empty;
        };
        if ch.is_ascii_digit() {
            let mut text = String::from(ch);
            loop {
                match (self.peek(), self.peek_next()) {
                    (Some(digit), _) if digit.is_ascii_digit() => {
                        text.push(self.bump().unwrap());
                    }
                    (Some('.' | ','), Some(digit)) if digit.is_ascii_digit() => {
                        text.push(self.bump().unwrap());
                    }
                    _ => break,
                }
            }
            return Node::Chars {
                text,
                face: digit_face(self.style),
                kind: AtomKind::Ord,
                limits: false,
            };
        }
        if ch.is_alphabetic() {
            return Node::Chars {
                text: ch.to_string(),
                face: letter_face(self.style),
                kind: AtomKind::Ord,
                limits: false,
            };
        }
        Node::Chars {
            text: ch.to_string(),
            face: Face::Roman,
            kind: kind_of_char(ch),
            limits: false,
        }
    }

    fn parse_command(&mut self) -> Node {
        if !self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
            return self.parse_control_symbol();
        }
        let name = self.read_name();
        let base = name.trim_end_matches('*').to_owned();
        match base.as_str() {
            "frac" | "dfrac" | "cfrac" | "tfrac" => {
                let num = self.parse_arg();
                let den = self.parse_arg();
                Node::Frac {
                    num: Box::new(num),
                    den: Box::new(den),
                }
            }
            "binom" | "dbinom" | "tbinom" => {
                let top = self.parse_arg();
                let bottom = self.parse_arg();
                Node::Matrix {
                    left: DelimKind::Paren,
                    right: DelimKind::Paren,
                    rows: vec![vec![top], vec![bottom]],
                }
            }
            "sqrt" => {
                self.skip_spaces();
                let index = if self.peek() == Some('[') {
                    self.bump();
                    let mut body = String::new();
                    while self.peek().is_some() && self.peek() != Some(']') {
                        body.push(self.bump().unwrap());
                    }
                    if self.peek() == Some(']') {
                        self.bump();
                    }
                    Some(Box::new(parse_math(&body)))
                } else {
                    None
                };
                Node::Sqrt {
                    index,
                    body: Box::new(self.parse_arg()),
                }
            }
            "text" | "mbox" | "textrm" | "operatorname" => self.parse_text(false),
            "textbf" => self.parse_text(true),
            "mathrm" => self.parse_styled(Style::roman()),
            "mathbf" | "boldsymbol" => self.parse_styled(if base == "mathbf" {
                Style::bold()
            } else {
                Style::bold_italic()
            }),
            "mathit" => self.parse_styled(Style::math()),
            "mathbb" | "mathcal" | "mathsf" => self.parse_styled(if base == "mathsf" {
                Style {
                    letters: Face::Sans,
                    text_mode: false,
                }
            } else {
                Style::roman()
            }),
            "overline" | "overbrace" => Node::Over {
                body: Box::new(self.parse_arg()),
                over: true,
            },
            "underline" => Node::Over {
                body: Box::new(self.parse_arg()),
                over: false,
            },
            "overrightarrow" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Arrow,
            },
            "hat" | "widehat" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Hat,
            },
            "bar" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Bar,
            },
            "vec" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Vec,
            },
            "dot" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Dot,
            },
            "ddot" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Ddot,
            },
            "tilde" | "widetilde" => Node::Accent {
                body: Box::new(self.parse_arg()),
                accent: AccentKind::Tilde,
            },
            "left" => {
                let left = self.parse_delim();
                let body = self.parse_row();
                let right = if self.at_right() {
                    self.bump();
                    self.read_name();
                    self.parse_delim()
                } else {
                    DelimKind::None
                };
                Node::Delimited {
                    left,
                    right,
                    body: Box::new(body),
                }
            }
            "right" => {
                let _ = self.parse_delim();
                Node::Empty
            }
            "begin" => self.parse_begin(),
            "end" => {
                let _ = self.read_braced_text();
                Node::Empty
            }
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "limits" | "nolimits" | "displaystyle" | "textstyle" | "scriptstyle"
            | "scriptscriptstyle" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl"
            | "Bigr" | "biggl" | "biggr" | "Biggl" | "Biggr" | "middle" => Node::Empty,
            "color" => {
                let _ = self.parse_arg();
                Node::Empty
            }
            "not" => self.parse_not(),
            _ => {
                if let Some(sym) = symbol(&base) {
                    self.symbol_node(sym)
                } else {
                    Node::Chars {
                        text: base,
                        face: Face::Roman,
                        kind: AtomKind::Ord,
                        limits: false,
                    }
                }
            }
        }
    }

    fn parse_control_symbol(&mut self) -> Node {
        match self.bump() {
            Some(',') => Node::Space(0.17),
            Some(':') => Node::Space(0.22),
            Some(';') => Node::Space(0.28),
            Some('!') => Node::Space(-0.12),
            Some(' ' | '\n') => Node::Space(0.33),
            Some('|') => Node::Chars {
                text: "‖".to_owned(),
                face: Face::Roman,
                kind: AtomKind::Ord,
                limits: false,
            },
            Some(ch @ ('{' | '}' | '$' | '%' | '&' | '#' | '_' | '^')) => Node::Chars {
                text: ch.to_string(),
                face: Face::Roman,
                kind: AtomKind::Ord,
                limits: false,
            },
            Some(ch) => Node::Chars {
                text: ch.to_string(),
                face: Face::Roman,
                kind: kind_of_char(ch),
                limits: false,
            },
            None => Node::Empty,
        }
    }

    fn parse_not(&mut self) -> Node {
        let next = self.parse_nucleus();
        if let Node::Chars { text, .. } = &next {
            let mapped = match text.as_str() {
                "=" => Some("≠"),
                "<" => Some("≮"),
                ">" => Some("≯"),
                "∈" => Some("∉"),
                _ => None,
            };
            if let Some(text) = mapped {
                return Node::Chars {
                    text: text.to_owned(),
                    face: Face::Roman,
                    kind: AtomKind::Rel,
                    limits: false,
                };
            }
        }
        Node::Row(vec![
            Node::Chars {
                text: "/".to_owned(),
                face: Face::Roman,
                kind: AtomKind::Ord,
                limits: false,
            },
            next,
        ])
    }

    fn parse_text(&mut self, bold: bool) -> Node {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return Node::Empty;
        }
        let saved = self.style;
        self.style = Style::sans_text(bold);
        let text = self.read_text_group();
        self.style = saved;
        if text.is_empty() {
            Node::Empty
        } else {
            Node::Chars {
                text,
                face: if bold { Face::SansBold } else { Face::Sans },
                kind: AtomKind::Ord,
                limits: false,
            }
        }
    }

    fn read_text_group(&mut self) -> String {
        if self.peek() == Some('{') {
            self.bump();
        }
        let mut out = String::new();
        let mut depth = 1_i32;
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                self.bump();
                if !self.peek().is_some_and(|next| next.is_ascii_alphabetic()) {
                    if let Some(next) = self.bump() {
                        if next == ' ' {
                            out.push(' ');
                        } else {
                            out.push(next);
                        }
                    }
                    continue;
                }
                let name = self.read_name();
                if let Some(sym) = symbol(name.trim_end_matches('*')) {
                    out.push_str(sym.text);
                }
                continue;
            }
            self.bump();
            if ch == '{' {
                depth += 1;
                continue;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            if ch == '~' {
                out.push(' ');
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn parse_styled(&mut self, style: Style) -> Node {
        let saved = self.style;
        self.style = style;
        let node = self.parse_arg();
        self.style = saved;
        node
    }

    fn parse_arg(&mut self) -> Node {
        self.skip_spaces();
        if self.peek() == Some('{') {
            self.parse_group()
        } else {
            self.parse_nucleus()
        }
    }

    fn parse_delim(&mut self) -> DelimKind {
        self.skip_spaces();
        match self.peek() {
            Some('\\') => {
                self.bump();
                if self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
                    let name = self.read_name();
                    delim_from_name(name.trim_end_matches('*'))
                } else {
                    delim_from_char(self.bump().unwrap_or('.'))
                }
            }
            Some(ch) => {
                self.bump();
                delim_from_char(ch)
            }
            None => DelimKind::None,
        }
    }

    fn parse_begin(&mut self) -> Node {
        let name = self.read_braced_text();
        let base = name.trim().to_owned();
        if base == "array" {
            self.skip_spaces();
            if self.peek() == Some('{') {
                let _ = self.read_braced_text();
            }
        }
        let body = self.read_until_end(&base);
        match base.as_str() {
            "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "cases"
            | "array" => {
                let (left, right) = matrix_delims(&base);
                Node::Matrix {
                    left,
                    right,
                    rows: split_matrix(&body),
                }
            }
            _ => {
                let rows: Vec<Node> = split_rows(&body)
                    .into_iter()
                    .map(|row| parse_math(row.trim()))
                    .filter(|node| !node_is_blank(node))
                    .collect();
                match rows.len() {
                    0 => Node::Empty,
                    1 => rows.into_iter().next().unwrap(),
                    _ => Node::Lines(rows),
                }
            }
        }
    }

    fn read_name(&mut self) -> String {
        let mut name = String::new();
        while self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
            name.push(self.bump().unwrap());
        }
        if self.peek() == Some('*') {
            name.push(self.bump().unwrap());
        }
        name
    }

    fn read_braced_text(&mut self) -> String {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return String::new();
        }
        self.bump();
        let mut text = String::new();
        let mut depth = 1_i32;
        while let Some(ch) = self.bump() {
            if ch == '{' {
                depth += 1;
                text.push(ch);
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                text.push(ch);
            } else {
                text.push(ch);
            }
        }
        text
    }

    fn read_until_end(&mut self, name: &str) -> String {
        let token: Vec<char> = format!("\\end{{{name}}}").chars().collect();
        if let Some(pos) = find_pattern(&self.chars[self.index..], &token) {
            let body: String = self.chars[self.index..self.index + pos].iter().collect();
            self.index += pos + token.len();
            body
        } else {
            let body: String = self.chars[self.index..].iter().collect();
            self.index = self.chars.len();
            body
        }
    }

    fn symbol_node(&self, sym: Sym) -> Node {
        let face = if sym.roman {
            match self.style.letters {
                Face::Bold | Face::BoldItalic => Face::Bold,
                _ => Face::Roman,
            }
        } else {
            self.style.letters
        };
        Node::Chars {
            text: sym.text.to_owned(),
            face,
            kind: sym.kind,
            limits: sym.limits,
        }
    }
}

fn push_atom(atoms: &mut Vec<Node>, atom: Node) {
    if node_is_blank(&atom) && !matches!(atom, Node::Space(_)) {
        return;
    }
    if let Node::Chars {
        text,
        face,
        kind,
        limits: false,
    } = &atom
        && let Some(Node::Chars {
            text: prev,
            face: prev_face,
            kind: prev_kind,
            limits: false,
        }) = atoms.last_mut()
        && *prev_face == *face
        && *prev_kind == *kind
        && *kind == AtomKind::Ord
    {
        prev.push_str(text);
        return;
    }
    atoms.push(atom);
}

fn node_is_blank(node: &Node) -> bool {
    match node {
        Node::Empty => true,
        Node::Row(items) => items.iter().all(node_is_blank),
        Node::Lines(items) => items.iter().all(node_is_blank),
        _ => false,
    }
}

fn digit_face(style: Style) -> Face {
    if style.text_mode {
        return if matches!(style.letters, Face::SansBold) {
            Face::SansBold
        } else {
            Face::Sans
        };
    }
    if matches!(style.letters, Face::Bold | Face::BoldItalic) {
        Face::Bold
    } else {
        Face::Roman
    }
}

fn letter_face(style: Style) -> Face {
    style.letters
}

fn kind_of_char(ch: char) -> AtomKind {
    match ch {
        '+' | '-' | '*' | '/' | '×' | '·' | '⋅' => AtomKind::Bin,
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '≡' => AtomKind::Rel,
        ',' | ';' | ':' => AtomKind::Punct,
        '(' | '[' => AtomKind::Open,
        ')' | ']' => AtomKind::Close,
        _ => AtomKind::Ord,
    }
}

fn delim_from_char(ch: char) -> DelimKind {
    match ch {
        '(' | ')' => DelimKind::Paren,
        '[' | ']' => DelimKind::Bracket,
        '{' | '}' | '\\' => DelimKind::Brace,
        '|' => DelimKind::Bar,
        '<' | '>' => DelimKind::Angle,
        '.' => DelimKind::None,
        _ => DelimKind::None,
    }
}

fn delim_from_name(name: &str) -> DelimKind {
    match name {
        "lbrace" | "rbrace" | "{" | "}" => DelimKind::Brace,
        "lbrack" | "rbrack" => DelimKind::Bracket,
        "langle" | "rangle" => DelimKind::Angle,
        "lvert" | "rvert" | "vert" => DelimKind::Bar,
        "lVert" | "rVert" | "Vert" => DelimKind::DoubleBar,
        "lfloor" | "rfloor" => DelimKind::Floor,
        "lceil" | "rceil" => DelimKind::Ceil,
        _ => DelimKind::None,
    }
}

fn matrix_delims(name: &str) -> (DelimKind, DelimKind) {
    match name {
        "pmatrix" => (DelimKind::Paren, DelimKind::Paren),
        "bmatrix" => (DelimKind::Bracket, DelimKind::Bracket),
        "Bmatrix" | "cases" => (DelimKind::Brace, DelimKind::None),
        "vmatrix" => (DelimKind::Bar, DelimKind::Bar),
        "Vmatrix" => (DelimKind::DoubleBar, DelimKind::DoubleBar),
        _ => (DelimKind::None, DelimKind::None),
    }
}

fn split_rows(body: &str) -> Vec<String> {
    split_latex(body, true)
}

fn split_cells(body: &str) -> Vec<String> {
    split_latex(body, false)
}

fn split_latex(body: &str, rows: bool) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\\' && index + 1 < chars.len() && depth == 0 {
            if rows && chars[index + 1] == '\\' {
                parts.push(std::mem::take(&mut current));
                index += 2;
                continue;
            }
            if !rows && chars[index + 1] != '&' {
                current.push(chars[index]);
                index += 1;
                continue;
            }
        }
        if !rows && chars[index] == '&' && depth == 0 {
            parts.push(std::mem::take(&mut current));
            index += 1;
            continue;
        }
        if chars[index] == '\\' && index + 1 < chars.len() {
            current.push(chars[index]);
            current.push(chars[index + 1]);
            index += 2;
            continue;
        }
        if chars[index] == '{' {
            depth += 1;
        } else if chars[index] == '}' {
            depth -= 1;
        }
        current.push(chars[index]);
        index += 1;
    }
    parts.push(current);
    parts
}

fn split_matrix(body: &str) -> Vec<Vec<Node>> {
    split_rows(body)
        .into_iter()
        .map(|row| {
            split_cells(&row)
                .into_iter()
                .map(|cell| parse_math(cell.trim()))
                .collect::<Vec<_>>()
        })
        .filter(|row: &Vec<Node>| row.iter().any(|node| !node_is_blank(node)))
        .collect()
}

struct BoxLayout {
    width: f32,
    ascent: f32,
    descent: f32,
    ops: Vec<Op>,
}

fn layout_visual_line(
    line: &VLine,
    font_size: f32,
    style: &NoteStyle<'_>,
    measure: &dyn Measurer,
) -> BoxLayout {
    if line.pieces.is_empty() {
        return BoxLayout {
            width: 0.0,
            ascent: font_size * 0.35,
            descent: font_size * 0.35,
            ops: Vec::new(),
        };
    }
    let mut pieces = Vec::new();
    if !style.prefix.is_empty() {
        pieces.push(layout_prose(
            style.prefix,
            font_size,
            style.bold,
            style.italic,
            measure,
        ));
    }
    for piece in &line.pieces {
        match piece {
            Piece::Prose(text) => {
                if text.is_empty() {
                    continue;
                }
                pieces.push(layout_prose(
                    text,
                    font_size,
                    style.bold,
                    style.italic,
                    measure,
                ));
            }
            Piece::Math { latex, display } => {
                let node = if *display {
                    parse_display(latex)
                } else {
                    parse_math(latex)
                };
                pieces.push(layout_node(&node, font_size, *display, measure));
            }
        }
    }
    concat_boxes(pieces, 0.0)
}

fn layout_prose(
    text: &str,
    size: f32,
    bold: bool,
    italic: bool,
    measure: &dyn Measurer,
) -> BoxLayout {
    let face = match (bold, italic) {
        (true, true) => Face::SansBold,
        (true, false) => Face::SansBold,
        (false, true) => Face::SansItalic,
        (false, false) => Face::Sans,
    };
    let width = measure.measure(text, size, face).max(0.0);
    BoxLayout {
        width,
        ascent: size,
        descent: size * 0.25,
        ops: vec![Op::Glyph {
            x: 0.0,
            y: 0.0,
            size,
            text: text.to_owned(),
            face,
        }],
    }
}

fn concat_boxes(boxes: Vec<BoxLayout>, gap: f32) -> BoxLayout {
    let mut x = 0.0;
    let mut ascent = 0.0_f32;
    let mut descent = 0.0_f32;
    let mut ops = Vec::new();
    for (index, child) in boxes.into_iter().enumerate() {
        if index > 0 {
            x += gap;
        }
        ops.extend(shift_ops(child.ops, x, 0.0));
        ascent = ascent.max(child.ascent);
        descent = descent.max(child.descent);
        x += child.width;
    }
    BoxLayout {
        width: x.max(0.0),
        ascent,
        descent,
        ops,
    }
}

fn shift_ops(ops: Vec<Op>, dx: f32, dy: f32) -> Vec<Op> {
    ops.into_iter()
        .map(|op| match op {
            Op::Glyph {
                x,
                y,
                size,
                text,
                face,
            } => Op::Glyph {
                x: x + dx,
                y: y + dy,
                size,
                text,
                face,
            },
            Op::Rule {
                x,
                y,
                width,
                weight,
            } => Op::Rule {
                x: x + dx,
                y: y + dy,
                width,
                weight,
            },
            Op::Path { points, weight } => Op::Path {
                points: points.into_iter().map(|(x, y)| (x + dx, y + dy)).collect(),
                weight,
            },
        })
        .collect()
}

fn layout_node(node: &Node, size: f32, display: bool, measure: &dyn Measurer) -> BoxLayout {
    match node {
        Node::Empty => empty_box(),
        Node::Space(em) => BoxLayout {
            width: em * size,
            ascent: 0.0,
            descent: 0.0,
            ops: Vec::new(),
        },
        Node::Chars {
            text, face, limits, ..
        } => {
            let glyph_size = if *limits && display {
                size * 1.15
            } else {
                size
            };
            let width = measure
                .measure(text, glyph_size, *face)
                .max(glyph_size * 0.2);
            let (ascent, descent) =
                if matches!(face, Face::Sans | Face::SansBold | Face::SansItalic) {
                    (glyph_size, glyph_size * 0.22)
                } else if *limits {
                    (glyph_size * 0.72, glyph_size * 0.28)
                } else {
                    (glyph_size * 0.76, glyph_size * 0.22)
                };
            BoxLayout {
                width,
                ascent,
                descent,
                ops: vec![Op::Glyph {
                    x: 0.0,
                    y: 0.0,
                    size: glyph_size,
                    text: text.clone(),
                    face: *face,
                }],
            }
        }
        Node::Row(items) => layout_row(items, size, display, measure),
        Node::Lines(lines) => {
            let laid: Vec<BoxLayout> = lines
                .iter()
                .map(|line| layout_node(line, size, display, measure))
                .collect();
            stack_boxes(&laid, size * 0.35)
        }
        Node::Frac { num, den } => {
            let scale = if display { 0.92 } else { 0.68 };
            let num = layout_node(num, size * scale, false, measure);
            let den = layout_node(den, size * scale, false, measure);
            let axis = -0.22 * size;
            let thick = (size * 0.055).max(0.7);
            let gap = size * 0.12;
            let width = num.width.max(den.width) + size * 0.32;
            let num_y = axis - thick * 0.5 - gap - num.descent;
            let den_y = axis + thick * 0.5 + gap + den.ascent;
            let mut ops = shift_ops(num.ops, (width - num.width) / 2.0, num_y);
            ops.extend(shift_ops(den.ops, (width - den.width) / 2.0, den_y));
            ops.push(Op::Rule {
                x: size * 0.06,
                y: axis,
                width: (width - size * 0.12).max(size * 0.3),
                weight: thick,
            });
            BoxLayout {
                width,
                ascent: -num_y + num.ascent,
                descent: den_y + den.descent,
                ops,
            }
        }
        Node::Sqrt { index, body } => layout_sqrt(index.as_deref(), body, size, display, measure),
        Node::Script { base, sup, sub } => {
            layout_script(base, sup.as_deref(), sub.as_deref(), size, display, measure)
        }
        Node::Delimited { left, right, body } => {
            let inner = layout_node(body, size, display, measure);
            with_delims(*left, *right, inner, size, measure)
        }
        Node::Matrix { left, right, rows } => {
            let inner = layout_matrix(rows, size, measure);
            with_delims(*left, *right, inner, size, measure)
        }
        Node::Accent { body, accent } => layout_accent(body, *accent, size, display, measure),
        Node::Over { body, over } => {
            let inner = layout_node(body, size, display, measure);
            let gap = size * 0.12;
            let weight = (size * 0.05).max(0.7);
            let (y, ascent, descent) = if *over {
                let y = -(inner.ascent + gap);
                (y, inner.ascent + gap + weight, inner.descent)
            } else {
                let y = inner.descent + gap;
                (y, inner.ascent, inner.descent + gap + weight)
            };
            let mut ops = inner.ops;
            ops.push(Op::Rule {
                x: 0.0,
                y,
                width: inner.width.max(size * 0.3),
                weight,
            });
            BoxLayout {
                width: inner.width,
                ascent,
                descent,
                ops,
            }
        }
    }
}

fn empty_box() -> BoxLayout {
    BoxLayout {
        width: 0.0,
        ascent: 0.0,
        descent: 0.0,
        ops: Vec::new(),
    }
}

fn layout_row(items: &[Node], size: f32, display: bool, measure: &dyn Measurer) -> BoxLayout {
    let mut x = 0.0;
    let mut ascent = 0.0_f32;
    let mut descent = 0.0_f32;
    let mut ops = Vec::new();
    let mut previous: Option<AtomKind> = None;
    for item in items {
        if let Node::Space(em) = item {
            x += em * size;
            previous = None;
            continue;
        }
        if node_is_blank(item) {
            continue;
        }
        let kind = atom_kind(item);
        if let Some(prev) = previous {
            x += space_between(prev, kind) * size;
        }
        let child = layout_node(item, size, display, measure);
        ops.extend(shift_ops(child.ops, x, 0.0));
        ascent = ascent.max(child.ascent);
        descent = descent.max(child.descent);
        x += child.width;
        previous = Some(kind);
    }
    BoxLayout {
        width: x.max(0.0),
        ascent,
        descent,
        ops,
    }
}

fn atom_kind(node: &Node) -> AtomKind {
    match node {
        Node::Chars { kind, .. } => *kind,
        Node::Script { base, .. } => atom_kind(base),
        Node::Row(items) => items.last().map(atom_kind).unwrap_or(AtomKind::Ord),
        Node::Delimited { left, right, .. } => {
            if *left != DelimKind::None && *right == DelimKind::None {
                AtomKind::Open
            } else {
                AtomKind::Ord
            }
        }
        _ => AtomKind::Ord,
    }
}

fn space_between(left: AtomKind, right: AtomKind) -> f32 {
    use AtomKind::*;
    match (left, right) {
        (Ord, Bin) | (Bin, Ord) | (Bin, Open) | (Close, Bin) | (Bin, Bin) => 0.18,
        (Ord, Rel)
        | (Rel, Ord)
        | (Rel, Open)
        | (Close, Rel)
        | (Rel, Rel)
        | (Rel, Bin)
        | (Bin, Rel) => 0.28,
        (Punct, Ord) | (Punct, Open) | (Punct, Rel) => 0.16,
        (Close, Ord) | (Close, Open) => 0.08,
        (Op, Ord) | (Op, Open) => 0.14,
        _ => 0.0,
    }
}

fn stack_boxes(lines: &[BoxLayout], gap: f32) -> BoxLayout {
    if lines.is_empty() {
        return empty_box();
    }
    let width = lines.iter().map(|line| line.width).fold(0.0_f32, f32::max);
    let mut baseline = 0.0;
    let mut ops = Vec::new();
    for (index, child) in lines.iter().enumerate() {
        ops.extend(shift_ops(child.ops.clone(), 0.0, baseline));
        if index + 1 < lines.len() {
            baseline += child.descent + gap + lines[index + 1].ascent;
        }
    }
    BoxLayout {
        width,
        ascent: lines[0].ascent,
        descent: baseline + lines.last().map(|line| line.descent).unwrap_or(0.0),
        ops,
    }
}

fn layout_script(
    base: &Node,
    sup: Option<&Node>,
    sub: Option<&Node>,
    size: f32,
    display: bool,
    measure: &dyn Measurer,
) -> BoxLayout {
    let base_box = layout_node(base, size, display, measure);
    let script_size = (size * 0.62).max(6.0);
    let sup_box = sup.map(|node| layout_node(node, script_size, false, measure));
    let sub_box = sub.map(|node| layout_node(node, script_size, false, measure));
    let limits = display && node_limits(base);
    if limits {
        let sup_width = sup_box.as_ref().map(|child| child.width).unwrap_or(0.0);
        let sub_width = sub_box.as_ref().map(|child| child.width).unwrap_or(0.0);
        let width = base_box.width.max(sup_width).max(sub_width);
        let gap = size * 0.06;
        let mut ascent = base_box.ascent;
        let mut descent = base_box.descent;
        let mut ops = shift_ops(base_box.ops, (width - base_box.width) / 2.0, 0.0);
        if let Some(sup_box) = sup_box {
            let y = -(base_box.ascent + gap + sup_box.descent);
            ops.extend(shift_ops(sup_box.ops, (width - sup_box.width) / 2.0, y));
            ascent = -y + sup_box.ascent;
        }
        if let Some(sub_box) = sub_box {
            let y = base_box.descent + gap + sub_box.ascent;
            ops.extend(shift_ops(sub_box.ops, (width - sub_box.width) / 2.0, y));
            descent = y + sub_box.descent;
        }
        return BoxLayout {
            width,
            ascent,
            descent,
            ops,
        };
    }
    let gap = size * 0.05;
    let sup_y = -(base_box.ascent * 0.55).max(size * 0.42);
    let sub_y = (base_box.descent * 0.15) + size * 0.2;
    let script_width = sup_box
        .as_ref()
        .map(|child| child.width)
        .unwrap_or(0.0)
        .max(sub_box.as_ref().map(|child| child.width).unwrap_or(0.0));
    let mut ops = base_box.ops;
    let x = base_box.width + gap;
    let mut ascent = base_box.ascent;
    let mut descent = base_box.descent;
    if let Some(sup_box) = sup_box {
        ops.extend(shift_ops(sup_box.ops, x, sup_y));
        ascent = ascent.max(-sup_y + sup_box.ascent);
    }
    if let Some(sub_box) = sub_box {
        ops.extend(shift_ops(sub_box.ops, x, sub_y));
        descent = descent.max(sub_y + sub_box.descent);
    }
    BoxLayout {
        width: base_box.width + gap + script_width,
        ascent,
        descent,
        ops,
    }
}

fn node_limits(node: &Node) -> bool {
    match node {
        Node::Chars { limits, .. } => *limits,
        Node::Row(items) if items.len() == 1 => node_limits(&items[0]),
        _ => false,
    }
}

fn layout_sqrt(
    index: Option<&Node>,
    body: &Node,
    size: f32,
    display: bool,
    measure: &dyn Measurer,
) -> BoxLayout {
    let body = layout_node(body, size, display, measure);
    let index_box = index.map(|node| layout_node(node, (size * 0.5).max(6.0), false, measure));
    let index_width = index_box
        .as_ref()
        .map(|child| child.width * 0.85)
        .unwrap_or(0.0);
    let hook = size * 0.46;
    let pad = size * 0.1;
    let top = -(body.ascent + pad);
    let bottom = body.descent;
    let body_x = index_width + hook + size * 0.04;
    let width = body_x + body.width + pad;
    let weight = (size * 0.05).max(0.75);
    let mut ops = Vec::new();
    if let Some(index_box) = index_box {
        ops.extend(shift_ops(index_box.ops, 0.0, top + size * 0.15));
    }
    let hook_left = index_width;
    ops.push(Op::Path {
        points: vec![
            (hook_left + hook * 0.08, top + (bottom - top) * 0.58),
            (hook_left + hook * 0.42, bottom),
            (hook_left + hook, top),
        ],
        weight,
    });
    ops.push(Op::Rule {
        x: hook_left + hook,
        y: top,
        width: body.width + pad,
        weight,
    });
    ops.extend(shift_ops(body.ops, body_x, 0.0));
    BoxLayout {
        width,
        ascent: -top + size * 0.04,
        descent: bottom.max(0.0),
        ops,
    }
}

fn layout_accent(
    body: &Node,
    accent: AccentKind,
    size: f32,
    display: bool,
    measure: &dyn Measurer,
) -> BoxLayout {
    let body = layout_node(body, size, display, measure);
    let text = match accent {
        AccentKind::Hat => "^",
        AccentKind::Bar => "¯",
        AccentKind::Vec | AccentKind::Arrow => "→",
        AccentKind::Dot => "˙",
        AccentKind::Ddot => "¨",
        AccentKind::Tilde => "~",
    };
    let accent_size = size * 0.55;
    let accent_width = measure.measure(text, accent_size, Face::Roman);
    let x = ((body.width - accent_width) / 2.0).max(0.0);
    let y = -(body.ascent + accent_size * 0.35);
    let mut ops = body.ops;
    ops.push(Op::Glyph {
        x,
        y,
        size: accent_size,
        text: text.to_owned(),
        face: Face::Roman,
    });
    BoxLayout {
        width: body.width.max(x + accent_width),
        ascent: -y + accent_size * 0.7,
        descent: body.descent,
        ops,
    }
}

fn layout_matrix(rows: &[Vec<Node>], size: f32, measure: &dyn Measurer) -> BoxLayout {
    if rows.is_empty() {
        return empty_box();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut cells = Vec::new();
    for row in rows {
        let mut laid = Vec::new();
        for cell in row {
            laid.push(layout_node(cell, size, false, measure));
        }
        while laid.len() < columns {
            laid.push(empty_box());
        }
        cells.push(laid);
    }
    let mut col_width = vec![0.0_f32; columns];
    let mut row_ascent = vec![0.0_f32; rows.len()];
    let mut row_descent = vec![0.0_f32; rows.len()];
    for (row_index, row) in cells.iter().enumerate() {
        for (col_index, cell) in row.iter().enumerate() {
            col_width[col_index] = col_width[col_index].max(cell.width);
            row_ascent[row_index] = row_ascent[row_index].max(cell.ascent);
            row_descent[row_index] = row_descent[row_index].max(cell.descent);
        }
    }
    let gap_x = size * 0.7;
    let gap_y = size * 0.28;
    let grid_width = col_width.iter().sum::<f32>() + gap_x * columns.saturating_sub(1) as f32;
    let grid_height = row_ascent.iter().chain(row_descent.iter()).sum::<f32>()
        + gap_y * rows.len().saturating_sub(1) as f32;
    let axis = -0.22 * size;
    let top = axis - grid_height / 2.0;
    let mut ops = Vec::new();
    let mut y = top;
    for (row_index, row) in cells.into_iter().enumerate() {
        let baseline = y + row_ascent[row_index];
        let mut x = 0.0;
        for (col_index, cell) in row.into_iter().enumerate() {
            let cell_x = x + (col_width[col_index] - cell.width) / 2.0;
            ops.extend(shift_ops(cell.ops, cell_x, baseline));
            x += col_width[col_index] + gap_x;
        }
        y += row_ascent[row_index] + row_descent[row_index] + gap_y;
    }
    BoxLayout {
        width: grid_width.max(size * 0.4),
        ascent: -top,
        descent: top + grid_height,
        ops,
    }
}

fn with_delims(
    left: DelimKind,
    right: DelimKind,
    inner: BoxLayout,
    size: f32,
    measure: &dyn Measurer,
) -> BoxLayout {
    let content_height = inner.ascent + inner.descent;
    let stretch = content_height > size * 1.55;
    let pad = size * 0.08;
    let left_box = delim_box(left, true, &inner, size, stretch, measure);
    let right_box = delim_box(right, false, &inner, size, stretch, measure);
    let mut ops = shift_ops(left_box.ops, 0.0, 0.0);
    let inner_x = left_box.width + if left == DelimKind::None { 0.0 } else { pad };
    ops.extend(shift_ops(inner.ops, inner_x, 0.0));
    let right_x = inner_x + inner.width + if right == DelimKind::None { 0.0 } else { pad };
    ops.extend(shift_ops(right_box.ops, right_x, 0.0));
    BoxLayout {
        width: right_x + right_box.width,
        ascent: inner.ascent.max(left_box.ascent).max(right_box.ascent),
        descent: inner.descent.max(left_box.descent).max(right_box.descent),
        ops,
    }
}

fn delim_box(
    kind: DelimKind,
    left: bool,
    inner: &BoxLayout,
    size: f32,
    stretch: bool,
    measure: &dyn Measurer,
) -> BoxLayout {
    if kind == DelimKind::None {
        return empty_box();
    }
    if !stretch && let Some(glyph) = delim_glyph(kind, left) {
        let width = measure.measure(glyph, size, Face::Roman).max(size * 0.35);
        return BoxLayout {
            width,
            ascent: inner.ascent,
            descent: inner.descent,
            ops: vec![Op::Glyph {
                x: 0.0,
                y: 0.0,
                size,
                text: glyph.to_owned(),
                face: Face::Roman,
            }],
        };
    }
    let top = -(inner.ascent + size * 0.05);
    let bottom = inner.descent + size * 0.05;
    let height = (bottom - top).max(size);
    let width = (height * 0.12).clamp(size * 0.28, size * 0.62);
    let weight = (size * 0.055).max(0.8);
    BoxLayout {
        width,
        ascent: -top,
        descent: bottom,
        ops: vec![Op::Path {
            points: delim_path(kind, left, 0.0, top, bottom, width),
            weight,
        }],
    }
}

fn delim_glyph(kind: DelimKind, left: bool) -> Option<&'static str> {
    Some(match (kind, left) {
        (DelimKind::Paren, true) => "(",
        (DelimKind::Paren, false) => ")",
        (DelimKind::Bracket, true) => "[",
        (DelimKind::Bracket, false) => "]",
        (DelimKind::Brace, true) => "{",
        (DelimKind::Brace, false) => "}",
        (DelimKind::Bar, _) => "|",
        (DelimKind::DoubleBar, _) => "‖",
        (DelimKind::Angle, true) => "⟨",
        (DelimKind::Angle, false) => "⟩",
        (DelimKind::Floor, true) => "⌊",
        (DelimKind::Floor, false) => "⌋",
        (DelimKind::Ceil, true) => "⌈",
        (DelimKind::Ceil, false) => "⌉",
        (DelimKind::None, _) => return None,
    })
}

fn delim_path(
    kind: DelimKind,
    left: bool,
    x: f32,
    top: f32,
    bottom: f32,
    width: f32,
) -> Vec<(f32, f32)> {
    let mid = (top + bottom) / 2.0;
    match kind {
        DelimKind::Paren => {
            let samples = 8;
            (0..samples)
                .map(|step| {
                    let t = step as f32 / (samples - 1) as f32;
                    let y = top + (bottom - top) * t;
                    let bulge = 1.0 - (2.0 * t - 1.0).powi(2);
                    let px = if left {
                        x + width * (1.0 - bulge)
                    } else {
                        x + width * bulge
                    };
                    (px, y)
                })
                .collect()
        }
        DelimKind::Bracket => {
            if left {
                vec![(x + width, top), (x, top), (x, bottom), (x + width, bottom)]
            } else {
                vec![(x, top), (x + width, top), (x + width, bottom), (x, bottom)]
            }
        }
        DelimKind::Brace => {
            let neck = width * 0.45;
            if left {
                vec![
                    (x + width, top),
                    (x + neck, top + (mid - top) * 0.35),
                    (x + neck, mid - (bottom - top) * 0.06),
                    (x, mid),
                    (x + neck, mid + (bottom - top) * 0.06),
                    (x + neck, bottom - (bottom - mid) * 0.35),
                    (x + width, bottom),
                ]
            } else {
                vec![
                    (x, top),
                    (x + width - neck, top + (mid - top) * 0.35),
                    (x + width - neck, mid - (bottom - top) * 0.06),
                    (x + width, mid),
                    (x + width - neck, mid + (bottom - top) * 0.06),
                    (x + width - neck, bottom - (bottom - mid) * 0.35),
                    (x, bottom),
                ]
            }
        }
        DelimKind::Angle => {
            if left {
                vec![(x + width, top), (x, mid), (x + width, bottom)]
            } else {
                vec![(x, top), (x + width, mid), (x, bottom)]
            }
        }
        DelimKind::Floor => {
            if left {
                vec![(x + width, top), (x, top), (x, bottom), (x + width, bottom)]
            } else {
                vec![(x, top), (x + width, top), (x + width, bottom), (x, bottom)]
            }
        }
        DelimKind::Ceil => {
            if left {
                vec![(x, top), (x + width, top), (x, top), (x, bottom)]
            } else {
                vec![
                    (x + width, top),
                    (x, top),
                    (x + width, top),
                    (x + width, bottom),
                ]
            }
        }
        DelimKind::Bar | DelimKind::DoubleBar | DelimKind::None => {
            vec![(x + width * 0.5, top), (x + width * 0.5, bottom)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> NoteStyle<'static> {
        NoteStyle {
            font_size: size,
            bold: false,
            italic: false,
            underline: false,
            prefix: "",
        }
    }

    fn layout(text: &str) -> Layout {
        layout_note(text, style(18.0), &Estimate)
    }

    fn glyph_strings(layout: &Layout) -> Vec<String> {
        layout
            .lines
            .iter()
            .flat_map(|line| line.ops.iter())
            .filter_map(|op| match op {
                Op::Glyph { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn joined(layout: &Layout) -> String {
        glyph_strings(layout).join("")
    }

    fn glyph<'a>(layout: &'a Layout, text: &str) -> &'a Op {
        layout
            .lines
            .iter()
            .flat_map(|line| line.ops.iter())
            .find(|op| matches!(op, Op::Glyph { text: found, .. } if found == text))
            .unwrap_or_else(|| panic!("missing glyph {text}"))
    }

    #[test]
    fn currency_is_not_math_and_latex_is() {
        assert!(!has_math("the parts cost $350 and $20"));
        assert!(!has_math("plain notebook text"));
        assert!(has_math("$m = 350\\text{ kg}$"));
        assert!(has_math("$$\\rho$$"));
        assert!(has_math("\\(a^2\\)"));
        assert!(has_math("\\[a+b\\]"));
        assert!(has_math("density \\text{kg}"));
    }

    #[test]
    fn scripts_move_off_the_baseline_and_shrink() {
        let laid = layout("$x^2$");
        let Op::Glyph {
            y: y_exp,
            size: exp,
            ..
        } = glyph(&laid, "2")
        else {
            panic!("exponent");
        };
        let Op::Glyph {
            y: y_base,
            size: base,
            ..
        } = glyph(&laid, "x")
        else {
            panic!("base");
        };
        assert!(*y_exp < *y_base);
        assert!(*exp < *base);
        let laid = layout("$\\rho_0$");
        assert!(joined(&laid).contains('ρ'));
        let Op::Glyph { y: y_sub, .. } = glyph(&laid, "0") else {
            panic!("subscript");
        };
        assert!(*y_sub > 0.0);
    }

    #[test]
    fn fraction_is_taller_than_a_text_line_and_keeps_units_upright() {
        let laid = layout(
            "$$\\rho_0 = \\frac{p}{r \\cdot T} = \\frac{100,000}{287 \\cdot 293,15} = 1,1888 \\text{ kg}/\\text{m}^3$$",
        );
        assert!(laid.height > 18.0 * 1.6);
        assert!(
            laid.lines
                .iter()
                .any(|line| line.ops.iter().any(|op| matches!(op, Op::Rule { .. })))
        );
        let glyphs = joined(&laid);
        assert!(glyphs.contains('ρ'));
        assert!(glyphs.contains("100,000"));
        assert!(glyphs.contains("kg"));
        assert!(!glyphs.contains("frac"));
        assert!(!glyphs.contains('\\'));
        let kg = glyph(&laid, " kg");
        assert!(matches!(
            kg,
            Op::Glyph {
                face: Face::Sans,
                ..
            }
        ));
    }

    #[test]
    fn bare_latex_commands_are_typeset_with_the_surrounding_sentence() {
        let source =
            "Teplota: $t = 20^\\circ\\text{C}$ (implies T = 273,15 + 20 = 293,15\\text{ K}$)";
        let laid = layout(source);
        let glyphs = joined(&laid);
        assert!(glyphs.contains("Teplota"));
        assert!(glyphs.contains('∘'));
        assert!(glyphs.contains('K'));
        assert!(!glyphs.contains("text"));
        assert!(!glyphs.contains('\\'));
    }

    #[test]
    fn gemini_markdown_becomes_heading_bullet_and_display_math() {
        let source = "\
### Transkripce\n\
\n\
* Hmotnost balónu: $m = 350\\text{ kg}$\n\
\n\
$$\n\
\\rho_0 = \\frac{p}{r \\cdot T}\n\
$$\n\
\n\
Podmínka $F_v = F_g$\n";
        let blocks = pasted_blocks(source).expect("math paste");
        assert!(blocks.len() >= 4);
        assert!(blocks[0].bold);
        assert!(blocks[0].scale > 1.1);
        assert_eq!(blocks[0].text, "Transkripce");
        assert_eq!(blocks[1].list, PasteList::Bullet);
        assert!(blocks[1].text.contains("$m = 350\\text{ kg}$"));
        assert!(blocks.iter().any(|block| block.text.contains("\\frac")));
        assert!(pasted_blocks("just a sentence").is_none());
    }

    #[test]
    fn matrix_and_square_root_produce_geometry() {
        let matrix = layout("$\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}$");
        let glyphs = joined(&matrix);
        assert!(glyphs.contains('a') && glyphs.contains('d'));
        assert!(matrix.width > 18.0);
        let root = layout("$\\sqrt{x^2+1}$");
        assert!(root.lines.iter().any(|line| {
            line.ops
                .iter()
                .any(|op| matches!(op, Op::Path { .. } | Op::Rule { .. }))
        }));
        assert!(joined(&root).contains('x'));
    }
}
