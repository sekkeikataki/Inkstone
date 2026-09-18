use crate::spreadsheet::address::{CellAddr, CellRange, CellRef};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Div0,
    Na,
    Name,
    Null,
    Num,
    Ref,
    Value,
    Circ,
}

impl ErrorKind {
    pub fn as_excel(self) -> &'static str {
        match self {
            Self::Div0 => "#DIV/0!",
            Self::Na => "#N/A",
            Self::Name => "#NAME?",
            Self::Null => "#NULL!",
            Self::Num => "#NUM!",
            Self::Ref => "#REF!",
            Self::Value => "#VALUE!",
            Self::Circ => "#CIRC!",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_uppercase().as_str() {
            "#DIV/0!" => Some(Self::Div0),
            "#N/A" | "#N/A!" => Some(Self::Na),
            "#NAME?" => Some(Self::Name),
            "#NULL!" => Some(Self::Null),
            "#NUM!" => Some(Self::Num),
            "#REF!" => Some(Self::Ref),
            "#VALUE!" => Some(Self::Value),
            "#CIRC!" | "#CIRCREF!" => Some(Self::Circ),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_excel())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(ErrorKind),
    Array(Vec<Vec<Value>>),
}

impl Value {
    pub fn number(value: f64) -> Self {
        if value.is_finite() {
            Self::Number(value)
        } else if value.is_nan() {
            Self::Error(ErrorKind::Num)
        } else {
            Self::Error(ErrorKind::Div0)
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn error(&self) -> Option<ErrorKind> {
        match self {
            Self::Error(kind) => Some(*kind),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Result<f64, ErrorKind> {
        match self {
            Self::Empty => Ok(0.0),
            Self::Number(value) => Ok(*value),
            Self::Bool(true) => Ok(1.0),
            Self::Bool(false) => Ok(0.0),
            Self::Text(text) => parse_number_like(text).ok_or(ErrorKind::Value),
            Self::Error(kind) => Err(*kind),
            Self::Array(values) => scalar_from_array(values)?.as_number(),
        }
    }

    pub fn as_text(&self) -> Result<String, ErrorKind> {
        match self {
            Self::Empty => Ok(String::new()),
            Self::Number(value) => Ok(format_general(*value)),
            Self::Bool(value) => Ok(if *value { "TRUE" } else { "FALSE" }.to_owned()),
            Self::Text(text) => Ok(text.clone()),
            Self::Error(kind) => Err(*kind),
            Self::Array(values) => scalar_from_array(values)?.as_text(),
        }
    }

    pub fn as_bool(&self) -> Result<bool, ErrorKind> {
        match self {
            Self::Empty => Ok(false),
            Self::Number(value) => Ok(*value != 0.0),
            Self::Bool(value) => Ok(*value),
            Self::Text(text) => match text.to_ascii_uppercase().as_str() {
                "TRUE" => Ok(true),
                "FALSE" => Ok(false),
                _ => Err(ErrorKind::Value),
            },
            Self::Error(kind) => Err(*kind),
            Self::Array(values) => scalar_from_array(values)?.as_bool(),
        }
    }

    pub fn is_truthy(&self) -> Result<bool, ErrorKind> {
        self.as_bool()
    }

    pub fn flatten(&self) -> Result<Vec<Value>, ErrorKind> {
        match self {
            Self::Array(rows) => {
                let mut out = Vec::new();
                for row in rows {
                    for value in row {
                        out.extend(value.flatten()?);
                    }
                }
                Ok(out)
            }
            Self::Error(kind) => Err(*kind),
            other => Ok(vec![other.clone()]),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Number(value) => format_general(*value),
            Self::Bool(true) => "TRUE".to_owned(),
            Self::Bool(false) => "FALSE".to_owned(),
            Self::Text(text) => text.clone(),
            Self::Error(kind) => kind.to_string(),
            Self::Array(rows) => rows
                .first()
                .and_then(|row| row.first())
                .map(Value::display)
                .unwrap_or_default(),
        }
    }

    pub fn comparable(&self) -> Result<Comparable, ErrorKind> {
        match self {
            Self::Empty => Ok(Comparable::Number(0.0)),
            Self::Number(value) => Ok(Comparable::Number(*value)),
            Self::Bool(value) => Ok(Comparable::Number(if *value { 1.0 } else { 0.0 })),
            Self::Text(text) => {
                if let Some(number) = parse_number_like(text) {
                    Ok(Comparable::Number(number))
                } else {
                    Ok(Comparable::Text(text.to_ascii_uppercase()))
                }
            }
            Self::Error(kind) => Err(*kind),
            Self::Array(values) => scalar_from_array(values)?.comparable(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Comparable {
    Number(f64),
    Text(String),
}

impl Comparable {
    pub fn cmp_excel(&self, other: &Self) -> Result<std::cmp::Ordering, ErrorKind> {
        use std::cmp::Ordering;
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => Ok(a.partial_cmp(b).unwrap_or(Ordering::Equal)),
            (Self::Text(a), Self::Text(b)) => Ok(a.cmp(b)),
            (Self::Number(_), Self::Text(_)) => Ok(Ordering::Less),
            (Self::Text(_), Self::Number(_)) => Ok(Ordering::Greater),
        }
    }
}

pub fn parse_number_like(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    if let Some(stripped) = trimmed.strip_suffix('%') {
        return stripped
            .trim()
            .parse::<f64>()
            .ok()
            .map(|value| value / 100.0);
    }
    trimmed.parse::<f64>().ok()
}

pub fn format_general(value: f64) -> String {
    if !value.is_finite() {
        return ErrorKind::Num.to_string();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    if value.fract().abs() < 1e-12 && value.abs() < 1e15 {
        return format!("{:.0}", value.round());
    }
    let formatted = format!("{value:.10}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn scalar_from_array(values: &[Vec<Value>]) -> Result<Value, ErrorKind> {
    if values.len() == 1 && values[0].len() == 1 {
        Ok(values[0][0].clone())
    } else if values.is_empty() || values.iter().all(|row| row.is_empty()) {
        Ok(Value::Empty)
    } else {
        Err(ErrorKind::Value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(Value),
    Ref {
        sheet: Option<String>,
        cell: CellRef,
    },
    Range {
        sheet: Option<String>,
        start: CellRef,
        end: CellRef,
    },
    UnaryMinus(Box<Expr>),
    UnaryPlus(Box<Expr>),
    Percent(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Range,
}

pub fn parse_formula(input: &str) -> Result<Expr, ErrorKind> {
    let source = input.trim();
    let source = source.strip_prefix('=').unwrap_or(source);
    if source.trim().is_empty() {
        return Ok(Expr::Literal(Value::Empty));
    }
    let tokens = tokenize(source)?;
    let mut parser = Parser { tokens, index: 0 };
    let expr = parser.parse_comparison()?;
    if !matches!(parser.peek(), Token::Eof) {
        return Err(ErrorKind::Value);
    }
    Ok(expr)
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Ident(String),
    Error(ErrorKind),
    Bool(bool),
    Ref(CellRef),
    SheetRef(String, CellRef),
    SheetRange(String, CellRef, CellRef),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Amp,
    Percent,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Colon,
    Comma,
    LParen,
    RParen,
    Eof,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ErrorKind> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '"' {
            let (value, next) = read_string(&chars, i)?;
            tokens.push(Token::String(value));
            i = next;
            continue;
        }
        if ch == '#' {
            let (error, next) = read_error(&chars, i)?;
            tokens.push(Token::Error(error));
            i = next;
            continue;
        }
        if ch == '\'' {
            let (sheet, next) = read_quoted_sheet(&chars, i)?;
            i = next;
            if i < chars.len() && chars[i] == '!' {
                i += 1;
                let (token, next) = read_ref_or_range(&chars, i, Some(sheet))?;
                tokens.push(token);
                i = next;
                continue;
            }
            return Err(ErrorKind::Name);
        }
        match ch {
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Amp);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '.' | '0'..='9' => {
                let (number, next) = read_number(&chars, i)?;
                tokens.push(Token::Number(number));
                i = next;
            }
            '$' | 'A'..='Z' | 'a'..='z' | '_' => {
                let (token, next) = read_ident_or_ref(&chars, i)?;
                tokens.push(token);
                i = next;
            }
            _ => return Err(ErrorKind::Value),
        }
    }
    tokens.push(Token::Eof);
    Ok(tokens)
}

fn read_string(chars: &[char], mut i: usize) -> Result<(String, usize), ErrorKind> {
    i += 1;
    let mut out = String::new();
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            if i + 1 < chars.len() && chars[i + 1] == '"' {
                out.push('"');
                i += 2;
                continue;
            }
            return Ok((out, i + 1));
        }
        out.push(ch);
        i += 1;
    }
    Err(ErrorKind::Value)
}

fn read_error(chars: &[char], start: usize) -> Result<(ErrorKind, usize), ErrorKind> {
    let mut i = start;
    while i < chars.len()
        && !matches!(
            chars[i],
            ' ' | '\t' | ',' | ')' | '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | '%'
        )
    {
        i += 1;
        if chars[i - 1] == '!' || chars[i - 1] == '?' {
            break;
        }
    }
    let token: String = chars[start..i].iter().collect();
    ErrorKind::parse(&token)
        .map(|kind| (kind, i))
        .ok_or(ErrorKind::Name)
}

fn read_quoted_sheet(chars: &[char], mut i: usize) -> Result<(String, usize), ErrorKind> {
    i += 1;
    let mut out = String::new();
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' {
            if i + 1 < chars.len() && chars[i + 1] == '\'' {
                out.push('\'');
                i += 2;
                continue;
            }
            return Ok((out, i + 1));
        }
        out.push(ch);
        i += 1;
    }
    Err(ErrorKind::Name)
}

fn read_number(chars: &[char], start: usize) -> Result<(f64, usize), ErrorKind> {
    let mut i = start;
    let mut saw_digit = false;
    while i < chars.len() && chars[i].is_ascii_digit() {
        saw_digit = true;
        i += 1;
    }
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() {
            saw_digit = true;
            i += 1;
        }
    }
    if i < chars.len() && matches!(chars[i], 'e' | 'E') {
        let mut j = i + 1;
        if j < chars.len() && matches!(chars[j], '+' | '-') {
            j += 1;
        }
        let exp_start = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    if !saw_digit {
        return Err(ErrorKind::Value);
    }
    let token: String = chars[start..i].iter().collect();
    token
        .parse::<f64>()
        .map(|value| (value, i))
        .map_err(|_| ErrorKind::Value)
}

fn read_ident_or_ref(chars: &[char], start: usize) -> Result<(Token, usize), ErrorKind> {
    if let Ok((token, next)) = read_ref_or_range(chars, start, None)
        && matches!(
            token,
            Token::Ref(_) | Token::SheetRef(_, _) | Token::SheetRange(_, _, _)
        )
        && (next >= chars.len() || chars[next] != '(')
    {
        return Ok((token, next));
    }
    let mut i = start;
    if chars[i] == '$' {
        return Err(ErrorKind::Value);
    }
    while i < chars.len()
        && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
    {
        i += 1;
    }
    let ident: String = chars[start..i].iter().collect();
    if i < chars.len() && chars[i] == '!' {
        i += 1;
        return read_ref_or_range(chars, i, Some(ident));
    }
    match ident.to_ascii_uppercase().as_str() {
        "TRUE" => Ok((Token::Bool(true), i)),
        "FALSE" => Ok((Token::Bool(false), i)),
        _ => Ok((Token::Ident(ident), i)),
    }
}

fn read_ref_or_range(
    chars: &[char],
    start: usize,
    sheet: Option<String>,
) -> Result<(Token, usize), ErrorKind> {
    let (first, i) = read_cell_ref(chars, start)?;
    if i < chars.len()
        && chars[i] == ':'
        && let Ok((second, next)) = read_cell_ref(chars, i + 1)
        && let Some(sheet) = sheet
    {
        return Ok((Token::SheetRange(sheet, first, second), next));
    }
    if let Some(sheet) = sheet {
        Ok((Token::SheetRef(sheet, first), i))
    } else {
        Ok((Token::Ref(first), i))
    }
}

fn read_cell_ref(chars: &[char], start: usize) -> Result<(CellRef, usize), ErrorKind> {
    let remaining: String = chars[start..].iter().collect();
    let mut end = 0;
    if remaining.as_bytes().first() == Some(&b'$') {
        end = 1;
    }
    while end < remaining.len() && remaining.as_bytes()[end].is_ascii_alphabetic() {
        end += 1;
    }
    if end < remaining.len() && remaining.as_bytes()[end] == b'$' {
        end += 1;
    }
    let digits_start = end;
    while end < remaining.len() && remaining.as_bytes()[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        return Err(ErrorKind::Value);
    }
    let token = &remaining[..end];
    let cell = crate::spreadsheet::address::parse_ref(token).ok_or(ErrorKind::Ref)?;
    Ok((cell, start + token.len()))
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens.get(self.index).cloned().unwrap_or(Token::Eof);
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn parse_comparison(&mut self) -> Result<Expr, ErrorKind> {
        let mut left = self.parse_concat()?;
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq,
                Token::Ne => BinOp::Ne,
                Token::Lt => BinOp::Lt,
                Token::Le => BinOp::Le,
                Token::Gt => BinOp::Gt,
                Token::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            let right = self.parse_concat()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<Expr, ErrorKind> {
        let mut left = self.parse_add()?;
        while matches!(self.peek(), Token::Amp) {
            self.bump();
            let right = self.parse_add()?;
            left = Expr::Binary(BinOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ErrorKind> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let right = self.parse_mul()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ErrorKind> {
        let mut left = self.parse_pow()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.bump();
            let right = self.parse_pow()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_pow(&mut self) -> Result<Expr, ErrorKind> {
        let left = self.parse_percent()?;
        if matches!(self.peek(), Token::Caret) {
            self.bump();
            let right = self.parse_pow()?;
            Ok(Expr::Binary(BinOp::Pow, Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_percent(&mut self) -> Result<Expr, ErrorKind> {
        let mut expr = self.parse_unary()?;
        while matches!(self.peek(), Token::Percent) {
            self.bump();
            expr = Expr::Percent(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ErrorKind> {
        match self.peek() {
            Token::Plus => {
                self.bump();
                Ok(Expr::UnaryPlus(Box::new(self.parse_unary()?)))
            }
            Token::Minus => {
                self.bump();
                Ok(Expr::UnaryMinus(Box::new(self.parse_unary()?)))
            }
            _ => self.parse_range(),
        }
    }

    fn parse_range(&mut self) -> Result<Expr, ErrorKind> {
        let left = self.parse_primary()?;
        if matches!(self.peek(), Token::Colon) {
            self.bump();
            let right = self.parse_primary()?;
            match (left, right) {
                (
                    Expr::Ref {
                        sheet: left_sheet,
                        cell: start,
                    },
                    Expr::Ref {
                        sheet: right_sheet,
                        cell: end,
                    },
                ) if left_sheet == right_sheet => Ok(Expr::Range {
                    sheet: left_sheet,
                    start,
                    end,
                }),
                _ => Err(ErrorKind::Ref),
            }
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ErrorKind> {
        match self.bump() {
            Token::Number(value) => Ok(Expr::Literal(Value::Number(value))),
            Token::String(value) => Ok(Expr::Literal(Value::Text(value))),
            Token::Bool(value) => Ok(Expr::Literal(Value::Bool(value))),
            Token::Error(kind) => Ok(Expr::Literal(Value::Error(kind))),
            Token::Ref(cell) => Ok(Expr::Ref { sheet: None, cell }),
            Token::SheetRef(sheet, cell) => Ok(Expr::Ref {
                sheet: Some(sheet),
                cell,
            }),
            Token::SheetRange(sheet, start, end) => Ok(Expr::Range {
                sheet: Some(sheet),
                start,
                end,
            }),
            Token::Ident(name) => {
                if matches!(self.peek(), Token::LParen) {
                    self.bump();
                    let args = self.parse_args()?;
                    Ok(Expr::Call(name, args))
                } else if let Some(range) = CellRange::parse(&name) {
                    if range.start == range.end {
                        Ok(Expr::Ref {
                            sheet: None,
                            cell: CellRef {
                                col: range.start.col,
                                row: range.start.row,
                                col_abs: false,
                                row_abs: false,
                            },
                        })
                    } else {
                        Ok(Expr::Range {
                            sheet: None,
                            start: CellRef {
                                col: range.start.col,
                                row: range.start.row,
                                col_abs: false,
                                row_abs: false,
                            },
                            end: CellRef {
                                col: range.end.col,
                                row: range.end.row,
                                col_abs: false,
                                row_abs: false,
                            },
                        })
                    }
                } else {
                    Ok(Expr::Call(name, Vec::new()))
                }
            }
            Token::LParen => {
                let expr = self.parse_comparison()?;
                if !matches!(self.bump(), Token::RParen) {
                    return Err(ErrorKind::Value);
                }
                Ok(expr)
            }
            _ => Err(ErrorKind::Value),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ErrorKind> {
        let mut args = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            self.bump();
            return Ok(args);
        }
        loop {
            if matches!(self.peek(), Token::Comma) {
                args.push(Expr::Literal(Value::Empty));
            } else if matches!(self.peek(), Token::RParen) {
                args.push(Expr::Literal(Value::Empty));
                self.bump();
                break;
            } else {
                args.push(self.parse_comparison()?);
            }
            match self.bump() {
                Token::Comma => {}
                Token::RParen => break,
                _ => return Err(ErrorKind::Value),
            }
        }
        Ok(args)
    }
}

pub fn adjust_formula(input: &str, dcol: i32, drow: i32) -> String {
    let trimmed = input.trim();
    if !trimmed.starts_with('=') {
        return input.to_owned();
    }
    let Ok(expr) = parse_formula(trimmed) else {
        return input.to_owned();
    };
    format!("={}", rewrite_expr(&expr, dcol, drow))
}

fn rewrite_expr(expr: &Expr, dcol: i32, drow: i32) -> String {
    match expr {
        Expr::Literal(Value::Number(value)) => format_general(*value),
        Expr::Literal(Value::Text(text)) => format!("\"{}\"", text.replace('"', "\"\"")),
        Expr::Literal(Value::Bool(true)) => "TRUE".to_owned(),
        Expr::Literal(Value::Bool(false)) => "FALSE".to_owned(),
        Expr::Literal(Value::Error(kind)) => kind.to_string(),
        Expr::Literal(Value::Empty) => String::new(),
        Expr::Literal(Value::Array(_)) => String::new(),
        Expr::Ref { sheet, cell } => {
            let next = cell.translate(dcol, drow).unwrap_or(*cell);
            match sheet {
                Some(sheet) => format!("{}!{}", quote_sheet(sheet), next.a1()),
                None => next.a1(),
            }
        }
        Expr::Range { sheet, start, end } => {
            let start = start.translate(dcol, drow).unwrap_or(*start);
            let end = end.translate(dcol, drow).unwrap_or(*end);
            match sheet {
                Some(sheet) => format!("{}!{}:{}", quote_sheet(sheet), start.a1(), end.a1()),
                None => format!("{}:{}", start.a1(), end.a1()),
            }
        }
        Expr::UnaryMinus(inner) => format!("-{}", rewrite_expr(inner, dcol, drow)),
        Expr::UnaryPlus(inner) => format!("+{}", rewrite_expr(inner, dcol, drow)),
        Expr::Percent(inner) => format!("{}%", rewrite_expr(inner, dcol, drow)),
        Expr::Binary(op, left, right) => format!(
            "{}{}{}",
            rewrite_expr(left, dcol, drow),
            bin_op_symbol(*op),
            rewrite_expr(right, dcol, drow)
        ),
        Expr::Call(name, args) => format!(
            "{}({})",
            name.to_ascii_uppercase(),
            args.iter()
                .map(|arg| rewrite_expr(arg, dcol, drow))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn quote_sheet(name: &str) -> String {
    if name
        .chars()
        .any(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
    {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_owned()
    }
}

fn bin_op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Pow => "^",
        BinOp::Concat => "&",
        BinOp::Eq => "=",
        BinOp::Ne => "<>",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Range => ":",
    }
}

pub struct EvalContext<'a> {
    pub sheets: &'a [super::Sheet],
    pub sheet_index: usize,
    pub current: CellAddr,
    visiting: Vec<(usize, CellAddr)>,
}

impl<'a> EvalContext<'a> {
    pub fn new(sheets: &'a [super::Sheet], sheet_index: usize, current: CellAddr) -> Self {
        Self {
            sheets,
            sheet_index,
            current,
            visiting: Vec::new(),
        }
    }

    pub fn evaluate_cell(&mut self, sheet_index: usize, addr: CellAddr) -> Value {
        if let Some(sheet) = self.sheets.get(sheet_index)
            && let Some(cell) = sheet.cells.get(&addr)
        {
            if !cell.input.trim_start().starts_with('=') {
                return parse_literal(&cell.input);
            }
            if self.visiting.contains(&(sheet_index, addr)) {
                return Value::Error(ErrorKind::Circ);
            }
            self.visiting.push((sheet_index, addr));
            let value = match parse_formula(&cell.input) {
                Ok(expr) => {
                    let previous = self.sheet_index;
                    let previous_cell = self.current;
                    self.sheet_index = sheet_index;
                    self.current = addr;
                    let value = eval_expr(&expr, self);
                    self.sheet_index = previous;
                    self.current = previous_cell;
                    value
                }
                Err(kind) => Value::Error(kind),
            };
            self.visiting.pop();
            value
        } else {
            Value::Empty
        }
    }
}

pub fn parse_literal(input: &str) -> Value {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Value::Empty;
    }
    if let Some(error) = ErrorKind::parse(trimmed) {
        return Value::Error(error);
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "TRUE" => return Value::Bool(true),
        "FALSE" => return Value::Bool(false),
        _ => {}
    }
    if let Some(number) = parse_number_like(trimmed)
        && !trimmed
            .chars()
            .any(|ch| ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E')
        && trimmed
            .chars()
            .any(|ch| ch.is_ascii_digit() || ch == '.' || ch == '%' || ch == '-' || ch == '+')
        && !trimmed.contains(' ')
    {
        return Value::Number(number);
    }
    Value::Text(input.to_owned())
}

pub fn eval_expr(expr: &Expr, ctx: &mut EvalContext<'_>) -> Value {
    match expr {
        Expr::Literal(value) => value.clone(),
        Expr::Ref { sheet, cell } => {
            let Some(sheet_index) = resolve_sheet(ctx, sheet.as_deref()) else {
                return Value::Error(ErrorKind::Ref);
            };
            ctx.evaluate_cell(sheet_index, cell.addr())
        }
        Expr::Range { sheet, start, end } => {
            let Some(sheet_index) = resolve_sheet(ctx, sheet.as_deref()) else {
                return Value::Error(ErrorKind::Ref);
            };
            range_value(ctx, sheet_index, CellRange::new(start.addr(), end.addr()))
        }
        Expr::UnaryMinus(inner) => match eval_expr(inner, ctx).as_number() {
            Ok(value) => Value::number(-value),
            Err(kind) => Value::Error(kind),
        },
        Expr::UnaryPlus(inner) => match eval_expr(inner, ctx).as_number() {
            Ok(value) => Value::number(value),
            Err(kind) => Value::Error(kind),
        },
        Expr::Percent(inner) => match eval_expr(inner, ctx).as_number() {
            Ok(value) => Value::number(value / 100.0),
            Err(kind) => Value::Error(kind),
        },
        Expr::Binary(op, left, right) => {
            eval_binary(*op, eval_expr(left, ctx), eval_expr(right, ctx))
        }
        Expr::Call(name, args) => eval_call(name, args, ctx),
    }
}

fn resolve_sheet(ctx: &EvalContext<'_>, name: Option<&str>) -> Option<usize> {
    match name {
        None => Some(ctx.sheet_index),
        Some(name) => ctx
            .sheets
            .iter()
            .position(|sheet| sheet.name.eq_ignore_ascii_case(name)),
    }
}

fn range_value(ctx: &mut EvalContext<'_>, sheet_index: usize, range: CellRange) -> Value {
    let mut rows = Vec::new();
    for row in range.start.row..=range.end.row {
        let mut cols = Vec::new();
        for col in range.start.col..=range.end.col {
            cols.push(ctx.evaluate_cell(sheet_index, CellAddr { col, row }));
        }
        rows.push(cols);
    }
    if rows.len() == 1 && rows[0].len() == 1 {
        return rows[0][0].clone();
    }
    Value::Array(rows)
}

fn eval_binary(op: BinOp, left: Value, right: Value) -> Value {
    if let Some(kind) = left.error().or(right.error())
        && !matches!(op, BinOp::Concat)
    {
        return Value::Error(kind);
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            let Ok(lhs) = left.as_number() else {
                return Value::Error(ErrorKind::Value);
            };
            let Ok(rhs) = right.as_number() else {
                return Value::Error(ErrorKind::Value);
            };
            let result = match op {
                BinOp::Add => lhs + rhs,
                BinOp::Sub => lhs - rhs,
                BinOp::Mul => lhs * rhs,
                BinOp::Div => {
                    if rhs == 0.0 {
                        return Value::Error(ErrorKind::Div0);
                    }
                    lhs / rhs
                }
                BinOp::Pow => lhs.powf(rhs),
                _ => unreachable!(),
            };
            Value::number(result)
        }
        BinOp::Concat => {
            let Ok(lhs) = left.as_text() else {
                return Value::Error(left.error().unwrap_or(ErrorKind::Value));
            };
            let Ok(rhs) = right.as_text() else {
                return Value::Error(right.error().unwrap_or(ErrorKind::Value));
            };
            Value::Text(format!("{lhs}{rhs}"))
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let Ok(lhs) = left.comparable() else {
                return Value::Error(left.error().unwrap_or(ErrorKind::Value));
            };
            let Ok(rhs) = right.comparable() else {
                return Value::Error(right.error().unwrap_or(ErrorKind::Value));
            };
            let Ok(order) = lhs.cmp_excel(&rhs) else {
                return Value::Error(ErrorKind::Value);
            };
            Value::Bool(match op {
                BinOp::Eq => order.is_eq(),
                BinOp::Ne => order.is_ne(),
                BinOp::Lt => order.is_lt(),
                BinOp::Le => order.is_le(),
                BinOp::Gt => order.is_gt(),
                BinOp::Ge => order.is_ge(),
                _ => false,
            })
        }
        BinOp::Range => Value::Error(ErrorKind::Value),
    }
}

fn eval_call(name: &str, args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let name = name.to_ascii_uppercase();
    let values = || {
        args.iter()
            .map(|arg| eval_expr(arg, ctx))
            .collect::<Vec<_>>()
    };
    match name.as_str() {
        "SUM" => reduce_numbers(args, ctx, 0.0, |acc, n| acc + n),
        "PRODUCT" => reduce_numbers(args, ctx, 1.0, |acc, n| acc * n),
        "AVERAGE" => {
            let (sum, count) = fold_numbers(args, ctx);
            if count == 0 {
                Value::Error(ErrorKind::Div0)
            } else {
                Value::number(sum / count as f64)
            }
        }
        "MIN" => extremum(args, ctx, true),
        "MAX" => extremum(args, ctx, false),
        "COUNT" => Value::Number(count_values(args, ctx, CountMode::Numbers) as f64),
        "COUNTA" => Value::Number(count_values(args, ctx, CountMode::NonEmpty) as f64),
        "COUNTBLANK" => Value::Number(count_values(args, ctx, CountMode::Blank) as f64),
        "ABS" => unary_num(args, ctx, f64::abs),
        "SIGN" => unary_num(args, ctx, |n| {
            if n > 0.0 {
                1.0
            } else if n < 0.0 {
                -1.0
            } else {
                0.0
            }
        }),
        "INT" => unary_num(args, ctx, f64::floor),
        "TRUNC" => unary_num(args, ctx, |n| n.trunc()),
        "ROUND" => round_fn(args, ctx, RoundMode::Nearest),
        "ROUNDUP" => round_fn(args, ctx, RoundMode::Up),
        "ROUNDDOWN" => round_fn(args, ctx, RoundMode::Down),
        "SQRT" => unary_num_checked(args, ctx, |n| {
            if n < 0.0 {
                Err(ErrorKind::Num)
            } else {
                Ok(n.sqrt())
            }
        }),
        "POWER" => binary_num(args, ctx, f64::powf),
        "MOD" => binary_num_checked(args, ctx, |n, d| {
            if d == 0.0 {
                Err(ErrorKind::Div0)
            } else {
                Ok(n - d * (n / d).floor())
            }
        }),
        "PI" => Value::Number(std::f64::consts::PI),
        "TRUE" => Value::Bool(true),
        "FALSE" => Value::Bool(false),
        "NOT" => match arg1(args, ctx).and_then(|v| v.as_bool()) {
            Ok(value) => Value::Bool(!value),
            Err(kind) => Value::Error(kind),
        },
        "AND" => logical_fold(args, ctx, true, |acc, v| acc && v),
        "OR" => logical_fold(args, ctx, false, |acc, v| acc || v),
        "XOR" => {
            let mut count = 0;
            match each_logical(args, ctx, |v| {
                if v {
                    count += 1;
                }
            }) {
                Ok(()) => Value::Bool(count % 2 == 1),
                Err(kind) => Value::Error(kind),
            }
        }
        "IF" => eval_if(args, ctx),
        "IFERROR" => {
            let value = eval_optional(args, 0, ctx);
            if value.error().is_some() {
                eval_optional(args, 1, ctx)
            } else {
                value
            }
        }
        "IFNA" => {
            let value = eval_optional(args, 0, ctx);
            if value.error() == Some(ErrorKind::Na) {
                eval_optional(args, 1, ctx)
            } else {
                value
            }
        }
        "ISBLANK" => Value::Bool(eval_optional(args, 0, ctx).is_empty()),
        "ISNUMBER" => Value::Bool(matches!(eval_optional(args, 0, ctx), Value::Number(_))),
        "ISTEXT" => Value::Bool(matches!(eval_optional(args, 0, ctx), Value::Text(_))),
        "ISERROR" => Value::Bool(eval_optional(args, 0, ctx).error().is_some()),
        "ISNA" => Value::Bool(eval_optional(args, 0, ctx).error() == Some(ErrorKind::Na)),
        "NA" => Value::Error(ErrorKind::Na),
        "N" => match eval_optional(args, 0, ctx) {
            Value::Number(n) => Value::Number(n),
            Value::Bool(true) => Value::Number(1.0),
            Value::Bool(false) | Value::Empty => Value::Number(0.0),
            Value::Error(kind) => Value::Error(kind),
            _ => Value::Number(0.0),
        },
        "T" => match eval_optional(args, 0, ctx) {
            Value::Text(text) => Value::Text(text),
            Value::Error(kind) => Value::Error(kind),
            _ => Value::Text(String::new()),
        },
        "LEN" => match arg1(args, ctx).and_then(|v| v.as_text()) {
            Ok(text) => Value::Number(text.chars().count() as f64),
            Err(kind) => Value::Error(kind),
        },
        "UPPER" => text_map(args, ctx, |s| s.to_ascii_uppercase()),
        "LOWER" => text_map(args, ctx, |s| s.to_ascii_lowercase()),
        "TRIM" => text_map(args, ctx, |s| trim_excel(&s)),
        "LEFT" => text_slice(args, ctx, true),
        "RIGHT" => text_slice(args, ctx, false),
        "MID" => eval_mid(args, ctx),
        "CONCAT" | "CONCATENATE" => concat_args(args, ctx),
        "EXACT" => match (arg_at(args, 0, ctx), arg_at(args, 1, ctx)) {
            (Ok(a), Ok(b)) => match (a.as_text(), b.as_text()) {
                (Ok(a), Ok(b)) => Value::Bool(a == b),
                (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
            },
            (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
        },
        "FIND" => eval_find(args, ctx, true),
        "SEARCH" => eval_find(args, ctx, false),
        "SUBSTITUTE" => eval_substitute(args, ctx),
        "REPT" => match (
            arg_at(args, 0, ctx).and_then(|v| v.as_text()),
            arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        ) {
            (Ok(text), Ok(count)) => {
                if count < 0.0 {
                    Value::Error(ErrorKind::Value)
                } else {
                    Value::Text(text.repeat(count.floor() as usize))
                }
            }
            (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
        },
        "VALUE" => match arg1(args, ctx).and_then(|v| v.as_text()) {
            Ok(text) => parse_number_like(&text)
                .map(Value::Number)
                .unwrap_or(Value::Error(ErrorKind::Value)),
            Err(kind) => Value::Error(kind),
        },
        "TEXT" => eval_text_fn(args, ctx),
        "LN" => unary_num_checked(args, ctx, |n| {
            if n <= 0.0 {
                Err(ErrorKind::Num)
            } else {
                Ok(n.ln())
            }
        }),
        "LOG" => eval_log(args, ctx),
        "LOG10" => unary_num_checked(args, ctx, |n| {
            if n <= 0.0 {
                Err(ErrorKind::Num)
            } else {
                Ok(n.log10())
            }
        }),
        "EXP" => unary_num(args, ctx, f64::exp),
        "SIN" => unary_num(args, ctx, f64::sin),
        "COS" => unary_num(args, ctx, f64::cos),
        "TAN" => unary_num(args, ctx, f64::tan),
        "ASIN" => unary_num_checked(args, ctx, |n| {
            if !(-1.0..=1.0).contains(&n) {
                Err(ErrorKind::Num)
            } else {
                Ok(n.asin())
            }
        }),
        "ACOS" => unary_num_checked(args, ctx, |n| {
            if !(-1.0..=1.0).contains(&n) {
                Err(ErrorKind::Num)
            } else {
                Ok(n.acos())
            }
        }),
        "ATAN" => unary_num(args, ctx, f64::atan),
        "ATAN2" => binary_num(args, ctx, |x, y| y.atan2(x)),
        "DEGREES" => unary_num(args, ctx, |n| n.to_degrees()),
        "RADIANS" => unary_num(args, ctx, |n| n.to_radians()),
        "FLOOR" => binary_or_one(args, ctx, |n, sig| {
            if sig == 0.0 {
                Err(ErrorKind::Div0)
            } else {
                Ok((n / sig).floor() * sig)
            }
        }),
        "CEILING" => binary_or_one(args, ctx, |n, sig| {
            if sig == 0.0 {
                Err(ErrorKind::Div0)
            } else {
                Ok((n / sig).ceil() * sig)
            }
        }),
        "MEDIAN" => median(args, ctx),
        "STDEV" | "STDEV.S" => stdev(args, ctx, true),
        "STDEVP" | "STDEV.P" => stdev(args, ctx, false),
        "LARGE" => nth_stat(args, ctx, false),
        "SMALL" => nth_stat(args, ctx, true),
        "RANK" | "RANK.EQ" => eval_rank(args, ctx),
        "SUMIF" => eval_conditional(args, ctx, CondKind::Sum),
        "COUNTIF" => eval_conditional(args, ctx, CondKind::Count),
        "AVERAGEIF" => eval_conditional(args, ctx, CondKind::Average),
        "VLOOKUP" => eval_lookup(args, ctx, true),
        "HLOOKUP" => eval_lookup(args, ctx, false),
        "INDEX" => eval_index(args, ctx),
        "MATCH" => eval_match(args, ctx),
        "CHOOSE" => eval_choose(args, ctx),
        "ROW" => eval_row_col(args, ctx, true),
        "COLUMN" => eval_row_col(args, ctx, false),
        "ROWS" => eval_rows_cols(args, ctx, true),
        "COLUMNS" => eval_rows_cols(args, ctx, false),
        "ADDRESS" => eval_address(args, ctx),
        "INDIRECT" => eval_indirect(args, ctx),
        "OFFSET" => eval_offset(args, ctx),
        "TODAY" => Value::Number(excel_serial_today()),
        "NOW" => Value::Number(excel_serial_now()),
        "DATE" => eval_date(args, ctx),
        "YEAR" => date_part(args, ctx, DatePart::Year),
        "MONTH" => date_part(args, ctx, DatePart::Month),
        "DAY" => date_part(args, ctx, DatePart::Day),
        "TYPE" => match eval_optional(args, 0, ctx) {
            Value::Number(_) | Value::Empty => Value::Number(1.0),
            Value::Text(_) => Value::Number(2.0),
            Value::Bool(_) => Value::Number(4.0),
            Value::Error(_) => Value::Number(16.0),
            Value::Array(_) => Value::Number(64.0),
        },
        "SUMIFS" => eval_multi_conditional(args, ctx, CondKind::Sum),
        "COUNTIFS" => eval_countifs(args, ctx),
        "AVERAGEIFS" => eval_multi_conditional(args, ctx, CondKind::Average),
        "SUMPRODUCT" => eval_sumproduct(args, ctx),
        "IFS" => eval_ifs(args, ctx),
        "SWITCH" => eval_switch(args, ctx),
        "TEXTJOIN" => eval_textjoin(args, ctx),
        "PROPER" => text_map(args, ctx, proper_case),
        "REPLACE" => eval_replace(args, ctx),
        "CHAR" => match arg1(args, ctx).and_then(|v| v.as_number()) {
            Ok(code) => {
                let code = code.round() as u32;
                char::from_u32(code)
                    .map(|ch| Value::Text(ch.to_string()))
                    .unwrap_or(Value::Error(ErrorKind::Value))
            }
            Err(kind) => Value::Error(kind),
        },
        "CODE" => match arg1(args, ctx).and_then(|v| v.as_text()) {
            Ok(text) => text
                .chars()
                .next()
                .map(|ch| Value::Number(u32::from(ch) as f64))
                .unwrap_or(Value::Error(ErrorKind::Value)),
            Err(kind) => Value::Error(kind),
        },
        "RAND" => Value::Number(volatile_rand(ctx, 1.0)),
        "RANDBETWEEN" => match (
            arg_at(args, 0, ctx).and_then(|v| v.as_number()),
            arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        ) {
            (Ok(lo), Ok(hi)) => {
                let lo = lo.round() as i64;
                let hi = hi.round() as i64;
                if hi < lo {
                    Value::Error(ErrorKind::Num)
                } else {
                    let span = (hi - lo + 1) as f64;
                    Value::Number(lo as f64 + (volatile_rand(ctx, span) * span).floor())
                }
            }
            (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
        },
        "FACT" => unary_num_checked(args, ctx, |n| {
            if n < 0.0 || n > 170.0 {
                Err(ErrorKind::Num)
            } else {
                Ok((1..=n.round() as u32).map(f64::from).product())
            }
        }),
        "GCD" => gcd_lcm(args, ctx, true),
        "LCM" => gcd_lcm(args, ctx, false),
        "EVEN" => unary_num(args, ctx, excel_even),
        "ODD" => unary_num(args, ctx, excel_odd),
        "VAR" | "VAR.S" | "VARS" => stdev_var(args, ctx, true, true),
        "VAR.P" | "VARP" => stdev_var(args, ctx, true, false),
        "PMT" => eval_pmt(args, ctx),
        "FV" => eval_fv(args, ctx),
        "PV" => eval_pv(args, ctx),
        "NPV" => eval_npv(args, ctx),
        "DATEDIF" => eval_datedif(args, ctx),
        "EDATE" => eval_edate(args, ctx, false),
        "EOMONTH" => eval_edate(args, ctx, true),
        "WEEKDAY" => eval_weekday(args, ctx),
        "TIME" => eval_time(args, ctx),
        "HOUR" => time_part(args, ctx, 0),
        "MINUTE" => time_part(args, ctx, 1),
        "SECOND" => time_part(args, ctx, 2),
        "TRANSPOSE" => eval_transpose(args, ctx),
        "SEQUENCE" => eval_sequence(args, ctx),
        "LOOKUP" => eval_lookup(args, ctx, true),
        "ISLOGICAL" => Value::Bool(matches!(eval_optional(args, 0, ctx), Value::Bool(_))),
        "ISERR" => {
            let err = eval_optional(args, 0, ctx).error();
            Value::Bool(err.is_some() && err != Some(ErrorKind::Na))
        }
        "ISFORMULA" => eval_isformula(args, ctx),
        _ => {
            let _ = values;
            Value::Error(ErrorKind::Name)
        }
    }
}

enum CountMode {
    Numbers,
    NonEmpty,
    Blank,
}

enum RoundMode {
    Nearest,
    Up,
    Down,
}

enum CondKind {
    Sum,
    Count,
    Average,
}

enum DatePart {
    Year,
    Month,
    Day,
}

fn arg1(args: &[Expr], ctx: &mut EvalContext<'_>) -> Result<Value, ErrorKind> {
    arg_at(args, 0, ctx)
}

fn arg_at(args: &[Expr], index: usize, ctx: &mut EvalContext<'_>) -> Result<Value, ErrorKind> {
    if let Some(expr) = args.get(index) {
        let value = eval_expr(expr, ctx);
        if let Some(kind) = value.error() {
            Err(kind)
        } else {
            Ok(value)
        }
    } else {
        Err(ErrorKind::Value)
    }
}

fn eval_optional(args: &[Expr], index: usize, ctx: &mut EvalContext<'_>) -> Value {
    args.get(index)
        .map(|expr| eval_expr(expr, ctx))
        .unwrap_or(Value::Empty)
}

fn unary_num(args: &[Expr], ctx: &mut EvalContext<'_>, func: fn(f64) -> f64) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_number()) {
        Ok(value) => Value::number(func(value)),
        Err(kind) => Value::Error(kind),
    }
}

fn unary_num_checked(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    func: fn(f64) -> Result<f64, ErrorKind>,
) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_number()).and_then(func) {
        Ok(value) => Value::number(value),
        Err(kind) => Value::Error(kind),
    }
}

fn binary_num(args: &[Expr], ctx: &mut EvalContext<'_>, func: fn(f64, f64) -> f64) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(a), Ok(b)) => Value::number(func(a, b)),
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn binary_num_checked(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    func: fn(f64, f64) -> Result<f64, ErrorKind>,
) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(a), Ok(b)) => match func(a, b) {
            Ok(value) => Value::number(value),
            Err(kind) => Value::Error(kind),
        },
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn binary_or_one(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    func: fn(f64, f64) -> Result<f64, ErrorKind>,
) -> Value {
    let second = if args.len() > 1 {
        arg_at(args, 1, ctx).and_then(|v| v.as_number())
    } else {
        Ok(1.0)
    };
    match (arg_at(args, 0, ctx).and_then(|v| v.as_number()), second) {
        (Ok(a), Ok(b)) => match func(a, b) {
            Ok(value) => Value::number(value),
            Err(kind) => Value::Error(kind),
        },
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn reduce_numbers(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    init: f64,
    func: fn(f64, f64) -> f64,
) -> Value {
    let (value, _) = fold_numbers_with(args, ctx, init, func);
    Value::number(value)
}

fn fold_numbers(args: &[Expr], ctx: &mut EvalContext<'_>) -> (f64, usize) {
    fold_numbers_with(args, ctx, 0.0, |acc, n| acc + n)
}

fn fold_numbers_with(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    mut acc: f64,
    func: fn(f64, f64) -> f64,
) -> (f64, usize) {
    let mut count = 0usize;
    for arg in args {
        if let Ok(values) = eval_expr(arg, ctx).flatten() {
            for value in values {
                if let Value::Number(_) | Value::Bool(_) = &value
                    && let Ok(number) = value.as_number()
                {
                    acc = func(acc, number);
                    count += 1;
                } else if let Value::Number(number) = value {
                    acc = func(acc, number);
                    count += 1;
                }
            }
        }
    }
    (acc, count)
}

fn collect_numbers(args: &[Expr], ctx: &mut EvalContext<'_>) -> Result<Vec<f64>, ErrorKind> {
    let mut numbers = Vec::new();
    for arg in args {
        let value = eval_expr(arg, ctx);
        if let Some(kind) = value.error() {
            return Err(kind);
        }
        for item in value.flatten()? {
            if let Ok(number) = item.as_number()
                && !matches!(item, Value::Empty | Value::Text(_))
            {
                numbers.push(number);
            } else if let Value::Number(number) = item {
                numbers.push(number);
            }
        }
    }
    Ok(numbers)
}

fn extremum(args: &[Expr], ctx: &mut EvalContext<'_>, min: bool) -> Value {
    match collect_numbers(args, ctx) {
        Ok(mut numbers) => {
            numbers.retain(|n| n.is_finite());
            if numbers.is_empty() {
                Value::Number(0.0)
            } else if min {
                Value::number(numbers.into_iter().fold(f64::INFINITY, f64::min))
            } else {
                Value::number(numbers.into_iter().fold(f64::NEG_INFINITY, f64::max))
            }
        }
        Err(kind) => Value::Error(kind),
    }
}

fn count_values(args: &[Expr], ctx: &mut EvalContext<'_>, mode: CountMode) -> usize {
    let mut count = 0;
    for arg in args {
        if let Ok(values) = eval_expr(arg, ctx).flatten() {
            for value in values {
                match mode {
                    CountMode::Numbers => {
                        if matches!(value, Value::Number(_)) {
                            count += 1;
                        }
                    }
                    CountMode::NonEmpty => {
                        if !value.is_empty() {
                            count += 1;
                        }
                    }
                    CountMode::Blank => {
                        if value.is_empty() {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

fn round_fn(args: &[Expr], ctx: &mut EvalContext<'_>, mode: RoundMode) -> Value {
    let digits = if args.len() > 1 {
        arg_at(args, 1, ctx)
            .and_then(|v| v.as_number())
            .unwrap_or(0.0)
    } else {
        0.0
    };
    match arg_at(args, 0, ctx).and_then(|v| v.as_number()) {
        Ok(number) => {
            let factor = 10f64.powi(digits.round() as i32);
            let scaled = number * factor;
            let rounded = match mode {
                RoundMode::Nearest => scaled.round(),
                RoundMode::Up => {
                    if number >= 0.0 {
                        scaled.ceil()
                    } else {
                        scaled.floor()
                    }
                }
                RoundMode::Down => {
                    if number >= 0.0 {
                        scaled.floor()
                    } else {
                        scaled.ceil()
                    }
                }
            };
            Value::number(rounded / factor)
        }
        Err(kind) => Value::Error(kind),
    }
}

fn logical_fold(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    init: bool,
    func: fn(bool, bool) -> bool,
) -> Value {
    let mut acc = init;
    match each_logical(args, ctx, |v| acc = func(acc, v)) {
        Ok(()) => Value::Bool(acc),
        Err(kind) => Value::Error(kind),
    }
}

fn each_logical(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    mut visit: impl FnMut(bool),
) -> Result<(), ErrorKind> {
    for arg in args {
        for value in eval_expr(arg, ctx).flatten()? {
            visit(value.as_bool()?);
        }
    }
    Ok(())
}

fn eval_if(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match eval_optional(args, 0, ctx).is_truthy() {
        Ok(true) => eval_optional(args, 1, ctx),
        Ok(false) => eval_optional(args, 2, ctx),
        Err(kind) => Value::Error(kind),
    }
}

fn text_map(args: &[Expr], ctx: &mut EvalContext<'_>, func: fn(String) -> String) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_text()) {
        Ok(text) => Value::Text(func(text)),
        Err(kind) => Value::Error(kind),
    }
}

fn trim_excel(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_slice(args: &[Expr], ctx: &mut EvalContext<'_>, left: bool) -> Value {
    let count = if args.len() > 1 {
        arg_at(args, 1, ctx)
            .and_then(|v| v.as_number())
            .unwrap_or(1.0)
    } else {
        1.0
    }
    .max(0.0) as usize;
    match arg_at(args, 0, ctx).and_then(|v| v.as_text()) {
        Ok(text) => {
            let chars: Vec<char> = text.chars().collect();
            let slice = if left {
                chars.into_iter().take(count).collect()
            } else {
                let start = chars.len().saturating_sub(count);
                chars[start..].iter().collect()
            };
            Value::Text(slice)
        }
        Err(kind) => Value::Error(kind),
    }
}

fn eval_mid(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_text()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        arg_at(args, 2, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(text), Ok(start), Ok(len)) => {
            if start < 1.0 || len < 0.0 {
                return Value::Error(ErrorKind::Value);
            }
            let chars: Vec<char> = text.chars().collect();
            let start = (start as usize).saturating_sub(1);
            Value::Text(chars.iter().skip(start).take(len as usize).collect())
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn concat_args(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let mut out = String::new();
    for arg in args {
        match eval_expr(arg, ctx).flatten() {
            Ok(values) => {
                for value in values {
                    match value.as_text() {
                        Ok(text) => out.push_str(&text),
                        Err(kind) => return Value::Error(kind),
                    }
                }
            }
            Err(kind) => return Value::Error(kind),
        }
    }
    Value::Text(out)
}

fn eval_find(args: &[Expr], ctx: &mut EvalContext<'_>, case_sensitive: bool) -> Value {
    let start = if args.len() > 2 {
        arg_at(args, 2, ctx)
            .and_then(|v| v.as_number())
            .unwrap_or(1.0)
    } else {
        1.0
    };
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_text()),
        arg_at(args, 1, ctx).and_then(|v| v.as_text()),
    ) {
        (Ok(needle), Ok(haystack)) => {
            if start < 1.0 {
                return Value::Error(ErrorKind::Value);
            }
            let skip = (start as usize).saturating_sub(1);
            let hay = if case_sensitive {
                haystack.clone()
            } else {
                haystack.to_ascii_lowercase()
            };
            let needle = if case_sensitive {
                needle
            } else {
                needle.to_ascii_lowercase()
            };
            hay.chars()
                .skip(skip)
                .collect::<String>()
                .find(&needle)
                .map(|index| Value::Number((skip + index + 1) as f64))
                .unwrap_or(Value::Error(ErrorKind::Value))
        }
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn eval_substitute(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_text()),
        arg_at(args, 1, ctx).and_then(|v| v.as_text()),
        arg_at(args, 2, ctx).and_then(|v| v.as_text()),
    ) {
        (Ok(text), Ok(old), Ok(new)) => {
            if args.len() > 3 {
                match arg_at(args, 3, ctx).and_then(|v| v.as_number()) {
                    Ok(instance) => {
                        let n = instance.max(1.0) as usize;
                        Value::Text(replace_nth(&text, &old, &new, n))
                    }
                    Err(kind) => Value::Error(kind),
                }
            } else {
                Value::Text(text.replace(&old, &new))
            }
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn replace_nth(text: &str, old: &str, new: &str, instance: usize) -> String {
    if old.is_empty() {
        return text.to_owned();
    }
    let mut count = 0;
    let mut out = String::new();
    let mut rest = text;
    while let Some(index) = rest.find(old) {
        count += 1;
        out.push_str(&rest[..index]);
        if count == instance {
            out.push_str(new);
            out.push_str(&rest[index + old.len()..]);
            return out;
        }
        out.push_str(old);
        rest = &rest[index + old.len()..];
    }
    out.push_str(rest);
    out
}

fn eval_text_fn(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx),
        arg_at(args, 1, ctx).and_then(|v| v.as_text()),
    ) {
        (Ok(value), Ok(format)) => Value::Text(super::format::format_value(&value, &format)),
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn eval_log(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let base = if args.len() > 1 {
        arg_at(args, 1, ctx).and_then(|v| v.as_number())
    } else {
        Ok(10.0)
    };
    match (arg_at(args, 0, ctx).and_then(|v| v.as_number()), base) {
        (Ok(n), Ok(base)) if n > 0.0 && base > 0.0 && base != 1.0 => Value::number(n.log(base)),
        (Ok(_), Ok(_)) => Value::Error(ErrorKind::Num),
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn median(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match collect_numbers(args, ctx) {
        Ok(mut numbers) => {
            if numbers.is_empty() {
                return Value::Number(0.0);
            }
            numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = numbers.len() / 2;
            if numbers.len() % 2 == 1 {
                Value::Number(numbers[mid])
            } else {
                Value::number((numbers[mid - 1] + numbers[mid]) / 2.0)
            }
        }
        Err(kind) => Value::Error(kind),
    }
}

fn stdev(args: &[Expr], ctx: &mut EvalContext<'_>, sample: bool) -> Value {
    match collect_numbers(args, ctx) {
        Ok(numbers) => {
            let n = numbers.len() as f64;
            let denom = if sample { n - 1.0 } else { n };
            if denom <= 0.0 {
                return Value::Error(ErrorKind::Div0);
            }
            let mean = numbers.iter().sum::<f64>() / n;
            let var = numbers.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / denom;
            Value::number(var.sqrt())
        }
        Err(kind) => Value::Error(kind),
    }
}

fn nth_stat(args: &[Expr], ctx: &mut EvalContext<'_>, small: bool) -> Value {
    match (
        collect_numbers(&args[..1.min(args.len())], ctx),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(mut numbers), Ok(k)) => {
            numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if !small {
                numbers.reverse();
            }
            let index = k.round() as usize;
            if index == 0 || index > numbers.len() {
                Value::Error(ErrorKind::Num)
            } else {
                Value::Number(numbers[index - 1])
            }
        }
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn eval_rank(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        collect_numbers(&args[1..2.min(args.len())], ctx),
    ) {
        (Ok(target), Ok(mut numbers)) => {
            numbers.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            numbers
                .iter()
                .position(|n| *n == target)
                .map(|index| Value::Number((index + 1) as f64))
                .unwrap_or(Value::Error(ErrorKind::Na))
        }
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn eval_conditional(args: &[Expr], ctx: &mut EvalContext<'_>, kind: CondKind) -> Value {
    let range = eval_optional(args, 0, ctx);
    let criteria = eval_optional(args, 1, ctx);
    let values = if args.len() > 2 {
        eval_optional(args, 2, ctx)
    } else {
        range.clone()
    };
    let Ok(tests) = range.flatten() else {
        return Value::Error(ErrorKind::Value);
    };
    let Ok(nums) = values.flatten() else {
        return Value::Error(ErrorKind::Value);
    };
    let mut sum = 0.0;
    let mut count = 0usize;
    for (test, value) in tests.into_iter().zip(nums) {
        if criteria_match(&test, &criteria) {
            match kind {
                CondKind::Count => count += 1,
                CondKind::Sum | CondKind::Average => {
                    if let Ok(number) = value.as_number() {
                        sum += number;
                        count += 1;
                    }
                }
            }
        }
    }
    match kind {
        CondKind::Count => Value::Number(count as f64),
        CondKind::Sum => Value::number(sum),
        CondKind::Average => {
            if count == 0 {
                Value::Error(ErrorKind::Div0)
            } else {
                Value::number(sum / count as f64)
            }
        }
    }
}

fn criteria_match(value: &Value, criteria: &Value) -> bool {
    match criteria {
        Value::Text(text) => {
            let trimmed = text.trim();
            let (op, rest) = if let Some(rest) = trimmed.strip_prefix(">=") {
                (BinOp::Ge, rest)
            } else if let Some(rest) = trimmed.strip_prefix("<=") {
                (BinOp::Le, rest)
            } else if let Some(rest) = trimmed.strip_prefix("<>") {
                (BinOp::Ne, rest)
            } else if let Some(rest) = trimmed.strip_prefix('>') {
                (BinOp::Gt, rest)
            } else if let Some(rest) = trimmed.strip_prefix('<') {
                (BinOp::Lt, rest)
            } else if let Some(rest) = trimmed.strip_prefix('=') {
                (BinOp::Eq, rest)
            } else {
                (BinOp::Eq, trimmed)
            };
            let rhs = parse_literal(rest);
            let result = eval_binary(op, value.clone(), rhs);
            matches!(result, Value::Bool(true))
        }
        other => matches!(
            eval_binary(BinOp::Eq, value.clone(), other.clone()),
            Value::Bool(true)
        ),
    }
}

fn eval_lookup(args: &[Expr], ctx: &mut EvalContext<'_>, vertical: bool) -> Value {
    let lookup = eval_optional(args, 0, ctx);
    let table = eval_optional(args, 1, ctx);
    let index = eval_optional(args, 2, ctx)
        .as_number()
        .unwrap_or(1.0)
        .round() as usize;
    let approx = eval_optional(args, 3, ctx).as_bool().unwrap_or(true);
    let Value::Array(rows) = table else {
        return Value::Error(ErrorKind::Value);
    };
    if index == 0 {
        return Value::Error(ErrorKind::Value);
    }
    if vertical {
        let mut last = None;
        for row in &rows {
            let Some(key) = row.first() else { continue };
            if excel_equal(key, &lookup) {
                return row
                    .get(index - 1)
                    .cloned()
                    .unwrap_or(Value::Error(ErrorKind::Ref));
            }
            if approx
                && let (Ok(a), Ok(b)) = (key.comparable(), lookup.comparable())
                && matches!(
                    a.cmp_excel(&b),
                    Ok(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            {
                last = row.get(index - 1).cloned();
            }
        }
        last.unwrap_or(Value::Error(ErrorKind::Na))
    } else {
        let mut last = None;
        let width = rows.first().map(Vec::len).unwrap_or(0);
        for col in 0..width {
            let Some(key) = rows.first().and_then(|row| row.get(col)) else {
                continue;
            };
            let value = rows.get(index - 1).and_then(|row| row.get(col)).cloned();
            if excel_equal(key, &lookup) {
                return value.unwrap_or(Value::Error(ErrorKind::Ref));
            }
            if approx
                && let (Ok(a), Ok(b)) = (key.comparable(), lookup.comparable())
                && matches!(
                    a.cmp_excel(&b),
                    Ok(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            {
                last = value;
            }
        }
        last.unwrap_or(Value::Error(ErrorKind::Na))
    }
}

fn excel_equal(left: &Value, right: &Value) -> bool {
    matches!(
        eval_binary(BinOp::Eq, left.clone(), right.clone()),
        Value::Bool(true)
    )
}

fn eval_index(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let array = eval_optional(args, 0, ctx);
    let row = eval_optional(args, 1, ctx)
        .as_number()
        .unwrap_or(0.0)
        .round() as usize;
    let col = if args.len() > 2 {
        eval_optional(args, 2, ctx)
            .as_number()
            .unwrap_or(0.0)
            .round() as usize
    } else {
        0
    };
    match array {
        Value::Array(rows) => {
            if row == 0 && col == 0 {
                return Value::Array(rows);
            }
            if row == 0 {
                let values = rows
                    .iter()
                    .filter_map(|r| r.get(col.saturating_sub(1)).cloned())
                    .map(|v| vec![v])
                    .collect();
                return Value::Array(values);
            }
            let Some(selected) = rows.get(row - 1) else {
                return Value::Error(ErrorKind::Ref);
            };
            if col == 0 {
                return selected.first().cloned().unwrap_or(Value::Empty);
            }
            selected
                .get(col - 1)
                .cloned()
                .unwrap_or(Value::Error(ErrorKind::Ref))
        }
        other if row <= 1 && col <= 1 => other,
        _ => Value::Error(ErrorKind::Ref),
    }
}

fn eval_match(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let lookup = eval_optional(args, 0, ctx);
    let values = eval_optional(args, 1, ctx);
    let match_type = eval_optional(args, 2, ctx).as_number().unwrap_or(1.0);
    let Ok(items) = values.flatten() else {
        return Value::Error(ErrorKind::Value);
    };
    if match_type == 0.0 {
        return items
            .iter()
            .position(|item| excel_equal(item, &lookup))
            .map(|index| Value::Number((index + 1) as f64))
            .unwrap_or(Value::Error(ErrorKind::Na));
    }
    let mut last = None;
    for (index, item) in items.iter().enumerate() {
        if excel_equal(item, &lookup) {
            return Value::Number((index + 1) as f64);
        }
        if let (Ok(a), Ok(b)) = (item.comparable(), lookup.comparable()) {
            let order = a.cmp_excel(&b);
            if match_type > 0.0
                && matches!(
                    order,
                    Ok(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            {
                last = Some(index + 1);
            }
            if match_type < 0.0
                && matches!(
                    order,
                    Ok(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                )
            {
                last = Some(index + 1);
            }
        }
    }
    last.map(|index| Value::Number(index as f64))
        .unwrap_or(Value::Error(ErrorKind::Na))
}

fn eval_choose(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match arg_at(args, 0, ctx).and_then(|v| v.as_number()) {
        Ok(index) => {
            let index = index.round() as usize;
            if index == 0 || index >= args.len() {
                Value::Error(ErrorKind::Value)
            } else {
                eval_optional(args, index, ctx)
            }
        }
        Err(kind) => Value::Error(kind),
    }
}

fn eval_row_col(args: &[Expr], ctx: &mut EvalContext<'_>, row: bool) -> Value {
    if args.is_empty() {
        return Value::Number(if row {
            (ctx.current.row + 1) as f64
        } else {
            (ctx.current.col + 1) as f64
        });
    }
    match &args[0] {
        Expr::Ref { cell, .. } => Value::Number(if row {
            (cell.row + 1) as f64
        } else {
            (cell.col + 1) as f64
        }),
        Expr::Range { start, .. } => Value::Number(if row {
            (start.row + 1) as f64
        } else {
            (start.col + 1) as f64
        }),
        _ => Value::Error(ErrorKind::Value),
    }
}

fn eval_rows_cols(args: &[Expr], ctx: &mut EvalContext<'_>, rows: bool) -> Value {
    match args.first() {
        Some(Expr::Range { start, end, .. }) => {
            let range = CellRange::new(start.addr(), end.addr());
            Value::Number(if rows { range.rows() } else { range.cols() } as f64)
        }
        Some(Expr::Ref { .. }) => Value::Number(1.0),
        Some(other) => match eval_expr(other, ctx) {
            Value::Array(values) => Value::Number(if rows {
                values.len() as f64
            } else {
                values.first().map(Vec::len).unwrap_or(0) as f64
            }),
            _ => Value::Number(1.0),
        },
        None => Value::Error(ErrorKind::Value),
    }
}

fn eval_address(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let row = eval_optional(args, 0, ctx)
        .as_number()
        .unwrap_or(1.0)
        .round() as i64;
    let col = eval_optional(args, 1, ctx)
        .as_number()
        .unwrap_or(1.0)
        .round() as i64;
    if row < 1 || col < 1 {
        return Value::Error(ErrorKind::Value);
    }
    let abs = if args.len() > 2 {
        eval_optional(args, 2, ctx)
            .as_number()
            .unwrap_or(1.0)
            .round() as i32
    } else {
        1
    };
    let addr = CellAddr::new((col - 1) as u32, (row - 1) as u32);
    let Some(addr) = addr else {
        return Value::Error(ErrorKind::Ref);
    };
    let (col_abs, row_abs) = match abs {
        1 => (true, true),
        2 => (false, true),
        3 => (true, false),
        _ => (false, false),
    };
    Value::Text(
        CellRef {
            col: addr.col,
            row: addr.row,
            col_abs,
            row_abs,
        }
        .a1(),
    )
}

fn eval_indirect(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_text()) {
        Ok(text) => {
            let text = text.trim();
            if let Some((sheet, rest)) = split_sheet_ref(text)
                && let Some(range) = CellRange::parse(rest)
            {
                let Some(sheet_index) = resolve_sheet(ctx, Some(sheet)) else {
                    return Value::Error(ErrorKind::Ref);
                };
                return range_value(ctx, sheet_index, range);
            }
            if let Some(range) = CellRange::parse(text) {
                range_value(ctx, ctx.sheet_index, range)
            } else {
                Value::Error(ErrorKind::Ref)
            }
        }
        Err(kind) => Value::Error(kind),
    }
}

fn split_sheet_ref(text: &str) -> Option<(&str, &str)> {
    let (sheet, rest) = text.split_once('!')?;
    let sheet = sheet.trim().trim_matches('\'');
    Some((sheet, rest))
}

fn eval_offset(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let base = match args.first() {
        Some(Expr::Ref { sheet, cell }) => (sheet.clone(), CellRange::single(cell.addr())),
        Some(Expr::Range { sheet, start, end }) => {
            (sheet.clone(), CellRange::new(start.addr(), end.addr()))
        }
        _ => return Value::Error(ErrorKind::Value),
    };
    let rows = eval_optional(args, 1, ctx)
        .as_number()
        .unwrap_or(0.0)
        .round() as i32;
    let cols = eval_optional(args, 2, ctx)
        .as_number()
        .unwrap_or(0.0)
        .round() as i32;
    let height = if args.len() > 3 {
        eval_optional(args, 3, ctx)
            .as_number()
            .unwrap_or(base.1.rows() as f64)
    } else {
        base.1.rows() as f64
    }
    .round() as i32;
    let width = if args.len() > 4 {
        eval_optional(args, 4, ctx)
            .as_number()
            .unwrap_or(base.1.cols() as f64)
    } else {
        base.1.cols() as f64
    }
    .round() as i32;
    if height <= 0 || width <= 0 {
        return Value::Error(ErrorKind::Ref);
    }
    let Some(start) = base.1.start.offset(cols, rows) else {
        return Value::Error(ErrorKind::Ref);
    };
    let Some(end) = start.offset(width - 1, height - 1) else {
        return Value::Error(ErrorKind::Ref);
    };
    let Some(sheet_index) = resolve_sheet(ctx, base.0.as_deref()) else {
        return Value::Error(ErrorKind::Ref);
    };
    range_value(ctx, sheet_index, CellRange::new(start, end))
}

fn eval_date(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        arg_at(args, 2, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(year), Ok(month), Ok(day)) => ymd_to_serial(
            year.round() as i32,
            month.round() as i32,
            day.round() as i32,
        )
        .map(|serial| Value::Number(serial as f64))
        .unwrap_or(Value::Error(ErrorKind::Num)),
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn date_part(args: &[Expr], ctx: &mut EvalContext<'_>, part: DatePart) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_number()) {
        Ok(serial) => {
            let Some((year, month, day)) = serial_to_ymd(serial.floor() as i64) else {
                return Value::Error(ErrorKind::Num);
            };
            Value::Number(match part {
                DatePart::Year => year as f64,
                DatePart::Month => month as f64,
                DatePart::Day => day as f64,
            })
        }
        Err(kind) => Value::Error(kind),
    }
}

fn excel_serial_today() -> f64 {
    excel_serial_now().floor()
}

fn excel_serial_now() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    unix / 86_400.0 + 25569.0
}

fn ymd_to_serial(year: i32, month: i32, day: i32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month as u32, day as u32) + 25_569)
}

pub(crate) fn serial_ymd(serial: i64) -> Option<(i32, i32, i32)> {
    serial_to_ymd(serial)
}

fn serial_to_ymd(serial: i64) -> Option<(i32, i32, i32)> {
    if serial < 1 {
        return None;
    }
    let (year, month, day) = civil_from_days(serial - 25_569);
    Some((year, month as i32, day as i32))
}

fn days_from_civil(mut year: i32, month: u32, day: u32) -> i64 {
    year -= i32::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = (year - era * 400) as u32;
    let shifted = if month > 2 {
        month as i32 - 3
    } else {
        month as i32 + 9
    };
    let doy = (153 * shifted as u32 + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

fn civil_from_days(mut z: i64) -> (i32, u32, u32) {
    z += 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (year + i32::from(month <= 2), month, day)
}

fn eval_countifs(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    if args.len() < 2 || !args.len().is_multiple_of(2) {
        return Value::Error(ErrorKind::Value);
    }
    let Ok(first) = eval_optional(args, 0, ctx).flatten() else {
        return Value::Error(ErrorKind::Value);
    };
    let mut pairs = vec![(first, eval_optional(args, 1, ctx))];
    let mut index = 2;
    while index + 1 < args.len() {
        let Ok(tests) = eval_optional(args, index, ctx).flatten() else {
            return Value::Error(ErrorKind::Value);
        };
        if tests.len() != pairs[0].0.len() {
            return Value::Error(ErrorKind::Value);
        }
        pairs.push((tests, eval_optional(args, index + 1, ctx)));
        index += 2;
    }
    let count = (0..pairs[0].0.len())
        .filter(|slot| {
            pairs.iter().all(|(tests, criteria)| {
                tests
                    .get(*slot)
                    .is_some_and(|test| criteria_match(test, criteria))
            })
        })
        .count();
    Value::Number(count as f64)
}

fn eval_multi_conditional(args: &[Expr], ctx: &mut EvalContext<'_>, kind: CondKind) -> Value {
    if args.len() < 3 || args.len().is_multiple_of(2) {
        return Value::Error(ErrorKind::Value);
    }
    let values = eval_optional(args, 0, ctx);
    let Ok(nums) = values.flatten() else {
        return Value::Error(ErrorKind::Value);
    };
    let mut pairs = Vec::new();
    let mut index = 1;
    while index + 1 < args.len() {
        let Ok(tests) = eval_optional(args, index, ctx).flatten() else {
            return Value::Error(ErrorKind::Value);
        };
        let criteria = eval_optional(args, index + 1, ctx);
        if tests.len() != nums.len() {
            return Value::Error(ErrorKind::Value);
        }
        pairs.push((tests, criteria));
        index += 2;
    }
    let mut sum = 0.0;
    let mut count = 0usize;
    for (slot, value) in nums.iter().enumerate() {
        if pairs.iter().all(|(tests, criteria)| {
            tests
                .get(slot)
                .is_some_and(|test| criteria_match(test, criteria))
        }) {
            match kind {
                CondKind::Count => count += 1,
                CondKind::Sum | CondKind::Average => {
                    if let Ok(number) = value.as_number() {
                        sum += number;
                        count += 1;
                    }
                }
            }
        }
    }
    match kind {
        CondKind::Count => Value::Number(count as f64),
        CondKind::Sum => Value::number(sum),
        CondKind::Average if count == 0 => Value::Error(ErrorKind::Div0),
        CondKind::Average => Value::number(sum / count as f64),
    }
}

fn eval_sumproduct(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    if args.is_empty() {
        return Value::Error(ErrorKind::Value);
    }
    let arrays: Vec<Vec<Value>> = args
        .iter()
        .map(|arg| eval_expr(arg, ctx).flatten())
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    if arrays.is_empty() || arrays.iter().any(|a| a.len() != arrays[0].len()) {
        return Value::Error(ErrorKind::Value);
    }
    let mut sum = 0.0;
    for i in 0..arrays[0].len() {
        let mut product = 1.0;
        for array in &arrays {
            match array[i].as_number() {
                Ok(number) => product *= number,
                Err(kind) => return Value::Error(kind),
            }
        }
        sum += product;
    }
    Value::number(sum)
}

fn eval_ifs(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    if args.is_empty() || !args.len().is_multiple_of(2) {
        return Value::Error(ErrorKind::Na);
    }
    let mut index = 0;
    while index + 1 < args.len() {
        match eval_optional(args, index, ctx).is_truthy() {
            Ok(true) => return eval_optional(args, index + 1, ctx),
            Ok(false) => index += 2,
            Err(kind) => return Value::Error(kind),
        }
    }
    Value::Error(ErrorKind::Na)
}

fn eval_switch(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    if args.len() < 3 {
        return Value::Error(ErrorKind::Value);
    }
    let expr = eval_optional(args, 0, ctx);
    let mut index = 1;
    while index + 1 < args.len() {
        let candidate = eval_optional(args, index, ctx);
        if excel_equal(&expr, &candidate) {
            return eval_optional(args, index + 1, ctx);
        }
        index += 2;
    }
    if args.len().is_multiple_of(2) {
        eval_optional(args, args.len() - 1, ctx)
    } else {
        Value::Error(ErrorKind::Na)
    }
}

fn eval_textjoin(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let delimiter = match arg_at(args, 0, ctx).and_then(|v| v.as_text()) {
        Ok(text) => text,
        Err(kind) => return Value::Error(kind),
    };
    let skip_empty = match arg_at(args, 1, ctx).and_then(|v| v.as_bool()) {
        Ok(value) => value,
        Err(kind) => return Value::Error(kind),
    };
    let mut parts = Vec::new();
    for arg in args.iter().skip(2) {
        match eval_expr(arg, ctx).flatten() {
            Ok(values) => {
                for value in values {
                    match value.as_text() {
                        Ok(text) if !(skip_empty && text.is_empty()) => parts.push(text),
                        Ok(_) => {}
                        Err(kind) => return Value::Error(kind),
                    }
                }
            }
            Err(kind) => return Value::Error(kind),
        }
    }
    Value::Text(parts.join(&delimiter))
}

fn proper_case(input: String) -> String {
    let mut out = String::new();
    let mut start = true;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if start {
                out.extend(ch.to_uppercase());
                start = false;
            } else {
                out.extend(ch.to_lowercase());
            }
        } else {
            out.push(ch);
            start = true;
        }
    }
    out
}

fn eval_replace(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_text()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        arg_at(args, 2, ctx).and_then(|v| v.as_number()),
        arg_at(args, 3, ctx).and_then(|v| v.as_text()),
    ) {
        (Ok(text), Ok(start), Ok(len), Ok(new)) => {
            if start < 1.0 || len < 0.0 {
                return Value::Error(ErrorKind::Value);
            }
            let chars: Vec<char> = text.chars().collect();
            let start = (start as usize).saturating_sub(1);
            let end = (start + len as usize).min(chars.len());
            let mut out: String = chars.iter().take(start).collect();
            out.push_str(&new);
            out.extend(chars.iter().skip(end));
            Value::Text(out)
        }
        (Err(kind), _, _, _)
        | (_, Err(kind), _, _)
        | (_, _, Err(kind), _)
        | (_, _, _, Err(kind)) => Value::Error(kind),
    }
}

fn volatile_rand(ctx: &EvalContext<'_>, _span: f64) -> f64 {
    let mut seed = 0x9E37_79B9_u64
        .wrapping_add(ctx.sheet_index as u64)
        .wrapping_mul(0x0100_0000_01B3)
        .wrapping_add(u64::from(ctx.current.col))
        .wrapping_mul(0xC2B2_AE3D)
        .wrapping_add(u64::from(ctx.current.row));
    if let Ok(duration) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        seed ^= duration.subsec_nanos() as u64;
    }
    seed ^= seed >> 30;
    seed = seed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    seed ^= seed >> 27;
    (seed as f64) / (u64::MAX as f64)
}

fn excel_even(n: f64) -> f64 {
    if n >= 0.0 {
        let c = n.ceil();
        if (c as i64) % 2 == 0 { c } else { c + 1.0 }
    } else {
        let f = n.floor();
        if (f as i64) % 2 == 0 { f } else { f - 1.0 }
    }
}

fn excel_odd(n: f64) -> f64 {
    if n >= 0.0 {
        let c = n.ceil();
        if (c as i64) % 2 == 0 { c + 1.0 } else { c }
    } else {
        let f = n.floor();
        if (f as i64) % 2 == 0 { f - 1.0 } else { f }
    }
}

fn gcd_lcm(args: &[Expr], ctx: &mut EvalContext<'_>, gcd: bool) -> Value {
    let (values, _) = fold_numbers(args, ctx);
    let mut ints: Vec<i64> = Vec::new();
    for arg in args {
        if let Ok(flat) = eval_expr(arg, ctx).flatten() {
            for value in flat {
                if let Ok(number) = value.as_number() {
                    ints.push(number.round().abs() as i64);
                }
            }
        }
    }
    if ints.is_empty() {
        return Value::Error(ErrorKind::Value);
    }
    let result = if gcd {
        ints.into_iter().reduce(gcd_i64).unwrap_or(0)
    } else {
        ints.into_iter().reduce(lcm_i64).unwrap_or(0)
    };
    let _ = values;
    Value::Number(result as f64)
}

fn gcd_i64(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.abs()
}

fn lcm_i64(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        (a / gcd_i64(a, b)).saturating_mul(b).abs()
    }
}

fn stdev_var(args: &[Expr], ctx: &mut EvalContext<'_>, variance: bool, sample: bool) -> Value {
    let mut nums = Vec::new();
    for arg in args {
        if let Ok(flat) = eval_expr(arg, ctx).flatten() {
            for value in flat {
                if matches!(value, Value::Number(_) | Value::Bool(_))
                    && let Ok(number) = value.as_number()
                {
                    nums.push(number);
                }
            }
        }
    }
    let n = nums.len();
    let denom = if sample { n.saturating_sub(1) } else { n };
    if denom == 0 {
        return Value::Error(ErrorKind::Div0);
    }
    let mean = nums.iter().sum::<f64>() / n as f64;
    let var = nums.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / denom as f64;
    if variance {
        Value::number(var)
    } else {
        Value::number(var.sqrt())
    }
}

fn eval_pmt(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let rate = arg_at(args, 0, ctx).and_then(|v| v.as_number());
    let nper = arg_at(args, 1, ctx).and_then(|v| v.as_number());
    let pv = arg_at(args, 2, ctx).and_then(|v| v.as_number());
    match (rate, nper, pv) {
        (Ok(rate), Ok(nper), Ok(pv)) => {
            let fv = eval_optional(args, 3, ctx).as_number().unwrap_or(0.0);
            let type_end = eval_optional(args, 4, ctx).as_number().unwrap_or(0.0);
            if nper == 0.0 {
                return Value::Error(ErrorKind::Div0);
            }
            if rate == 0.0 {
                return Value::number(-(pv + fv) / nper);
            }
            let pow = (1.0 + rate).powf(nper);
            Value::number(-(rate * (pv * pow + fv)) / ((1.0 + rate * type_end) * (pow - 1.0)))
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn eval_fv(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let rate = arg_at(args, 0, ctx).and_then(|v| v.as_number());
    let nper = arg_at(args, 1, ctx).and_then(|v| v.as_number());
    let pmt = arg_at(args, 2, ctx).and_then(|v| v.as_number());
    match (rate, nper, pmt) {
        (Ok(rate), Ok(nper), Ok(pmt)) => {
            let pv = eval_optional(args, 3, ctx).as_number().unwrap_or(0.0);
            let type_end = eval_optional(args, 4, ctx).as_number().unwrap_or(0.0);
            if rate == 0.0 {
                return Value::number(-(pv + pmt * nper));
            }
            let pow = (1.0 + rate).powf(nper);
            Value::number(-pv * pow - pmt * (1.0 + rate * type_end) * (pow - 1.0) / rate)
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn eval_pv(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let rate = arg_at(args, 0, ctx).and_then(|v| v.as_number());
    let nper = arg_at(args, 1, ctx).and_then(|v| v.as_number());
    let pmt = arg_at(args, 2, ctx).and_then(|v| v.as_number());
    match (rate, nper, pmt) {
        (Ok(rate), Ok(nper), Ok(pmt)) => {
            let fv = eval_optional(args, 3, ctx).as_number().unwrap_or(0.0);
            let type_end = eval_optional(args, 4, ctx).as_number().unwrap_or(0.0);
            if rate == 0.0 {
                return Value::number(-(fv + pmt * nper));
            }
            let pow = (1.0 + rate).powf(nper);
            Value::number((-fv - pmt * (1.0 + rate * type_end) * (pow - 1.0) / rate) / pow)
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn eval_npv(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let rate = match arg_at(args, 0, ctx).and_then(|v| v.as_number()) {
        Ok(rate) => rate,
        Err(kind) => return Value::Error(kind),
    };
    let mut npv = 0.0;
    let mut period = 1.0;
    for arg in args.iter().skip(1) {
        match eval_expr(arg, ctx).flatten() {
            Ok(values) => {
                for value in values {
                    match value.as_number() {
                        Ok(cash) => {
                            npv += cash / (1.0 + rate).powf(period);
                            period += 1.0;
                        }
                        Err(kind) => return Value::Error(kind),
                    }
                }
            }
            Err(kind) => return Value::Error(kind),
        }
    }
    Value::number(npv)
}

fn eval_datedif(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        arg_at(args, 2, ctx).and_then(|v| v.as_text()),
    ) {
        (Ok(start), Ok(end), Ok(unit)) => {
            if end < start {
                return Value::Error(ErrorKind::Num);
            }
            let Some((sy, sm, sd)) = serial_to_ymd(start.floor() as i64) else {
                return Value::Error(ErrorKind::Num);
            };
            let Some((ey, em, ed)) = serial_to_ymd(end.floor() as i64) else {
                return Value::Error(ErrorKind::Num);
            };
            let value = match unit.to_ascii_uppercase().as_str() {
                "Y" => (ey - sy - i32::from(em < sm || (em == sm && ed < sd))) as f64,
                "M" => {
                    let mut months = (ey - sy) * 12 + (em - sm);
                    if ed < sd {
                        months -= 1;
                    }
                    months as f64
                }
                "D" => end.floor() - start.floor(),
                "YM" => {
                    let mut months = (em - sm + 12) % 12;
                    if ed < sd {
                        months = (months + 11) % 12;
                    }
                    months as f64
                }
                "MD" => {
                    let mut day = ed - sd;
                    if day < 0 {
                        day += 30;
                    }
                    day as f64
                }
                "YD" => ((end.floor() - start.floor()) as i64 % 365) as f64,
                _ => return Value::Error(ErrorKind::Num),
            };
            Value::Number(value)
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn eval_edate(args: &[Expr], ctx: &mut EvalContext<'_>, eomonth: bool) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(serial), Ok(months)) => {
            let Some((year, month, day)) = serial_to_ymd(serial.floor() as i64) else {
                return Value::Error(ErrorKind::Num);
            };
            let total = year * 12 + month - 1 + months.round() as i32;
            let year = total.div_euclid(12);
            let month = total.rem_euclid(12) + 1;
            let last = last_day_of_month(year, month as u32);
            let day = if eomonth {
                last as i32
            } else {
                day.min(last as i32)
            };
            ymd_to_serial(year, month, day)
                .map(|value| Value::Number(value as f64))
                .unwrap_or(Value::Error(ErrorKind::Num))
        }
        (Err(kind), _) | (_, Err(kind)) => Value::Error(kind),
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn eval_weekday(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_number()) {
        Ok(serial) => {
            let return_type = eval_optional(args, 1, ctx)
                .as_number()
                .unwrap_or(1.0)
                .round() as i32;
            let weekday = ((serial.floor() as i64 + 6) % 7) as i32; // 0=Sun
            let value = match return_type {
                1 => weekday + 1,
                2 => {
                    if weekday == 0 {
                        7
                    } else {
                        weekday
                    }
                }
                3 => {
                    if weekday == 0 {
                        6
                    } else {
                        weekday - 1
                    }
                }
                _ => weekday + 1,
            };
            Value::Number(value as f64)
        }
        Err(kind) => Value::Error(kind),
    }
}

fn eval_time(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match (
        arg_at(args, 0, ctx).and_then(|v| v.as_number()),
        arg_at(args, 1, ctx).and_then(|v| v.as_number()),
        arg_at(args, 2, ctx).and_then(|v| v.as_number()),
    ) {
        (Ok(h), Ok(m), Ok(s)) => {
            let fraction = (h * 3600.0 + m * 60.0 + s) / 86_400.0;
            Value::number(fraction.rem_euclid(1.0))
        }
        (Err(kind), _, _) | (_, Err(kind), _) | (_, _, Err(kind)) => Value::Error(kind),
    }
}

fn time_part(args: &[Expr], ctx: &mut EvalContext<'_>, part: u8) -> Value {
    match arg1(args, ctx).and_then(|v| v.as_number()) {
        Ok(serial) => {
            let mut seconds = ((serial.fract().abs() * 86_400.0).round() as i64).rem_euclid(86_400);
            let hour = seconds / 3600;
            seconds %= 3600;
            let minute = seconds / 60;
            let second = seconds % 60;
            Value::Number(match part {
                0 => hour as f64,
                1 => minute as f64,
                _ => second as f64,
            })
        }
        Err(kind) => Value::Error(kind),
    }
}

fn eval_transpose(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    match eval_optional(args, 0, ctx) {
        Value::Array(rows) => {
            let width = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut cols = vec![Vec::new(); width];
            for row in rows {
                for (index, value) in row.into_iter().enumerate() {
                    cols[index].push(value);
                }
            }
            Value::Array(cols)
        }
        other => other,
    }
}

fn eval_sequence(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let rows = arg_at(args, 0, ctx)
        .and_then(|v| v.as_number())
        .unwrap_or(1.0)
        .round() as i32;
    let cols = if args.len() > 1 {
        arg_at(args, 1, ctx)
            .and_then(|v| v.as_number())
            .unwrap_or(1.0)
            .round() as i32
    } else {
        1
    };
    let start = eval_optional(args, 2, ctx).as_number().unwrap_or(1.0);
    let step = eval_optional(args, 3, ctx).as_number().unwrap_or(1.0);
    if rows <= 0 || cols <= 0 {
        return Value::Error(ErrorKind::Num);
    }
    let mut value = start;
    let mut grid = Vec::new();
    for _ in 0..rows {
        let mut row = Vec::new();
        for _ in 0..cols {
            row.push(Value::Number(value));
            value += step;
        }
        grid.push(row);
    }
    if grid.len() == 1 && grid[0].len() == 1 {
        grid[0][0].clone()
    } else {
        Value::Array(grid)
    }
}

fn eval_isformula(args: &[Expr], ctx: &mut EvalContext<'_>) -> Value {
    let (sheet, addr) = match args.first() {
        Some(Expr::Ref { sheet, cell }) => (sheet.clone(), cell.addr()),
        _ => return Value::Bool(false),
    };
    let Some(index) = resolve_sheet(ctx, sheet.as_deref()) else {
        return Value::Bool(false);
    };
    Value::Bool(
        ctx.sheets
            .get(index)
            .and_then(|sheet| sheet.cells.get(&addr))
            .is_some_and(|cell| cell.is_formula()),
    )
}
