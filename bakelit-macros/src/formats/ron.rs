//! RON, parsed here so struct and variant names survive (the `ron` crate's `Value`
//! drops them).

use crate::value::{DataError, LineIndex, Node, Value};

pub fn parse_ron(text: &str) -> Result<Node, DataError> {
    let mut p = Parser {
        s: text,
        i: 0,
        idx: LineIndex::new(text),
        depth: 0,
    };
    p.ws()?;
    if p.rest().starts_with("#!") {
        return Err(p.err(p.i, "RON extensions (#![enable(...)]) aren't supported"));
    }
    let node = p.value()?;
    p.ws()?;
    if p.i < text.len() {
        return Err(p.err(
            p.i,
            format!(
                "expected the end of the file, found {:?}",
                p.peek().unwrap()
            ),
        ));
    }
    Ok(node)
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
    idx: LineIndex<'a>,
    depth: usize,
}

type R<T> = Result<T, DataError>;

impl<'a> Parser<'a> {
    fn err(&self, at: usize, msg: impl Into<String>) -> DataError {
        DataError::at(Some(self.idx.pos(at)), msg)
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        let yes = self.peek() == Some(c);
        if yes {
            self.i += c.len_utf8();
        }
        yes
    }

    fn expect(&mut self, c: char) -> R<()> {
        if self.eat(c) {
            return Ok(());
        }
        Err(match self.peek() {
            Some(found) => self.err(self.i, format!("expected {c:?}, found {found:?}")),
            None => self.err(self.i, format!("expected {c:?}, found the end of the file")),
        })
    }

