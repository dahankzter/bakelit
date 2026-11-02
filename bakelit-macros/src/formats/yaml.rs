//! YAML: one document, scalars resolved by the YAML 1.2 core schema from their text.

use crate::value::{DataError, Node, Pos, Value};
use saphyr::{MarkedYaml, ScalarStyle, Tag, YamlData, YamlLoader};
use saphyr_parser::{Marker, Parser, ScanError};

pub fn parse_yaml(text: &str) -> Result<Node, DataError> {
    let mut loader: YamlLoader<MarkedYaml> = YamlLoader::default();
    loader.early_parse(false);
    let mut parser = Parser::new_from_str(text);
    parser.load(&mut loader, true).map_err(scan_error)?;
    if let Some(e) = loader.error() {
        return Err(scan_error(e.clone()));
    }
    let docs = loader.into_documents();
    match docs.as_slice() {
        [] => Ok(Node::new(Value::Null, None)),
        [doc] => walk(doc),
        _ => Err(DataError::at(
            None,
            format!(
                "the file has {} YAML documents; bake! reads one",
                docs.len()
            ),
        )),
    }
}

fn scan_error(e: ScanError) -> DataError {
    DataError::at(Some(pos(e.marker())), e.info().to_string())
}

fn pos(m: &Marker) -> Pos {
    Pos {
        line: m.line(),
        col: m.col() + 1,
    }
}

fn walk(y: &MarkedYaml) -> Result<Node, DataError> {
    let at = Some(pos(&y.span.start));
    let err = |msg: String| DataError::at(at, msg);
    let value = match &y.data {
        YamlData::Representation(raw, style, tag) => {
            scalar(raw, *style, tag.as_deref()).map_err(err)?
        }
        YamlData::Sequence(items) => Value::List(items.iter().map(walk).collect::<Result<_, _>>()?),
        YamlData::Mapping(map) => {
            let mut entries = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for (k, v) in map.iter() {
                let key = match &k.data {
                    YamlData::Representation(raw, ScalarStyle::Plain, None) if raw == "<<" => {
                        return Err(DataError::at(
                            Some(pos(&k.span.start)),
                            "merge keys (<<) aren't supported; write the keys out",
                        ));
                    }
                    YamlData::Representation(_, _, Some(tag)) if !tag.is_yaml_core_schema() => {
                        let msg = format!("the tag {} isn't supported", tag_name(tag));
                        return Err(DataError::at(Some(pos(&k.span.start)), msg));
                    }
                    YamlData::Representation(raw, _, _) => raw.to_string(),
                    _ => {
                        return Err(DataError::at(
                            Some(pos(&k.span.start)),
                            "a key must be a scalar",
                        ));
                    }
                };
                // Keys that differ only in quoting (`a` and "a") are the same key here.
                if !seen.insert(key.clone()) {
                    return Err(DataError::at(
                        Some(pos(&k.span.start)),
                        format!("duplicate key {key:?}"),
                    ));
                }
                entries.push((key, walk(v)?));
            }
            Value::Map(entries)
        }
        YamlData::Tagged(tag, inner) => match (tag.is_yaml_core_schema(), tag.suffix.as_str()) {
            (true, "seq" | "map") => return walk(inner),
            _ => return Err(err(format!("the tag {} isn't supported", tag_name(tag)))),
        },
        YamlData::Alias(_) => return Err(err("an alias to an unknown anchor".into())),
        YamlData::BadValue => return Err(err("a bad value".into())),
        YamlData::Value(_) => unreachable!("early_parse is off"),
    };
    Ok(Node::new(value, at))
}

fn tag_name(tag: &Tag) -> String {
    match tag.is_yaml_core_schema() {
        true => format!("!!{}", tag.suffix),
        false => format!("{}{}", tag.handle, tag.suffix),
    }
}

