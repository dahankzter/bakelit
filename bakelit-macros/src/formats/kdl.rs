//! KDL 2: nodes as keys of a mapping.
//!
//! A node with one argument is that value, with several a list; with properties
//! and/or children, a mapping; with children all named `-`, a list of their values;
//! with nothing, null. Siblings with one name are a list of their values. A document
//! whose nodes are all named `-` is a list too.

use crate::value::{DataError, LineIndex, MAX_DEPTH, Node, Pos, Value, check_float};
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use std::collections::{HashMap, HashSet};

pub fn parse_kdl(text: &str) -> Result<Node, DataError> {
    let idx = LineIndex::new(text);
    // The kdl crate recurses per nesting level, and overflows the stack long before
    // any limit of its own.
    if let Some(at) = too_deep(text) {
        return Err(DataError::at(
            Some(idx.pos(at)),
            format!("nested deeper than {MAX_DEPTH} levels"),
        ));
    }
    let doc = match KdlDocument::parse_v2(text) {
        Ok(doc) => doc,
        Err(e) => {
            if KdlDocument::parse_v1(text).is_ok() {
                return Err(DataError::at(
                    None,
                    "this looks like KDL 1; bakelit reads KDL 2",
                ));
            }
            let d = e.diagnostics.first();
            let pos = d.map(|d| idx.pos(d.span.offset()));
            let msg = d
                .and_then(|d| d.message.clone().or_else(|| d.label.clone()))
                .unwrap_or_else(|| "invalid KDL".into());
            return Err(DataError::at(pos, msg));
        }
    };
    let kdl = Kdl { text, idx };
    let value = match kdl.dashes(doc.nodes())? {
        Some(list) => list,
        None => kdl.document(&doc)?,
    };
    Ok(Node::new(value, Some(Pos { line: 1, col: 1 })))
}

