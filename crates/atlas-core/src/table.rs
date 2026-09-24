//! A spreadsheet as columns and rows for a Slate Link wire.
//!
//! Callers must not open a dehydrated cloud placeholder. This reader only
//! parses a file the caller has already decided is safe to read.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

const MAX_ROWS: usize = 400;
const MAX_COLS: usize = 24;

/// Columns a card will read or add. The board shows about a dozen at the
/// default size and scrolls the rest. A column past this stays in the file
/// and is refused from the card so the write is not invisible.
pub const SHEET_CARD_COLS: usize = MAX_COLS;

/// Opening of a CSV, TSV, or Excel file for a board card. Solid cell fills
/// from Excel are kept. A cloud placeholder is `None`.
pub fn read_sheet_card(path: &Path) -> Option<Vec<Vec<crate::office::SheetCell>>> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => delimited_card(path, b','),
        "tsv" => delimited_card(path, b'\t'),
        "xlsx" | "xlsm" | "xltx" | "xltm" => {
            crate::office::xlsx_sheet(path, MAX_ROWS, SHEET_CARD_COLS)
        }
        _ => None,
    }
}

fn delimited_card(path: &Path, sep: u8) -> Option<Vec<Vec<crate::office::SheetCell>>> {
    let grid = read_delimited(path, sep)?;
    let rows: Vec<Vec<crate::office::SheetCell>> = grid
        .into_iter()
        .map(|row| {
            row.into_iter()
                .take(SHEET_CARD_COLS)
                .map(|text| crate::office::SheetCell { text, fill: None })
                .collect()
        })
        .collect();
    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}

pub fn read_linked_table(path: &Path) -> Option<LinkedTable> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let grid = match ext.as_str() {
        "csv" => read_delimited(path, b',')?,
        "tsv" => read_delimited(path, b'\t')?,
        "xlsx" | "xlsm" | "xltx" | "xltm" => crate::office::xlsx_table(path, MAX_ROWS, MAX_COLS)?,
        _ => return None,
    };
    if grid.is_empty() {
        return None;
    }
    let columns = grid[0].clone();
    let rows = grid.into_iter().skip(1).collect();
    Some(LinkedTable { columns, rows })
}

fn read_delimited(path: &Path, sep: u8) -> Option<Vec<Vec<String>>> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
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
                    push_cell_char(&mut cell, '"');
                } else {
                    quoted = false;
                }
            } else {
                push_cell_char(&mut cell, c);
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
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if row.len() < MAX_COLS {
                row.push(std::mem::take(&mut cell));
            }
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
            continue;
        }
        push_cell_char(&mut cell, c);
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

fn push_cell_char(cell: &mut String, c: char) {
    if cell.chars().count() < 200 {
        cell.push(c);
    }
}

const WRITE_CAP: usize = 2 * 1024 * 1024;
const WRITE_ROWS: usize = 5_000;
const WRITE_CELL: usize = 32 * 1024;

/// Write one cell of a CSV, TSV, or Excel file. Returns the previous cell so
/// the caller can undo. A cloud placeholder, a file past the write cap, or an
/// unsupported extension is `None` and the file is left untouched.
pub fn write_sheet_cell(
    path: &Path,
    row: usize,
    col: usize,
    text: &str,
) -> Option<crate::office::PriorCell> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => write_delimited(path, b',', row, col, text),
        "tsv" => write_delimited(path, b'\t', row, col, text),
        "xlsx" | "xlsm" | "xltx" | "xltm" => crate::office::set_xlsx_cell(path, row, col, text),
        _ => None,
    }
}

/// Put a previous cell back. Returns the cell state that was replaced, so
/// redo is the same call with that value.
pub fn revert_sheet_cell(
    path: &Path,
    row: usize,
    col: usize,
    prior: &crate::office::PriorCell,
) -> Option<crate::office::PriorCell> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => revert_delimited(path, b',', row, col, prior),
        "tsv" => revert_delimited(path, b'\t', row, col, prior),
        "xlsx" | "xlsm" | "xltx" | "xltm" => {
            if prior.present {
                crate::office::set_xlsx_cell(path, row, col, &prior.text)
            } else {
                crate::office::clear_xlsx_cell(path, row, col)
            }
        }
        _ => None,
    }
}

/// Column index one past the rightmost cell in the file.
pub fn next_sheet_column(path: &Path) -> Option<usize> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => next_delimited(path, b','),
        "tsv" => next_delimited(path, b'\t'),
        "xlsx" | "xlsm" | "xltx" | "xltm" => crate::office::xlsx_next_column(path),
        _ => None,
    }
}

fn next_delimited(path: &Path, sep: u8) -> Option<usize> {
    let grid = load_delimited(path, sep)?;
    Some(grid.rows.iter().map(|row| row.len()).max().unwrap_or(0))
}

fn write_delimited(
    path: &Path,
    sep: u8,
    row: usize,
    col: usize,
    text: &str,
) -> Option<crate::office::PriorCell> {
    let mut grid = load_delimited(path, sep)?;
    if row > WRITE_ROWS || col > WRITE_COLS_LIMIT {
        return None;
    }
    let prior = capture_cell(&grid.rows, row, col);
    ensure_cell(&mut grid.rows, row, col);
    grid.rows[row][col] = text.to_string();
    save_delimited(path, sep, &grid)?;
    Some(prior)
}

fn revert_delimited(
    path: &Path,
    sep: u8,
    row: usize,
    col: usize,
    prior: &crate::office::PriorCell,
) -> Option<crate::office::PriorCell> {
    let mut grid = load_delimited(path, sep)?;
    let current = capture_cell(&grid.rows, row, col);
    if prior.present {
        ensure_cell(&mut grid.rows, row, col);
        grid.rows[row][col] = prior.text.clone();
    } else if row < grid.rows.len() {
        let len = grid.rows[row].len();
        grid.rows[row].truncate(prior.row_len.min(len));
        while grid.rows.last().is_some_and(|r| r.is_empty()) {
            grid.rows.pop();
        }
    }
    save_delimited(path, sep, &grid)?;
    Some(current)
}

