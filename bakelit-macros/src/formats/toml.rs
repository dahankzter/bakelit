//! TOML, with positions from the `toml` crate's spans.

use crate::value::{DataError, LineIndex, Node, Value};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

pub fn parse_toml(text: &str) -> Result<Node, DataError> {
    let idx = LineIndex::new(text);
    let table = DeTable::parse(text).map_err(|e| {
        let pos = e.span().map(|s| idx.pos(s.start));
        DataError::at(pos, e.message().trim_end().to_string())
    })?;
    let pos = Some(idx.pos(table.span().start));
    Ok(Node::new(walk_table(table.get_ref(), &idx)?, pos))
}

fn walk_table(t: &DeTable, idx: &LineIndex) -> Result<Value, DataError> {
    let entries = t
        .iter()
        .map(|(k, v)| Ok((k.get_ref().to_string(), walk(v, idx)?)))
        .collect::<Result<_, DataError>>()?;
    Ok(Value::Map(entries))
}

fn walk(v: &Spanned<DeValue>, idx: &LineIndex) -> Result<Node, DataError> {
    let pos = Some(idx.pos(v.span().start));
    let value = match v.get_ref() {
        DeValue::String(s) => Value::Str(s.to_string()),
        DeValue::Boolean(b) => Value::Bool(*b),
        DeValue::Datetime(d) => Value::Str(d.to_string()),
        DeValue::Integer(i) => {
            let raw = i.as_str().replace('_', "");
            let (negative, digits) = match raw.strip_prefix('-') {
                Some(d) => (true, d),
                None => (false, raw.strip_prefix('+').unwrap_or(&raw)),
            };
            let magnitude = u128::from_str_radix(digits, i.radix())
                .ok()
                .filter(|m| {
                    if negative {
                        *m <= 1u128 << 63
                    } else {
                        *m <= i64::MAX as u128
                    }
                })
                .ok_or_else(|| {
                    DataError::at(
                        pos,
                        format!("{} is out of range for TOML's 64-bit integers", i.as_str()),
                    )
                })?;
            Value::Int {
                negative: negative && magnitude != 0,
                magnitude,
            }
        }
        DeValue::Float(f) => {
            let raw = f.as_str().replace('_', "");
            let body = raw.trim_start_matches(['+', '-']);
            let sign = if raw.starts_with('-') { -1.0 } else { 1.0 };
            let value = match body {
                "inf" => f64::INFINITY,
                "nan" => f64::NAN,
                _ => {
                    let parsed = body
                        .parse::<f64>()
                        .map_err(|_| DataError::at(pos, format!("{} isn't a float", f.as_str())))?;
                    crate::value::check_float(f.as_str(), parsed)
                        .map_err(|m| DataError::at(pos, m))?
                }
            };
            Value::Float(sign * value)
        }
        DeValue::Array(items) => Value::List(
            items
                .iter()
                .map(|i| walk(i, idx))
                .collect::<Result<_, _>>()?,
        ),
        DeValue::Table(t) => walk_table(t, idx)?,
    };
    Ok(Node::new(value, pos))
}

#[cfg(test)]
#[path = "toml_tests.rs"]
mod tests;
