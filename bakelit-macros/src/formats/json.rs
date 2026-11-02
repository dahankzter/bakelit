//! JSON, and the JSON family (JSONC, JSON5, JSON Lines) on the same parser.

use crate::value::{DataError, LineIndex, Node, Value};
use jsonc_parser::ast;
use jsonc_parser::{CollectOptions, CommentCollectionStrategy, ParseOptions};

/// Which dialect: what the parser allows, and how strings and numbers are read.
#[derive(Clone, Copy, PartialEq)]
enum Dialect {
    Json,
    #[cfg_attr(not(feature = "jsonc"), allow(dead_code))]
    Jsonc,
    #[cfg_attr(not(feature = "json5"), allow(dead_code))]
    Json5,
}

impl Dialect {
    fn options(self) -> ParseOptions {
        let json5 = self == Dialect::Json5;
        ParseOptions {
            allow_comments: self != Dialect::Json,
            allow_trailing_commas: self != Dialect::Json,
            allow_loose_object_property_names: json5,
            allow_missing_commas: false,
            allow_single_quoted_strings: json5,
            allow_hexadecimal_numbers: json5,
            allow_unary_plus_numbers: json5,
            allow_bare_decimal_point_numbers: json5,
            allow_extended_string_escapes: json5,
            allow_non_finite_numbers: json5,
        }
    }
}

/// RFC 8259 JSON.
#[cfg(any(feature = "json", feature = "jsonl"))]
pub fn parse_json(text: &str) -> Result<Node, DataError> {
    parse_with(text, Dialect::Json)
}

/// JSON with comments and trailing commas.
#[cfg(feature = "jsonc")]
pub fn parse_jsonc(text: &str) -> Result<Node, DataError> {
    parse_with(text, Dialect::Jsonc)
}

/// JSON5.
#[cfg(feature = "json5")]
pub fn parse_json5(text: &str) -> Result<Node, DataError> {
    parse_with(text, Dialect::Json5)
}

/// JSON Lines: one JSON value on every line, as a list.
#[cfg(feature = "jsonl")]
pub fn parse_jsonl(text: &str) -> Result<Node, DataError> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut rows = Vec::new();
    if body.is_empty() {
        return Ok(Node::new(Value::List(rows), None));
    }
    for (i, line) in body.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let shift = |mut e: DataError| {
            e.pos = e.pos.map(|p| crate::value::Pos {
                line: p.line + i,
                col: p.col,
            });
            e
        };
        if line.trim().is_empty() {
            let pos = Some(crate::value::Pos {
                line: i + 1,
                col: 1,
            });
            return Err(DataError::at(
                pos,
                "a blank line; JSON Lines has one value on every line",
            ));
        }
        let mut node = parse_json(line).map_err(shift)?;
        node.shift_lines(i);
        rows.push(node);
    }
    Ok(Node::new(Value::List(rows), None))
}

fn parse_with(text: &str, dialect: Dialect) -> Result<Node, DataError> {
    let options = dialect.options();
    let idx = LineIndex::new(text);
    if dialect != Dialect::Json5 {
        whitespace(text, &idx)?;
    }
    let collect = CollectOptions {
        comments: CommentCollectionStrategy::Off,
        tokens: false,
    };
    let parsed = jsonc_parser::parse_to_ast(text, &collect, &options).map_err(|e| {
        let msg = e.kind().to_string();
        let mut chars = msg.chars();
        let msg = chars.next().map_or(String::new(), |c| {
            c.to_lowercase().collect::<String>() + chars.as_str()
        });
        DataError::at(Some(idx.pos(e.range().start)), msg)
    })?;
    let value = parsed
        .value
        .ok_or_else(|| DataError::at(None, "the file has no value"))?;
    walk(&value, text, &idx, dialect)
}

fn walk(v: &ast::Value, text: &str, idx: &LineIndex, dialect: Dialect) -> Result<Node, DataError> {
    let at = |start: usize| Some(idx.pos(start));
    Ok(match v {
        ast::Value::NullKeyword(n) => Node::new(Value::Null, at(n.range.start)),
        ast::Value::BooleanLit(b) => Node::new(Value::Bool(b.value), at(b.range.start)),
        ast::Value::StringLit(s) => Node::new(
            Value::Str(string(s, text, idx, dialect)?),
            at(s.range.start),
        ),
        ast::Value::NumberLit(n) => {
            let pos = at(n.range.start);
            let value = match dialect {
                Dialect::Json5 => number5(n.value),
                _ => number(n.value),
            };
            Node::new(value.map_err(|m| DataError::at(pos, m))?, pos)
        }
        ast::Value::Array(a) => {
            let items = a
                .elements
                .iter()
                .map(|e| walk(e, text, idx, dialect))
                .collect::<Result<_, _>>()?;
            Node::new(Value::List(items), at(a.range.start))
        }
        ast::Value::Object(o) => {
            let mut entries: Vec<(String, Node)> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for p in &o.properties {
                let (key, start) = match &p.name {
                    ast::ObjectPropName::String(s) => {
                        (string(s, text, idx, dialect)?, s.range.start)
                    }
                    ast::ObjectPropName::Word(w) => {
                        // JSON5: an ECMAScript identifier name.
                        let mut chars = w.value.chars();
                        let ident = chars
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '$' || c == '_')
                            && chars.all(|c| c.is_alphanumeric() || c == '$' || c == '_');
                        if !ident {
                            let msg = format!(
                                "an unquoted key must be an identifier; write \"{}\"",
                                w.value
                            );
                            return Err(DataError::at(at(w.range.start), msg));
                        }
                        (w.value.to_string(), w.range.start)
                    }
                };
                if !seen.insert(key.clone()) {
                    return Err(DataError::at(at(start), format!("duplicate key {key:?}")));
                }
                entries.push((key, walk(&p.value, text, idx, dialect)?));
            }
            Node::new(Value::Map(entries), at(o.range.start))
        }
    })
}