/// A scalar by its tag, its quoting, or the core schema's rules for plain text.
fn scalar(raw: &str, style: ScalarStyle, tag: Option<&Tag>) -> Result<Value, String> {
    if let Some(tag) = tag {
        if !tag.is_yaml_core_schema() {
            return Err(format!("the tag {} isn't supported", tag_name(tag)));
        }
        if tag.suffix != "str"
            && let Some(e) = plain_error(raw)
        {
            return Err(e);
        }
        let resolved = plain(raw);
        return match tag.suffix.as_str() {
            "str" => Ok(Value::Str(raw.to_string())),
            "int" => match resolved {
                Value::Int { .. } => Ok(resolved),
                _ => Err(format!("{raw:?} isn't an integer")),
            },
            "float" => match resolved {
                Value::Float(_) => Ok(resolved),
                Value::Int {
                    negative,
                    magnitude,
                } => {
                    let f = magnitude as f64;
                    Ok(Value::Float(if negative { -f } else { f }))
                }
                _ => Err(format!("{raw:?} isn't a float")),
            },
            "bool" => match resolved {
                Value::Bool(_) => Ok(resolved),
                _ => Err(format!("{raw:?} isn't true or false")),
            },
            "null" => match resolved {
                Value::Null => Ok(resolved),
                _ => Err(format!("{raw:?} isn't null")),
            },
            _ => Err(format!("the tag {} isn't supported", tag_name(tag))),
        };
    }
    Ok(match style {
        ScalarStyle::Plain => match plain_error(raw) {
            Some(e) => return Err(e),
            None => plain(raw),
        },
        _ => Value::Str(raw.to_string()),
    })
}

/// A plain scalar the core schema reads as a number that doesn't fit: an integer
/// beyond u128, or a float that overflows or underflows.
fn plain_error(raw: &str) -> Option<String> {
    let body = raw.strip_prefix(['-', '+']).unwrap_or(raw);
    if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        return body
            .parse::<u128>()
            .err()
            .map(|_| format!("{raw} is too large for any integer type"));
    }
    if float_shape(body) {
        return body
            .parse::<f64>()
            .ok()
            .and_then(|f| crate::value::check_float(raw, f).err());
    }
    None
}

/// The YAML 1.2 core schema.
fn plain(raw: &str) -> Value {
    match raw {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        _ => {}
    }
    let (negative, body) = match raw.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, raw.strip_prefix('+').unwrap_or(raw)),
    };
    let digits = |s: &str, ok: fn(u8) -> bool| !s.is_empty() && s.bytes().all(ok);
    let int = |radix: u32, s: &str| {
        u128::from_str_radix(s, radix).ok().map(|m| Value::Int {
            negative: negative && m != 0,
            magnitude: m,
        })
    };
    if digits(body, |b| b.is_ascii_digit())
        && let Some(v) = int(10, body)
    {
        return v;
    }
    if raw == body {
        if let Some(o) = body
            .strip_prefix("0o")
            .filter(|o| digits(o, |b| (b'0'..=b'7').contains(&b)))
            && let Some(v) = int(8, o)
        {
            return v;
        }
        if let Some(h) = body
            .strip_prefix("0x")
            .filter(|h| digits(h, |b| b.is_ascii_hexdigit()))
            && let Some(v) = int(16, h)
        {
            return v;
        }
    }
    let sign = if negative { -1.0 } else { 1.0 };
    match body {
        ".inf" | ".Inf" | ".INF" => return Value::Float(sign * f64::INFINITY),
        ".nan" | ".NaN" | ".NAN" if raw == body => return Value::Float(f64::NAN),
        _ => {}
    }
    if float_shape(body)
        && let Ok(f) = body.parse::<f64>()
        && f.is_finite()
    {
        return Value::Float(sign * f);
    }
    Value::Str(raw.to_string())
}

/// `( \.[0-9]+ | [0-9]+ (\.[0-9]*)? ) ([eE][-+]?[0-9]+)?`, the sign removed.
fn float_shape(body: &str) -> bool {
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let (mantissa, exp) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (body, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((w, f)) => digits(w) && digits(f) && !(w.is_empty() && f.is_empty()),
        None => !mantissa.is_empty() && digits(mantissa),
    };
    let exp_ok = exp.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exp_ok
}

#[cfg(test)]
#[path = "yaml_tests.rs"]
mod tests;
