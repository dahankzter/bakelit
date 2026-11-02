//! INI: top-level keys, and sections as nested mappings, as `rust-ini` reads them,
//! with backslashes kept as written (Windows paths) and double quotes around a value
//! removed. A `;` after a value is part of it.

use crate::value::{DataError, Node, Pos, Value};

pub fn parse_ini(text: &str) -> Result<Node, DataError> {
    let options = ini::ParseOption {
        enabled_escape: false,
        ..Default::default()
    };
    let ini = ini::Ini::load_from_str_opt(text, options).map_err(|e| {
        DataError::at(
            Some(Pos {
                line: e.line,
                col: e.col.max(1),
            }),
            e.msg.to_string(),
        )
    })?;
    let mut top: Vec<(String, Node)> = Vec::new();
    let mut sections: Vec<(String, Node)> = Vec::new();
    for (name, props) in ini.iter() {
        let mut entries: Vec<(String, Node)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (k, v) in props.iter() {
            // rust-ini glues a line without "=" onto the next key.
            if let Some((line, _)) = k.split_once('\n') {
                let msg = format!("the line {:?} has no \"=\"", line.trim());
                return Err(DataError::at(None, msg));
            }
            if !seen.insert(k) {
                let at = name.map_or(String::new(), |n| format!(" in [{n}]"));
                return Err(DataError::at(None, format!("duplicate key {k:?}{at}")));
            }
            entries.push((k.to_string(), Node::new(Value::Text(v.to_string()), None)));
        }
        match name {
            None => top.extend(entries),
            Some(n) if sections.iter().any(|(s, _)| s == n) => {
                return Err(DataError::at(
                    None,
                    format!("the section [{n}] appears twice"),
                ));
            }
            Some(n) => sections.push((n.to_string(), Node::new(Value::Map(entries), None))),
        }
    }
    for (name, _) in &sections {
        if top.iter().any(|(k, _)| k == name) {
            return Err(DataError::at(
                None,
                format!("the key {name:?} and the section [{name}] have the same name"),
            ));
        }
    }
    top.extend(sections);
    Ok(Node::new(Value::Map(top), None))
}
