//! `.env` files: `KEY=value` lines. Nothing is read from the environment, and
//! `$` substitution is an error rather than a silent empty string.

use crate::value::{DataError, LineIndex, Node, Value};

const SUBSTITUTION: &str =
    "variable substitution isn't supported; use single quotes for a literal $";

pub fn parse_env(text: &str) -> Result<Node, DataError> {
    let idx = LineIndex::new(text);
    let err = |at: usize, msg: &str| DataError::at(Some(idx.pos(at)), msg);
    let mut entries: Vec<(String, Node)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i < text.len() {
        let line_end = text[i..].find('\n').map_or(text.len(), |n| i + n);
        let line = &text[i..line_end];
        let trimmed = line.trim_start();
        if trimmed.trim().is_empty() || trimmed.starts_with('#') {
            i = line_end + 1;
            continue;
        }
        let mut at = i + (line.len() - trimmed.len());
        let mut rest = trimmed;
        if let Some(r) = rest.strip_prefix("export ") {
            at += rest.len() - r.trim_start().len();
            rest = r.trim_start();
        }
        let key_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let key = &rest[..key_len];
        let after_key = rest[key_len..].trim_start_matches([' ', '\t']);
        if !after_key.starts_with('=') {
            return Err(err(at, "expected KEY=value"));
        }
        if key.is_empty() || key.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(err(
                at,
                "a key is letters, digits and _, starting with a letter or _, like API_KEY",
            ));
        }
        let key_at = at;
        let value_part = after_key[1..].trim_start_matches([' ', '\t']);
        let value_at = i + (line.len() - value_part.len().min(line.len()));
        let value_at = value_at.max(at);
        let (value, end) = match value_part.chars().next() {
            Some(q @ ('\'' | '"')) => {
                let body_start = value_at + 1;
                let close = find_close(&text[body_start..], q)
                    .ok_or_else(|| err(value_at, "a quoted value that never ends"))?;
                let raw = &text[body_start..body_start + close];
                let value = match q {
                    '\'' => raw.to_string(),
                    _ => unescape(raw).map_err(|m| err(value_at, m))?,
                };
                let after = body_start + close + 1;
                let line_end = text[after..].find('\n').map_or(text.len(), |n| after + n);
                let tail = text[after..line_end].trim();
                if !(tail.is_empty() || tail.starts_with('#')) {
                    return Err(err(value_at, "unexpected text after the closing quote"));
                }
                (value, line_end)
            }
            _ => {
                // Unquoted: up to a `#` after whitespace (which may be the space
                // after `=`), trailing space trimmed.
                let raw = value_part.trim_end_matches('\r');
                let spaced = after_key[1..].starts_with([' ', '\t']);
                let cut = raw
                    .char_indices()
                    .find(|&(j, c)| {
                        c == '#'
                            && if j == 0 {
                                spaced
                            } else {
                                raw[..j].ends_with([' ', '\t'])
                            }
                    })
                    .map_or(raw.len(), |(j, _)| j);
                let value = raw[..cut].trim_end();
                if substitutes(value) {
                    return Err(err(value_at, SUBSTITUTION));
                }
                (value.to_string(), line_end)
            }
        };
        if !seen.insert(key) {
            return Err(err(key_at, &format!("duplicate key {key:?}")));
        }
        entries.push((
            key.to_string(),
            Node::new(Value::Text(value), Some(idx.pos(value_at))),
        ));
        i = end + 1;
    }
    Ok(Node::new(Value::Map(entries), None))
}

/// The offset of the closing quote, skipping `\"` in double quotes.
fn find_close(s: &str, q: char) -> Option<usize> {
    let mut escaped = false;
    for (j, c) in s.char_indices() {
        if q == '"' && !escaped && c == '\\' {
            escaped = true;
            continue;
        }
        if c == q && !escaped {
            return Some(j);
        }
        escaped = false;
    }
    None
}

/// `$NAME` or `${...}`, which shells and dotenv tools would substitute.
fn substitutes(s: &str) -> bool {
    s.match_indices('$').any(|(j, _)| {
        s[j + 1..].starts_with(|c: char| c == '{' || c == '_' || c.is_ascii_alphabetic())
    })
}

/// Double-quoted escapes: `\n`, `\r`, `\t`, `\"`, `\\` and `\$`.
fn unescape(raw: &str) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(c @ ('"' | '\\' | '$')) => out.push(c),
                _ => return Err("an unknown escape; use \\n, \\r, \\t, \\\", \\\\ or \\$"),
            },
            '$' if substitutes(&format!("${}", chars.as_str())) => return Err(SUBSTITUTION),
            c => out.push(c),
        }
    }
    Ok(out)
}
