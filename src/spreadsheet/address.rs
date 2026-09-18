use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

pub const MAX_COLS: u32 = 16_384;
pub const MAX_ROWS: u32 = 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellAddr {
    pub col: u32,
    pub row: u32,
}

impl CellAddr {
    pub fn new(col: u32, row: u32) -> Option<Self> {
        if col < MAX_COLS && row < MAX_ROWS {
            Some(Self { col, row })
        } else {
            None
        }
    }

    pub fn a1(self) -> String {
        format!("{}{}", col_name(self.col), self.row + 1)
    }

    pub fn parse_a1(input: &str) -> Option<Self> {
        parse_a1(input).map(|(addr, _, _)| addr)
    }

    pub fn offset(self, dcol: i32, drow: i32) -> Option<Self> {
        let col = i64::from(self.col) + i64::from(dcol);
        let row = i64::from(self.row) + i64::from(drow);
        if col < 0 || row < 0 {
            return None;
        }
        Self::new(col as u32, row as u32)
    }
}

impl fmt::Display for CellAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.a1())
    }
}

impl FromStr for CellAddr {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_a1(s).ok_or(())
    }
}

impl Serialize for CellAddr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.a1())
    }
}

impl<'de> Deserialize<'de> for CellAddr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse_a1(&value).ok_or_else(|| serde::de::Error::custom("invalid cell address"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub col: u32,
    pub row: u32,
    pub col_abs: bool,
    pub row_abs: bool,
}

impl CellRef {
    pub fn addr(self) -> CellAddr {
        CellAddr {
            col: self.col,
            row: self.row,
        }
    }

    pub fn a1(self) -> String {
        format!(
            "{}{}{}{}",
            if self.col_abs { "$" } else { "" },
            col_name(self.col),
            if self.row_abs { "$" } else { "" },
            self.row + 1
        )
    }

    pub fn translate(self, dcol: i32, drow: i32) -> Option<Self> {
        let col = if self.col_abs {
            self.col
        } else {
            let next = i64::from(self.col) + i64::from(dcol);
            if next < 0 {
                return None;
            }
            next as u32
        };
        let row = if self.row_abs {
            self.row
        } else {
            let next = i64::from(self.row) + i64::from(drow);
            if next < 0 {
                return None;
            }
            next as u32
        };
        CellAddr::new(col, row).map(|addr| Self {
            col: addr.col,
            row: addr.row,
            col_abs: self.col_abs,
            row_abs: self.row_abs,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRange {
    pub start: CellAddr,
    pub end: CellAddr,
}

impl CellRange {
    pub fn new(a: CellAddr, b: CellAddr) -> Self {
        Self {
            start: CellAddr {
                col: a.col.min(b.col),
                row: a.row.min(b.row),
            },
            end: CellAddr {
                col: a.col.max(b.col),
                row: a.row.max(b.row),
            },
        }
    }

    pub fn single(addr: CellAddr) -> Self {
        Self {
            start: addr,
            end: addr,
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if let Some((left, right)) = input.split_once(':') {
            Some(Self::new(
                CellAddr::parse_a1(left)?,
                CellAddr::parse_a1(right)?,
            ))
        } else {
            Some(Self::single(CellAddr::parse_a1(input)?))
        }
    }

    pub fn a1(self) -> String {
        if self.start == self.end {
            self.start.a1()
        } else {
            format!("{}:{}", self.start.a1(), self.end.a1())
        }
    }

    pub fn contains(self, addr: CellAddr) -> bool {
        addr.col >= self.start.col
            && addr.col <= self.end.col
            && addr.row >= self.start.row
            && addr.row <= self.end.row
    }

    pub fn intersects(self, other: Self) -> bool {
        self.start.col <= other.end.col
            && self.end.col >= other.start.col
            && self.start.row <= other.end.row
            && self.end.row >= other.start.row
    }

    pub fn cols(self) -> u32 {
        self.end.col - self.start.col + 1
    }

    pub fn rows(self) -> u32 {
        self.end.row - self.start.row + 1
    }

    pub fn cells(self) -> impl Iterator<Item = CellAddr> {
        (self.start.row..=self.end.row).flat_map(move |row| {
            (self.start.col..=self.end.col).map(move |col| CellAddr { col, row })
        })
    }
}

impl fmt::Display for CellRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.a1())
    }
}

impl Serialize for CellRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.a1())
    }
}

impl<'de> Deserialize<'de> for CellRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("invalid cell range"))
    }
}

pub fn col_name(mut col: u32) -> String {
    let mut chars = Vec::new();
    loop {
        chars.push(char::from(b'A' + (col % 26) as u8));
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    chars.iter().rev().collect()
}

pub fn parse_col(input: &str) -> Option<u32> {
    if input.is_empty() || input.len() > 3 {
        return None;
    }
    let mut value: u32 = 0;
    for ch in input.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        value = value
            .checked_mul(26)?
            .checked_add(u32::from(ch.to_ascii_uppercase()) - u32::from(b'A') + 1)?;
    }
    value.checked_sub(1).filter(|col| *col < MAX_COLS)
}

pub fn parse_a1(input: &str) -> Option<(CellAddr, bool, bool)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut index = 0;
    let col_abs = bytes[index] == b'$';
    if col_abs {
        index += 1;
    }
    let col_start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
        index += 1;
    }
    if index == col_start {
        return None;
    }
    let col = parse_col(std::str::from_utf8(&bytes[col_start..index]).ok()?)?;
    let row_abs = index < bytes.len() && bytes[index] == b'$';
    if row_abs {
        index += 1;
    }
    let row_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index != bytes.len() || index == row_start {
        return None;
    }
    let row = std::str::from_utf8(&bytes[row_start..index])
        .ok()?
        .parse::<u32>()
        .ok()?;
    if row == 0 {
        return None;
    }
    CellAddr::new(col, row - 1).map(|addr| (addr, col_abs, row_abs))
}

pub fn parse_ref(input: &str) -> Option<CellRef> {
    let (addr, col_abs, row_abs) = parse_a1(input)?;
    Some(CellRef {
        col: addr.col,
        row: addr.row,
        col_abs,
        row_abs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_match_excel() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(701), "ZZ");
        assert_eq!(col_name(702), "AAA");
        assert_eq!(col_name(16_383), "XFD");
        assert_eq!(parse_col("XFD"), Some(16_383));
        assert_eq!(parse_col("XFE"), None);
    }

    #[test]
    fn a1_round_trips_absolute_flags() {
        let parsed = parse_ref("$B$3").unwrap();
        assert_eq!(parsed.addr().a1(), "B3");
        assert!(parsed.col_abs && parsed.row_abs);
        assert_eq!(parsed.a1(), "$B$3");
        assert_eq!(CellAddr::parse_a1("A1").unwrap().a1(), "A1");
    }

    #[test]
    fn relative_translation_respects_absolute_anchors() {
        let relative = parse_ref("B2").unwrap().translate(1, 2).unwrap();
        assert_eq!(relative.a1(), "C4");
        let mixed = parse_ref("$B2").unwrap().translate(3, 1).unwrap();
        assert_eq!(mixed.a1(), "$B3");
        assert!(parse_ref("A1").unwrap().translate(-1, 0).is_none());
    }
}
