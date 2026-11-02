//! Markdown with YAML (`---`) or TOML (`+++`) front matter; the rest is the body.

use crate::value::{DataError, Node, Value};

/// The front matter, and the body after its closing fence.
pub fn parse_markdown(text: &str) -> Result<(Node, &str), DataError> {
    let first_end = text.find('\n').unwrap_or(text.len());
    let fence = text[..first_end].trim_end_matches('\r');
    if fence != "---" && fence != "+++" {
        return Err(DataError::at(
            None,
            "the file has no front matter; start it with --- (YAML) or +++ (TOML)",
        ));
    }
    let front_start = (first_end + 1).min(text.len());
    let mut at = front_start;
    let (front, body) = loop {
        if at >= text.len() {
            return Err(DataError::at(
                None,
                format!("the front matter's {fence} is never closed"),
            ));
        }
        let end = text[at..].find('\n').map_or(text.len(), |n| at + n);
        if text[at..end].trim_end_matches('\r') == fence {
            break (&text[front_start..at], &text[(end + 1).min(text.len())..]);
        }
        at = end + 1;
    };
    let shift = |mut e: DataError| {
        if let Some(p) = &mut e.pos {
            p.line += 1;
        }
        e
    };
    let mut node = match fence {
        "---" => yaml(front).map_err(shift)?,
        _ => toml(front).map_err(shift)?,
    };
    node.shift_lines(1);
    match node.value {
        Value::Null => node.value = Value::Map(Vec::new()),
        Value::Map(_) => {}
        _ => return Err(DataError::at(None, "the front matter must be a mapping")),
    }
    Ok((node, body))
}

fn yaml(front: &str) -> Result<Node, DataError> {
    #[cfg(feature = "yaml")]
    return super::yaml::parse_yaml(front);
    #[cfg(not(feature = "yaml"))]
    {
        let _ = front;
        Err(DataError::at(
            None,
            "YAML front matter needs bakelit's `yaml` feature",
        ))
    }
}

fn toml(front: &str) -> Result<Node, DataError> {
    #[cfg(feature = "toml")]
    return super::toml::parse_toml(front);
    #[cfg(not(feature = "toml"))]
    {
        let _ = front;
        Err(DataError::at(
            None,
            "TOML front matter needs bakelit's `toml` feature",
        ))
    }
}

#[cfg(all(test, feature = "yaml", feature = "toml"))]
#[path = "markdown_tests.rs"]
mod tests;
