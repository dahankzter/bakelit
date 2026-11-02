//! CSV and TSV: a header row, then one mapping of text per row.

use crate::value::{DataError, LineIndex, Node, Pos, Value};

/// `delimiter` is `,` for CSV and a tab for TSV, which has no quoting.
pub fn parse_csv(text: &str, delimiter: u8) -> Result<Node, DataError> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .quoting(delimiter != b'\t')
        .has_headers(true)
        .flexible(true)
        .from_reader(text.as_bytes());
    // csv's own line numbers skip newlines inside quoted fields.
    let idx = LineIndex::new(text);
    // A record's position can point at the previous line's "\n" (after a "\r").
    let at = |p: Option<&csv::Position>| {
        p.map(|p| {
            let start = p.byte() as usize;
            let skip = text
                .get(start..)
                .map_or(0, |r| r.len() - r.trim_start_matches(['\r', '\n']).len());
            idx.pos(start + skip)
        })
    };
    let error = |e: csv::Error| {
        let msg = match e.kind() {
            csv::ErrorKind::Utf8 { .. } => "the file isn't UTF-8 text".to_string(),
            _ => e.to_string(),
        };
        DataError::at(at(e.position()), msg)
    };
    let headers = reader.headers().map_err(error)?.clone();
    if headers.is_empty() {
        return Err(DataError::at(None, "the file has no header row"));
    }
    let names: Vec<String> = headers.iter().map(str::to_string).collect();
    scan(text, delimiter, names.len() == 1)
        .map_err(|(at, msg)| DataError::at(Some(idx.pos(at)), msg))?;
    for (i, name) in names.iter().enumerate() {
        if names[..i].contains(name) {
            let first_line = Some(Pos { line: 1, col: 1 });
            return Err(DataError::at(
                first_line,
                format!("the header has {name:?} twice"),
            ));
        }
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(error)?;
        let pos = at(record.position());
        if record.len() != names.len() {
            let s = if record.len() == 1 { "" } else { "s" };
            let msg = format!(
                "this row has {} field{s}, the header has {}",
                record.len(),
                names.len()
            );
            return Err(DataError::at(pos, msg));
        }
        let cells = names
            .iter()
            .zip(record.iter())
            .map(|(k, v)| (k.clone(), Node::new(Value::Text(v.to_string()), pos)));
        rows.push(Node::new(Value::Map(cells.collect()), pos));
    }
    Ok(Node::new(Value::List(rows), None))
}

/// What the csv crate quietly accepts: text after a closing quote (`"x"y` reads as
/// `xy`), and blank lines, which it skips even where a one-column file means an empty
/// field.
fn scan(text: &str, delimiter: u8, one_column: bool) -> Result<(), (usize, &'static str)> {
    let b = text.as_bytes();
    let quoting = delimiter != b'\t';
    let (mut i, mut field_start, mut line_start, mut line) = (0, true, 0, 1);
    while i < b.len() {
        let c = b[i];
        if quoting && field_start && c == b'"' {
            i += 1;
            loop {
                match b.get(i) {
                    None => return Ok(()),
                    Some(b'"') if b.get(i + 1) == Some(&b'"') => i += 2,
                    Some(b'"') => break,
                    Some(_) => i += 1,
                }
            }
            i += 1;
            match b.get(i) {
                None | Some(b'\r' | b'\n') => {}
                Some(&d) if d == delimiter => {}
                Some(_) => return Err((i, "text after a closing quote; quote the whole field")),
            }
            field_start = false;
            continue;
        }
        field_start = c == delimiter || c == b'\n';
        if c == b'\n' {
            let blank = text[line_start..i].trim_end_matches('\r').is_empty();
            let more = !text[i + 1..].trim().is_empty();
            if one_column && blank && line > 1 && more {
                return Err((
                    line_start,
                    "a blank line; in a one-column file write an empty field as \"\"",
                ));
            }
            line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    Ok(())
}