/// The offset of the first `{` beyond `MAX_DEPTH` levels, outside strings and comments.
fn too_deep(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let (mut i, mut depth) = (0, 0usize);
    while i < b.len() {
        match b[i] {
            b'{' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Some(i);
                }
            }
            b'}' => depth = depth.saturating_sub(1),
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'#' if b.get(i + 1).is_some_and(|c| *c == b'#' || *c == b'"') => {
                // A raw string: #"..."#, ##"..."##.
                let hashes = b[i..].iter().take_while(|c| **c == b'#').count();
                let close: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat_n(b'#', hashes))
                    .collect();
                i += hashes + 1;
                while i < b.len() && !b[i..].starts_with(&close) {
                    i += 1;
                }
                i += close.len() - 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut nest = 1;
                i += 2;
                while i < b.len() && nest > 0 {
                    if b[i..].starts_with(b"/*") {
                        nest += 1;
                        i += 1;
                    } else if b[i..].starts_with(b"*/") {
                        nest -= 1;
                        i += 1;
                    }
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

struct Kdl<'a> {
    text: &'a str,
    idx: LineIndex<'a>,
}

impl Kdl<'_> {
    fn at(&self, offset: usize) -> Option<Pos> {
        Some(self.idx.pos(offset))
    }

    /// The values of nodes all named `-`, as a list; `None` if no node is named `-`.
    fn dashes(&self, nodes: &[KdlNode]) -> Result<Option<Value>, DataError> {
        if !nodes.iter().any(|n| n.name().value() == "-") {
            return Ok(None);
        }
        if let Some(other) = nodes.iter().find(|n| n.name().value() != "-") {
            return Err(DataError::at(
                self.at(other.span().offset()),
                r#"a list's children are all named "-""#,
            ));
        }
        let items = nodes
            .iter()
            .map(|n| self.node(n))
            .collect::<Result<_, _>>()?;
        Ok(Some(Value::List(items)))
    }

    fn document(&self, doc: &KdlDocument) -> Result<Value, DataError> {
        let mut groups: Vec<(String, Vec<Node>)> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for n in doc.nodes() {
            let value = self.node(n)?;
            let name = n.name().value().to_string();
            match index.get(&name) {
                Some(&i) => groups[i].1.push(value),
                None => {
                    index.insert(name.clone(), groups.len());
                    groups.push((name, vec![value]));
                }
            }
        }
        let entries = groups.into_iter().map(|(k, mut nodes)| {
            let node = match nodes.len() {
                1 => nodes.pop().unwrap(),
                _ => {
                    let pos = nodes[0].pos;
                    Node::new(Value::List(nodes), pos)
                }
            };
            (k, node)
        });
        Ok(Value::Map(entries.collect()))
    }

    fn node(&self, n: &KdlNode) -> Result<Node, DataError> {
        let pos = self.at(n.span().offset());
        let err = |msg: String| DataError::at(pos, msg);
        if let Some(t) = n.ty() {
            return Err(err(format!(
                "type annotations like ({}) aren't supported",
                t.value()
            )));
        }
        let (args, props): (Vec<&KdlEntry>, Vec<&KdlEntry>) =
            n.entries().iter().partition(|e| e.name().is_none());
        if !args.is_empty() && (!props.is_empty() || n.children().is_some()) {
            return Err(err(
                "a node with arguments can't also have properties or children".into(),
            ));
        }
        let value = match (args.as_slice(), n.children()) {
            ([one], _) => return self.entry(one),
            ([_, ..], _) => Value::List(
                args.iter()
                    .map(|e| self.entry(e))
                    .collect::<Result<_, _>>()?,
            ),
            ([], children) => {
                let list = match children {
                    Some(c) => self.dashes(c.nodes())?,
                    None => None,
                };
                match list {
                    Some(_) if !props.is_empty() => {
                        return Err(err(r#"a list's children are all named "-""#.into()));
                    }
                    Some(list) => list,
                    None if props.is_empty() && children.is_none() => Value::Null,
                    None => {
                        let mut entries: Vec<(String, Node)> = Vec::new();
                        let mut seen = HashSet::new();
                        for p in &props {
                            let key = p.name().unwrap().value().to_string();
                            if !seen.insert(key.clone()) {
                                let at = self.at(p.span().offset());
                                return Err(DataError::at(at, format!("duplicate key {key:?}")));
                            }
                            entries.push((key, self.entry(p)?));
                        }
                        if let Some(children) = children {
                            let Value::Map(kids) = self.document(children)? else {
                                unreachable!()
                            };
                            for (k, v) in kids {
                                if !seen.insert(k.clone()) {
                                    let msg =
                                        format!("the key {k:?} is both a property and a child");
                                    return Err(DataError::at(v.pos, msg));
                                }
                                entries.push((k, v));
                            }
                        }
                        Value::Map(entries)
                    }
                }
            }
        };
        Ok(Node::new(value, pos))
    }

    /// The source text of an entry's value (after `name =` for a property), and where
    /// it starts.
    fn value_text(&self, e: &KdlEntry) -> (&str, usize) {
        let start = e.span().offset();
        let end = (start + e.span().len()).min(self.text.len());
        let whole = self.text.get(start..end).unwrap_or("");
        match e.name() {
            Some(_) => match whole.find('=') {
                Some(eq) => {
                    let after = &whole[eq + 1..];
                    let value = after.trim_start();
                    (value, start + eq + 1 + after.len() - value.len())
                }
                None => (whole, start),
            },
            None => (whole, start),
        }
    }

    fn entry(&self, e: &KdlEntry) -> Result<Node, DataError> {
        let (raw, value_at) = self.value_text(e);
        let pos = self.at(value_at);
        if let Some(t) = e.ty() {
            return Err(DataError::at(
                pos,
                format!("type annotations like ({}) aren't supported", t.value()),
            ));
        }
        let value = match e.value() {
            KdlValue::String(s) => Value::Str(s.clone()),
            KdlValue::Integer(i) => Value::Int {
                negative: *i < 0,
                magnitude: i.unsigned_abs(),
            },
            // #inf, #-inf and #nan are written as keywords; a number that overflows isn't.
            KdlValue::Float(f) if raw.starts_with('#') => Value::Float(*f),
            KdlValue::Float(f) => {
                let raw = raw.trim_end().replace('_', "");
                Value::Float(check_float(&raw, *f).map_err(|m| DataError::at(pos, m))?)
            }
            KdlValue::Bool(b) => Value::Bool(*b),
            KdlValue::Null => Value::Null,
        };
        Ok(Node::new(value, pos))
    }
}

#[cfg(test)]
#[path = "kdl_tests.rs"]
mod tests;