/// The string's value; raw control characters are errors, as RFC 8259 says (JSON5
/// allows them).
fn string(
    s: &ast::StringLit,
    text: &str,
    idx: &LineIndex,
    dialect: Dialect,
) -> Result<String, DataError> {
    let raw = &text[s.range.start..s.range.end];
    let at = |i: usize| Some(idx.pos(s.range.start + i));
    if dialect == Dialect::Json5 {
        // A line terminator only after a backslash (a line continuation).
        let (mut escaped, mut after_cr) = (false, false);
        for (i, c) in raw.char_indices() {
            let continued = escaped || (after_cr && c == '\n');
            if (c == '\n' || c == '\r') && !continued {
                return Err(DataError::at(
                    at(i),
                    "a line break in a string; end the line with \\ to continue it",
                ));
            }
            after_cr = escaped && c == '\r';
            escaped = !escaped && c == '\\';
        }
    } else if let Some(i) = raw.find(|c: char| (c as u32) < 0x20) {
        return Err(DataError::at(
            at(i),
            "control character in a string; escape it, like \\t or \\n",
        ));
    }
    Ok(s.value.to_string())
}

/// RFC 8259 whitespace only (jsonc-parser also skips other Unicode whitespace),
/// outside strings and comments.
fn whitespace(text: &str, idx: &LineIndex) -> Result<(), DataError> {
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                while let Some((_, c)) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, c)| *c == '/') => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, c)| *c == '*') => {
                chars.next();
                let mut star = false;
                for (_, c) in chars.by_ref() {
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
            }
            ' ' | '\t' | '\n' | '\r' => {}
            c if c.is_whitespace() => {
                let msg = "only spaces, tabs and line breaks may separate JSON values";
                return Err(DataError::at(Some(idx.pos(i)), msg));
            }
            _ => {}
        }
    }
    Ok(())
}

/// A JSON5 number: also a sign `+`, hex, `.5`, `5.`, `Infinity` and `NaN`.
#[cfg_attr(not(feature = "json5"), allow(dead_code))]
fn number5(raw: &str) -> Result<Value, String> {
    let (negative, body) = match raw.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, raw.strip_prefix('+').unwrap_or(raw)),
    };
    let sign = if negative { -1.0 } else { 1.0 };
    match body {
        "Infinity" => return Ok(Value::Float(sign * f64::INFINITY)),
        "NaN" => return Ok(Value::Float(f64::NAN)),
        _ => {}
    }
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        let magnitude = u128::from_str_radix(hex, 16)
            .map_err(|_| format!("{raw} is too large for any integer type"))?;
        return Ok(Value::Int {
            negative: negative && magnitude != 0,
            magnitude,
        });
    }
    match number(body)? {
        Value::Int { magnitude, .. } => Ok(Value::Int {
            negative: negative && magnitude != 0,
            magnitude,
        }),
        Value::Float(f) => Ok(Value::Float(sign * f)),
        other => Ok(other),
    }
}

/// A JSON number: an integer if it has no fraction or exponent, else an f64.
pub(crate) fn number(raw: &str) -> Result<Value, String> {
    if raw.contains(['.', 'e', 'E']) {
        let f: f64 = raw.parse().map_err(|_| format!("{raw} isn't a number"))?;
        return crate::value::check_float(raw, f).map(Value::Float);
    }
    let (negative, digits) = match raw.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, raw),
    };
    let magnitude: u128 = digits
        .parse()
        .map_err(|_| format!("{raw} is too large for any integer type"))?;
    Ok(Value::Int {
        negative: negative && magnitude != 0,
        magnitude,
    })
}

#[cfg(all(test, any(feature = "json", feature = "jsonl")))]
#[path = "json_tests.rs"]
mod json_tests;

#[cfg(all(test, any(feature = "jsonc", feature = "json5", feature = "jsonl")))]
#[path = "json_family_tests.rs"]
mod tests;