const WRITE_COLS_LIMIT: usize = 256;

fn capture_cell(rows: &[Vec<String>], row: usize, col: usize) -> crate::office::PriorCell {
    match rows.get(row) {
        Some(cells) if col < cells.len() => crate::office::PriorCell {
            text: cells[col].clone(),
            present: true,
            row_len: cells.len(),
        },
        Some(cells) => crate::office::PriorCell {
            text: String::new(),
            present: false,
            row_len: cells.len(),
        },
        None => crate::office::PriorCell {
            text: String::new(),
            present: false,
            row_len: 0,
        },
    }
}

fn ensure_cell(rows: &mut Vec<Vec<String>>, row: usize, col: usize) {
    while rows.len() <= row {
        rows.push(Vec::new());
    }
    while rows[row].len() <= col {
        rows[row].push(String::new());
    }
}

struct Delimited {
    rows: Vec<Vec<String>>,
    bom: bool,
    crlf: bool,
    trailing_newline: bool,
}

fn load_delimited(path: &Path, sep: u8) -> Option<Delimited> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > WRITE_CAP {
        return None;
    }
    let bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if bom { &bytes[3..] } else { &bytes[..] };
    let crlf = body.windows(2).any(|w| w == b"\r\n");
    let trailing_newline = body.ends_with(b"\n") || body.ends_with(b"\r");
    let text = String::from_utf8_lossy(body);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if rows.len() > WRITE_ROWS || cell.len() > WRITE_CELL {
            return None;
        }
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        if c == '"' && cell.is_empty() {
            quoted = true;
            continue;
        }
        if c == sep as char {
            if row.len() > WRITE_COLS_LIMIT {
                return None;
            }
            row.push(std::mem::take(&mut cell));
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
            continue;
        }
        cell.push(c);
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    if let Some(last) = rows.last() {
        if last.iter().all(|c| c.is_empty()) && trailing_newline {
            rows.pop();
        }
    }
    Some(Delimited {
        rows,
        bom,
        crlf,
        trailing_newline,
    })
}

fn save_delimited(path: &Path, sep: u8, grid: &Delimited) -> Option<()> {
    let mut out = Vec::new();
    if grid.bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    let nl: &[u8] = if grid.crlf { b"\r\n" } else { b"\n" };
    for (i, row) in grid.rows.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(nl);
        }
        for (c, cell) in row.iter().enumerate() {
            if c > 0 {
                out.push(sep);
            }
            out.extend_from_slice(&escape_cell(cell, sep).into_bytes());
        }
    }
    if grid.trailing_newline && !grid.rows.is_empty() {
        out.extend_from_slice(nl);
    }
    let tmp = path.with_extension("slate-sheet-tmp");
    std::fs::write(&tmp, out).ok()?;
    if std::fs::rename(&tmp, path).is_err() {
        let bak = path.with_extension("slate-sheet-bak");
        std::fs::rename(path, &bak).ok()?;
        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::rename(&bak, path);
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        let _ = std::fs::remove_file(&bak);
    }
    Some(())
}

fn escape_cell(cell: &str, sep: u8) -> String {
    let sep = sep as char;
    if cell.contains(sep) || cell.contains('"') || cell.contains('\n') || cell.contains('\r') {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_keeps_a_quoted_comma_and_the_header() {
        let dir = std::env::temp_dir().join(format!("slate-link-csv-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("rows.csv");
        std::fs::write(&path, "Month,Value\n\"Jan, early\",12\nFeb,9\n").unwrap();
        let table = read_linked_table(&path).unwrap();
        assert_eq!(table.columns, vec!["Month", "Value"]);
        assert_eq!(table.rows, vec![vec!["Jan, early", "12"], vec!["Feb", "9"]]);
        let prior = write_sheet_cell(&path, 1, 1, "15").unwrap();
        assert_eq!(prior.text, "12");
        assert!(prior.present);
        let added = write_sheet_cell(&path, 0, 2, "Note").unwrap();
        assert!(!added.present);
        assert_eq!(added.row_len, 2);
        let table = read_linked_table(&path).unwrap();
        assert_eq!(table.columns, vec!["Month", "Value", "Note"]);
        assert_eq!(table.rows[0], vec!["Jan, early", "15"]);
        let back = revert_sheet_cell(&path, 1, 1, &prior).unwrap();
        assert_eq!(back.text, "15");
        revert_sheet_cell(&path, 0, 2, &added).unwrap();
        let table = read_linked_table(&path).unwrap();
        assert_eq!(table.columns, vec!["Month", "Value"]);
        assert_eq!(table.rows[0][1], "12");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sheet_card_keeps_rows_past_a_dozen_and_stops_at_the_guard() {
        let dir = std::env::temp_dir().join(format!("slate-sheet-card-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("rows.csv");
        let mut body = String::from("A,B\n");
        for i in 0..20 {
            body.push_str(&format!("{i},x\n"));
        }
        std::fs::write(&path, &body).unwrap();
        let rows = read_sheet_card(&path).unwrap();
        assert_eq!(rows.len(), 21);
        assert_eq!(rows[13][0].text, "12");

        body.clear();
        body.push_str("A,B\n");
        for i in 0..450 {
            body.push_str(&format!("{i},x\n"));
        }
        std::fs::write(&path, body).unwrap();
        let rows = read_sheet_card(&path).unwrap();
        assert_eq!(rows.len(), MAX_ROWS);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
