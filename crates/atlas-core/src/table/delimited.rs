//! Bounded prefix parser shared by linked tables and board cards.
use super::{MAX_COLS, MAX_ROWS};
use std::{io::Read, path::Path};

// Even a single unterminated quoted field has a finite I/O and CPU cost.
// 8 MiB covers 400 × 24 ordinary cells; longer data remains in the source.
const READ_BYTE_LIMIT: u64 = 8 * 1024 * 1024;

pub(super) fn read_delimited(path: &Path, sep: u8) -> Option<Vec<Vec<String>>> {
    read_prefix(std::fs::File::open(path).ok()?, sep)
}

fn read_prefix(reader: impl Read, sep: u8) -> Option<Vec<Vec<String>>> {
    let mut bytes = Vec::new();
    reader.take(READ_BYTE_LIMIT).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut cell_chars = 0;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if rows.len() >= MAX_ROWS {
            break;
        }
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    push_cell_char(&mut cell, &mut cell_chars, '"');
                } else {
                    quoted = false;
                }
            } else {
                push_cell_char(&mut cell, &mut cell_chars, c);
            }
            continue;
        }
        if c == '"' && cell.is_empty() {
            quoted = true;
            continue;
        }
        if c == sep as char {
            if row.len() < MAX_COLS {
                row.push(std::mem::take(&mut cell));
            } else {
                cell.clear();
            }
            cell_chars = 0;
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if row.len() < MAX_COLS {
                row.push(std::mem::take(&mut cell));
            } else {
                cell.clear();
            }
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
            cell_chars = 0;
            continue;
        }
        push_cell_char(&mut cell, &mut cell_chars, c);
    }
    if row.len() < MAX_COLS {
        row.push(cell);
    }
    if row.iter().any(|c| !c.is_empty()) && rows.len() < MAX_ROWS {
        rows.push(row);
    }
    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}

fn push_cell_char(cell: &mut String, chars: &mut usize, c: char) {
    if *chars < 200 {
        *chars += 1;
        cell.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn an_unterminated_cell_stops_reading_at_the_byte_budget() {
        let read = Cell::new(0);
        struct Endless<'a>(&'a Cell<u64>);
        impl Read for Endless<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes.fill(b'x');
                self.0.set(self.0.get() + bytes.len() as u64);
                Ok(bytes.len())
            }
        }
        let rows = read_prefix(Endless(&read), b',').unwrap();
        assert_eq!(read.get(), READ_BYTE_LIMIT);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].len(), 200);
    }

    #[test]
    fn unicode_quotes_newlines_and_cell_caps_share_one_parser() {
        let data = format!(
            "A,B\r\n\"a,\"\"b\"\"\nfox\",{}\nnext,last",
            "🦊".repeat(250)
        );
        let rows = read_prefix(data.as_bytes(), b',').unwrap();
        assert_eq!(rows[1][0], "a,\"b\"\nfox");
        assert_eq!(rows[1][1].chars().count(), 200);
        assert_eq!(rows[2], ["next", "last"]);
    }

    #[test]
    fn an_overwide_row_does_not_leak_its_discarded_cell_into_the_next_row() {
        let first = ["x"; MAX_COLS + 1].join(",");
        let body = format!("{first}\nnext,row");
        let rows = read_prefix(body.as_bytes(), b',').unwrap();
        assert_eq!(rows[0].len(), MAX_COLS);
        assert_eq!(rows[1], ["next", "row"]);
    }
}
