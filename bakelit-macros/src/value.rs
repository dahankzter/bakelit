//! The value model every format parses into.

/// A 1-based line and column (in characters).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub value: Value,
    pub pos: Option<Pos>,
}

/// Each format builds only some variants (TOML has no null, only RON names).
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Every integer from `i128::MIN` to `u128::MAX`, exactly. Zero is never negative.
    Int {
        negative: bool,
        magnitude: u128,
    },
    Float(f64),
    /// A string from a format with typed values.
    Str(String),
    /// Untyped text (CSV, INI, .env, .properties), parsed into the target type.
    Text(String),
    Char(char),
    List(Vec<Node>),
    /// Unique keys, in file order.
    Map(Vec<(String, Node)>),
    /// RON's `Name`, `Name(..)` and `Name(f: ..)`: the payload is `Null`, a `List` or a `Map`.
    Named(String, Box<Node>),
}

impl Node {
    pub fn new(value: Value, pos: Option<Pos>) -> Node {
        Node { value, pos }
    }

    /// Moves every position down by `by` lines (for text parsed out of a larger file).
    #[allow(dead_code)] // JSON Lines and Markdown.
    pub fn shift_lines(&mut self, by: usize) {
        if let Some(p) = &mut self.pos {
            p.line += by;
        }
        match &mut self.value {
            Value::List(items) => items.iter_mut().for_each(|c| c.shift_lines(by)),
            Value::Map(entries) => entries.iter_mut().for_each(|(_, c)| c.shift_lines(by)),
            Value::Named(_, payload) => payload.shift_lines(by),
            _ => {}
        }
    }

    /// How an error message names this value: `found <describe>`.
    pub fn describe(&self) -> String {
        match &self.value {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int {
                negative,
                magnitude,
            } => format!("{}{magnitude}", if *negative { "-" } else { "" }),
            Value::Float(f) => format!("{f:?}"),
            Value::Text(s) if s.is_empty() => "an empty field".into(),
            Value::Str(s) | Value::Text(s) => {
                let short: String = s.chars().take(40).collect();
                let more = if short.len() < s.len() { "..." } else { "" };
                format!("{:?}{more}", short)
            }
            Value::Char(c) => format!("{c:?}"),
            Value::List(_) => "a list".into(),
            Value::Map(_) => "a mapping".into(),
            Value::Named(name, payload) => match payload.value {
                Value::Null => name.clone(),
                _ => format!("{name}(...)"),
            },
        }
    }
}

/// The deepest nesting any format may produce; deeper data would risk the
/// compiler's stack.
pub const MAX_DEPTH: usize = 128;

/// A float read from `raw`: overflow to infinity and underflow to zero are errors.
#[allow(dead_code)] // Not every format reads floats from text.
pub fn check_float(raw: &str, f: f64) -> Result<f64, String> {
    if f.is_infinite() {
        return Err(format!("{raw} is out of range for f64"));
    }
    let mantissa = raw.split(['e', 'E']).next().unwrap_or(raw);
    if f == 0.0 && mantissa.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        return Err(format!("{raw} is too small for f64; it would be 0"));
    }
    Ok(f)
}

impl Node {
    /// How deeply lists, mappings and named values nest (a scalar is 1).
    pub fn depth(&self) -> usize {
        1 + match &self.value {
            Value::List(items) => items.iter().map(Node::depth).max().unwrap_or(0),
            Value::Map(entries) => entries.iter().map(|(_, n)| n.depth()).max().unwrap_or(0),
            Value::Named(_, payload) => payload.depth(),
            _ => 0,
        }
    }
}

/// One step of a data path.
#[derive(Clone, Debug)]
pub enum Seg {
    Key(String),
    Index(usize),
    /// Every element of a list (inference reports per list, not per element).
    Each,
}

/// `$`, `$.servers[2].port`, `$["odd key"]`.
pub fn render_path(path: &[Seg]) -> String {
    let mut out = String::from("$");
    for seg in path {
        match seg {
            Seg::Index(i) => out.push_str(&format!("[{i}]")),
            Seg::Each => out.push_str("[]"),
            Seg::Key(k) if is_plain_key(k) => {
                out.push('.');
                out.push_str(k);
            }
            Seg::Key(k) => out.push_str(&format!("[{k:?}]")),
        }
    }
    out
}

fn is_plain_key(k: &str) -> bool {
    let mut chars = k.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A problem in the data, rendered as `<file>:<line>:<col>: <path>: <msg>`.
#[derive(Clone, Debug)]
pub struct DataError {
    pub pos: Option<Pos>,
    pub path: String,
    pub msg: String,
}

impl DataError {
    /// An error about the file as a whole, or at a position before any path exists.
    pub fn at(pos: Option<Pos>, msg: impl Into<String>) -> DataError {
        DataError {
            pos,
            path: String::new(),
            msg: msg.into(),
        }
    }

    pub fn render(&self, file: &str) -> String {
        let mut out = file.to_string();
        if let Some(p) = self.pos {
            out.push_str(&format!(":{}:{}", p.line, p.col));
        }
        if !self.path.is_empty() {
            out.push_str(": ");
            out.push_str(&self.path);
        }
        out.push_str(": ");
        out.push_str(&self.msg);
        out
    }
}

/// Byte offsets to line and column.
#[allow(dead_code)] // Not every format needs it.
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

#[allow(dead_code)]
impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let starts = core::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        LineIndex { text, starts }
    }

    pub fn pos(&self, offset: usize) -> Pos {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line];
        let col = self
            .text
            .get(start..offset)
            .map_or(offset - start, |s| s.chars().count())
            + 1;
        Pos {
            line: line + 1,
            col,
        }
    }
}