    /// Whitespace and comments (`//` to the end of the line, `/* */` nested).
    fn ws(&mut self) -> R<()> {
        loop {
            let rest = self.rest();
            // RON's whitespace is Rust's Pattern_White_Space.
            let pattern_ws = |c: &char| {
                matches!(
                    c,
                    ' ' | '\t'
                        | '\n'
                        | '\r'
                        | '\u{b}'
                        | '\u{c}'
                        | '\u{85}'
                        | '\u{200e}'
                        | '\u{200f}'
                        | '\u{2028}'
                        | '\u{2029}'
                )
            };
            if let Some(c) = rest.chars().next().filter(pattern_ws) {
                self.i += c.len_utf8();
            } else if rest.starts_with("//") {
                self.i += rest.find('\n').unwrap_or(rest.len());
            } else if rest.starts_with("/*") {
                let start = self.i;
                self.i += 2;
                let mut depth = 1;
                while depth > 0 {
                    let rest = self.rest();
                    if rest.is_empty() {
                        return Err(self.err(start, "a comment that never ends"));
                    } else if rest.starts_with("/*") {
                        depth += 1;
                        self.i += 2;
                    } else if rest.starts_with("*/") {
                        depth -= 1;
                        self.i += 2;
                    } else {
                        self.bump();
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    fn value(&mut self) -> R<Node> {
        self.depth += 1;
        if self.depth > crate::value::MAX_DEPTH {
            let msg = format!("nested deeper than {} levels", crate::value::MAX_DEPTH);
            return Err(self.err(self.i, msg));
        }
        let node = self.value_inner();
        self.depth -= 1;
        node
    }

    fn value_inner(&mut self) -> R<Node> {
        let start = self.i;
        let pos = Some(self.idx.pos(start));
        let rest = self.rest();
        let value = match self.peek() {
            None => return Err(self.err(start, "expected a value, found the end of the file")),
            Some('[') => {
                self.bump();
                Value::List(self.seq(']')?)
            }
            Some('{') => {
                self.bump();
                self.map()?
            }
            Some('(') => self.paren()?,
            Some('"') => Value::Str(self.string()?),
            Some('\'') => Value::Char(self.char()?),
            Some('b')
                if rest.starts_with("b\"") || rest.starts_with("b'") || rest.starts_with("br") =>
            {
                return Err(self.err(start, "byte strings aren't supported"));
            }
            Some('r')
                if rest.starts_with("r\"")
                    || rest.starts_with("r#\"")
                    || rest.starts_with("r##") =>
            {
                Value::Str(self.raw_string()?)
            }
            Some(c) if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => self.number()?,
            Some(c) if c.is_alphabetic() || c == '_' => {
                let name = self.ident()?;
                match name.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "inf" => Value::Float(f64::INFINITY),
                    "NaN" => Value::Float(f64::NAN),
                    "None" => Value::Null,
                    "Some" => {
                        self.ws()?;
                        self.expect('(')?;
                        self.ws()?;
                        let inner = self.value()?;
                        self.ws()?;
                        self.eat(',');
                        self.ws()?;
                        self.expect(')')?;
                        // Kept explicit, so `Some(None)` survives into Option<Option<T>>.
                        let payload = Node::new(Value::List(vec![inner]), pos);
                        return Ok(Node::new(Value::Named(name, Box::new(payload)), pos));
                    }
                    _ => {
                        let before = self.i;
                        self.ws()?;
                        let payload = match self.peek() {
                            Some('(') => {
                                let at = Some(self.idx.pos(self.i));
                                let payload = self.paren()?;
                                Node::new(payload, at)
                            }
                            _ => {
                                self.i = before;
                                Node::new(Value::Null, pos)
                            }
                        };
                        Value::Named(name, Box::new(payload))
                    }
                }
            }
            Some(c) => return Err(self.err(start, format!("expected a value, found {c:?}"))),
        };
        Ok(Node::new(value, pos))
    }

    /// Values up to `close`, with an optional trailing comma.
    fn seq(&mut self, close: char) -> R<Vec<Node>> {
        let mut items = Vec::new();
        loop {
            self.ws()?;
            if self.eat(close) {
                return Ok(items);
            }
            items.push(self.value()?);
            self.ws()?;
            if !self.eat(',') {
                self.ws()?;
                self.expect(close)?;
                return Ok(items);
            }
        }
    }

    fn map(&mut self) -> R<Value> {
        let mut entries: Vec<(String, Node)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        loop {
            self.ws()?;
            if self.eat('}') {
                return Ok(Value::Map(entries));
            }
            let key_at = self.i;
            let key = self.value()?;
            let key = match key.value {
                Value::Str(s) => s,
                Value::Char(c) => c.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Int {
                    negative,
                    magnitude,
                } => format!("{}{magnitude}", if negative { "-" } else { "" }),
                _ => {
                    return Err(self.err(
                        key_at,
                        "a map key must be a string, a number, a char or a bool",
                    ));
                }
            };
            self.ws()?;
            self.expect(':')?;
            self.ws()?;
            let value = self.value()?;
            if !seen.insert(key.clone()) {
                return Err(self.err(key_at, format!("duplicate key {key:?}")));
            }
            entries.push((key, value));
            self.ws()?;
            if !self.eat(',') {
                self.ws()?;
                self.expect('}')?;
                return Ok(Value::Map(entries));
            }
        }
    }

    /// `()`, a tuple `(a, b)`, or struct fields `(a: 1)`.
    fn paren(&mut self) -> R<Value> {
        self.expect('(')?;
        self.ws()?;
        if self.eat(')') {
            return Ok(Value::Null);
        }
        let back = self.i;
        let is_field =
            self.peek().is_some_and(|c| c.is_alphabetic() || c == '_') && self.ident().is_ok() && {
                self.ws()?;
                self.rest().starts_with(':') && !self.rest().starts_with("::")
            };
        self.i = back;
        if !is_field {
            return Ok(Value::List(self.seq(')')?));
        }
        let mut entries: Vec<(String, Node)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        loop {
            self.ws()?;
            if self.eat(')') {
                return Ok(Value::Map(entries));
            }
            let key_at = self.i;
            let key = self.ident()?;
            self.ws()?;
            self.expect(':')?;
            self.ws()?;
            let value = self.value()?;
            if !seen.insert(key.clone()) {
                return Err(self.err(key_at, format!("duplicate key {key:?}")));
            }
            entries.push((key, value));
            self.ws()?;
            if !self.eat(',') {
                self.ws()?;
                self.expect(')')?;
                return Ok(Value::Map(entries));
            }
        }
    }

    /// An identifier, `r#` raw identifiers without the prefix.
    fn ident(&mut self) -> R<String> {
        let start = self.i;
        if self.rest().starts_with("r#") {
            self.i += 2;
        }
        let name_start = self.i;
        match self.peek() {
            Some(c) if c.is_alphabetic() || c == '_' => {}
            _ => {
                self.i = start;
                return Err(self.err(start, "expected a name"));
            }
        }
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.bump();
        }
        Ok(self.s[name_start..self.i].to_string())
    }

    fn number(&mut self) -> R<Value> {
        let start = self.i;
        let negative = self.eat('-');
        if !negative {
            self.eat('+');
        }
        if self.rest().starts_with("inf") {
            self.i += 3;
            return Ok(Value::Float(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }));
        }
        if self.rest().starts_with("NaN") {
            self.i += 3;
            return Ok(Value::Float(f64::NAN));
        }
        let radix = match self.rest().get(..2) {
            Some("0x") => 16,
            Some("0o") => 8,
            Some("0b") => 2,
            _ => 10,
        };
        if radix != 10 {
            self.i += 2;
            let digits_at = self.i;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                self.bump();
            }
            if self.s[digits_at..].starts_with('_') {
                return Err(self.err(
                    start,
                    format!("an unexpected underscore in {}", &self.s[start..self.i]),
                ));
            }
            let digits = self.s[digits_at..self.i].replace('_', "");
            let magnitude = u128::from_str_radix(&digits, radix).map_err(|_| {
                self.err(start, format!("{} isn't a number", &self.s[start..self.i]))
            })?;
            return Ok(Value::Int {
                negative: negative && magnitude != 0,
                magnitude,
            });
        }
        let mut float = false;
        while let Some(c) = self.peek() {
            match c {
                '0'..='9' | '_' => {}
                '.' if !float && !self.rest().starts_with("..") => float = true,
                'e' | 'E' => {
                    float = true;
                    self.bump();
                    if matches!(self.peek(), Some('+' | '-')) {
                        self.bump();
                    }
                    continue;
                }
                _ => break,
            }
            self.bump();
        }
        if self.rest().starts_with("..") {
            return Err(self.err(start, "ranges aren't supported"));
        }
        if let Some(c) = self.peek().filter(|c| c.is_alphabetic()) {
            let written = &self.s[start..self.i];
            let msg = match c {
                'u' | 'i' | 'f' => format!("type suffixes aren't supported; write {written}"),
                _ => format!("{c:?} can't follow a number"),
            };
            return Err(self.err(start, msg));
        }
        let raw = &self.s[start..self.i];
        let raw_body = raw.trim_start_matches(['-', '+']);
        if raw_body.starts_with('_') || ["._"].iter().any(|u| raw_body.contains(u)) {
            return Err(self.err(start, format!("an unexpected underscore in {raw}")));
        }
        let text = raw.replace('_', "");
        let body = text.trim_start_matches(['-', '+']);
        if body.is_empty() || body.starts_with('.') && !float {
            return Err(self.err(start, format!("{raw:?} isn't a number")));
        }
        if float {
            let f: f64 = body
                .parse()
                .map_err(|_| self.err(start, format!("{raw} isn't a number")))?;
            let f = crate::value::check_float(raw, f).map_err(|m| self.err(start, m))?;
            return Ok(Value::Float(if negative { -f } else { f }));
        }
        let magnitude: u128 = body
            .parse()
            .map_err(|_| self.err(start, format!("{raw} is too large for any integer type")))?;
        if negative && magnitude > 1u128 << 127 {
            return Err(self.err(start, format!("{raw} is too large for any integer type")));
        }
        Ok(Value::Int {
            negative: negative && magnitude != 0,
            magnitude,
        })
    }

    fn escape(&mut self, at: usize) -> R<char> {
        let bad = |p: &Self| p.err(at, "an unknown escape");
        Ok(match self.bump().ok_or_else(|| bad(self))? {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'x' => {
                let hex = self.rest().get(..2).ok_or_else(|| bad(self))?;
                let n = u8::from_str_radix(hex, 16).map_err(|_| bad(self))?;
                self.i += 2;
                if n > 0x7f {
                    return Err(bad(self));
                }
                n as char
            }
            'u' => {
                self.expect('{')?;
                let close = self.rest().find('}').ok_or_else(|| bad(self))?;
                let hex = &self.rest()[..close];
                if !hex.starts_with(|c: char| c.is_ascii_hexdigit())
                    || hex.replace('_', "").len() > 6
                {
                    return Err(bad(self));
                }
                let n = u32::from_str_radix(&hex.replace('_', ""), 16).map_err(|_| bad(self))?;
                self.i += close + 1;
                char::from_u32(n).ok_or_else(|| bad(self))?
            }
            _ => return Err(bad(self)),
        })
    }

    fn string(&mut self) -> R<String> {
        let start = self.i;
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let at = self.i;
            match self.bump() {
                None => return Err(self.err(start, "a string that never ends")),
                Some('"') => return Ok(out),
                // A backslash at the end of a line skips to the next text.
                Some('\\') if self.rest().starts_with('\n') || self.rest().starts_with("\r\n") => {
                    let rest = self.rest();
                    self.i += rest.len() - rest.trim_start().len();
                }
                Some('\\') => out.push(self.escape(at)?),
                Some(c) => out.push(c),
            }
        }
    }

    fn raw_string(&mut self) -> R<String> {
        let start = self.i;
        self.expect('r')?;
        let mut hashes = 0;
        while self.eat('#') {
            hashes += 1;
        }
        self.expect('"')?;
        let close = format!("\"{}", "#".repeat(hashes));
        let end = self
            .rest()
            .find(&close)
            .ok_or_else(|| self.err(start, "a string that never ends"))?;
        let out = self.rest()[..end].to_string();
        self.i += end + close.len();
        Ok(out)
    }

    fn char(&mut self) -> R<char> {
        let start = self.i;
        self.expect('\'')?;
        let c = match self.bump() {
            Some('\\') => self.escape(start)?,
            Some(c) => c,
            None => return Err(self.err(start, "a char that never ends")),
        };
        if !self.eat('\'') {
            return Err(self.err(
                start,
                "a char holds one character; use \"...\" for a string",
            ));
        }
        Ok(c)
    }
}

#[cfg(test)]
#[path = "ron_tests.rs"]
mod tests;
