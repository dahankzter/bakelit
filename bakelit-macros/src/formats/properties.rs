//! Java `.properties`, as the `java-properties` crate reads them (in UTF-8).

use crate::value::{DataError, Node, Pos, Value};
use java_properties::{LineContent, PropertiesIter};

pub fn parse_properties(text: &str) -> Result<Node, DataError> {
    let mut entries: Vec<(String, Node)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in PropertiesIter::new_with_encoding(text.as_bytes(), encoding_rs::UTF_8) {
        let line = line.map_err(|e| {
            DataError::at(
                e.line_number().map(|l| Pos { line: l, col: 1 }),
                e.to_string(),
            )
        })?;
        let pos = Some(Pos {
            line: line.line_number(),
            col: 1,
        });
        if let LineContent::KVPair(k, v) = line.consume_content() {
            if !seen.insert(k.clone()) {
                return Err(DataError::at(pos, format!("duplicate key {k:?}")));
            }
            entries.push((k, Node::new(Value::Text(v), pos)));
        }
    }
    Ok(Node::new(Value::Map(entries), None))
}
