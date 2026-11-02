//! Types derived from the data when the item's type isn't defined.

use crate::items::{Def, DefKind, Field};
use crate::ty::{IntTy, Ty};
use crate::value::{DataError, Node, Pos, Seg, Value, render_path};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::{BTreeMap, HashMap, HashSet};

/// How untyped text (CSV and the like) is inferred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextRule {
    /// CSV and TSV columns: integers, floats and booleans when every cell agrees.
    Guess,
    /// INI, .env and .properties: always `&str`.
    Str,
}

/// What one cell of untyped text looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TextKind {
    /// As `Shape::Int`.
    Int {
        neg: u128,
        pos: u128,
        at: Option<Pos>,
    },
    Float,
    Bool,
    Str,
}

impl TextKind {
    fn at(self, here: Option<Pos>) -> TextKind {
        match self {
            TextKind::Int { neg, pos, .. } => TextKind::Int { neg, pos, at: here },
            other => other,
        }
    }

    fn of(t: &str) -> TextKind {
        if t == "true" || t == "false" {
            return TextKind::Bool;
        }
        let body = t.strip_prefix('-').unwrap_or(t);
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let no_leading_zero = |s: &str| s == "0" || !s.starts_with('0');
        if digits(body) && no_leading_zero(body) {
            if let Ok(m) = body.parse::<u128>() {
                return match t.starts_with('-') {
                    true => TextKind::Int {
                        neg: m,
                        pos: 0,
                        at: None,
                    },
                    false => TextKind::Int {
                        neg: 0,
                        pos: m,
                        at: None,
                    },
                };
            }
            return TextKind::Str;
        }
        let (mantissa, exp) = match body.split_once(['e', 'E']) {
            Some((m, e)) => (m, Some(e)),
            None => (body, None),
        };
        let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, "0"));
        let exp_ok = exp.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)));
        match digits(whole) && no_leading_zero(whole) && digits(frac) && exp_ok {
            true => TextKind::Float,
            false => TextKind::Str,
        }
    }

    fn merge(self, other: TextKind) -> TextKind {
        match (self, other) {
            (
                TextKind::Int { neg: a, pos: b, at },
                TextKind::Int {
                    neg: c,
                    pos: d,
                    at: at2,
                },
            ) => TextKind::Int {
                neg: a.max(c),
                pos: b.max(d),
                at: if c > a || d > b { at2 } else { at },
            },
            (TextKind::Int { .. } | TextKind::Float, TextKind::Int { .. } | TextKind::Float) => {
                TextKind::Float
            }
            (TextKind::Bool, TextKind::Bool) => TextKind::Bool,
            _ => TextKind::Str,
        }
    }
}

pub struct Inferred {
    pub defs: BTreeMap<String, Def>,
    pub items: Vec<syn::Item>,
}

/// What the values seen so far at one place in the data have in common.
#[derive(Clone, Debug)]
enum Shape {
    Unknown,
    /// Where the first null was.
    Null(Option<Pos>),
    Bool,
    /// The largest negative magnitude and the largest value seen, and where the
    /// latest of those was.
    Int {
        neg: u128,
        pos: u128,
        at: Option<Pos>,
    },
    Float,
    Str,
    Char,
    /// The element shape, and where the first list was.
    List(Box<Shape>, Option<Pos>),
    Map(Fields),
    Option(Box<Shape>),
    /// CSV cells, when guessing.
    Text(TextKind),
}

/// Fields in first-seen order, and whether every mapping had them.
#[derive(Clone, Debug, Default)]
struct Fields {
    list: Vec<(String, Shape, bool)>,
    index: HashMap<String, usize>,
}

impl Shape {
    fn plural(&self) -> &'static str {
        match self {
            Shape::Unknown | Shape::Null(_) => "null",
            Shape::Bool => "booleans",
            Shape::Int { .. } => "integers",
            Shape::Float => "floats",
            Shape::Str => "strings",
            Shape::Char => "chars",
            Shape::List(..) => "a list",
            Shape::Map(_) => "a mapping",
            Shape::Option(inner) => inner.plural(),
            Shape::Text(_) => "text",
        }
    }
}

const ADVICE: &str = "write the types yourself";

struct Infer {
    text: TextRule,
    /// Give the root struct a `body` field (Markdown).
    body: bool,
    path: Vec<Seg>,
    defs: BTreeMap<String, Def>,
    /// Parents before children: a struct reserves its slot before its fields.
    items: Vec<Option<syn::Item>>,
}

impl Infer {
    fn err(&self, n: Option<&Node>, msg: String) -> DataError {
        DataError {
            pos: n.and_then(|n| n.pos),
            path: render_path(&self.path),
            msg,
        }
    }

    fn err_at(&self, pos: Option<Pos>, msg: String) -> DataError {
        DataError {
            pos,
            path: render_path(&self.path),
            msg,
        }
    }

    fn at<R>(&mut self, seg: Seg, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(seg);
        let r = f(self);
        self.path.pop();
        r
    }

    /// The shape `s` widened by the value `n`.
    fn add(&mut self, s: Shape, n: &Node) -> Result<Shape, DataError> {
        // RON's explicit `Some(x)`.
        if let Value::Named(name, payload) = &n.value
            && name == "Some"
            && let Value::List(l) = &payload.value
            && l.len() == 1
        {
            let inner = match s {
                Shape::Option(inner) => *inner,
                Shape::Unknown | Shape::Null(_) => Shape::Unknown,
                other => other,
            };
            return Ok(Shape::Option(Box::new(self.add(inner, &l[0])?)));
        }
        let guess = self.text == TextRule::Guess;
        // An empty CSV cell is a missing value.
        let null = match &n.value {
            Value::Null => true,
            Value::Text(t) => guess && t.is_empty(),
            _ => false,
        };
        if let Shape::Option(inner) = s {
            return match null {
                true => Ok(Shape::Option(inner)),
                false => Ok(Shape::Option(Box::new(self.add(*inner, n)?))),
            };
        }
        if null {
            return Ok(match s {
                Shape::Unknown => Shape::Null(n.pos),
                Shape::Null(at) => Shape::Null(at),
                other => Shape::Option(Box::new(other)),
            });
        }
        if let Shape::Null(_) = s {
            return Ok(Shape::Option(Box::new(self.add(Shape::Unknown, n)?)));
        }
        let mismatch = |me: &Self, s: &Shape| {
            me.err(
                Some(n),
                format!("found {} after {}; {ADVICE}", n.describe(), s.plural()),
            )
        };
        if let (Value::Text(t), true) = (&n.value, guess) {
            return match s {
                Shape::Unknown => Ok(Shape::Text(TextKind::of(t).at(n.pos))),
                Shape::Text(k) => Ok(Shape::Text(k.merge(TextKind::of(t).at(n.pos)))),
                other => Err(mismatch(self, &other)),
            };
        }
        Ok(match (&n.value, s) {
            (Value::Bool(_), Shape::Unknown | Shape::Bool) => Shape::Bool,
            (
                Value::Int {
                    negative,
                    magnitude,
                },
                Shape::Unknown,
            ) => int_shape(
                Shape::Int {
                    neg: 0,
                    pos: 0,
                    at: None,
                },
                *negative,
                *magnitude,
                n.pos,
            ),
            (
                Value::Int {
                    negative,
                    magnitude,
                },
                s @ Shape::Int { .. },
            ) => int_shape(s, *negative, *magnitude, n.pos),
            (Value::Int { .. } | Value::Float(_), Shape::Float)
            | (Value::Float(_), Shape::Unknown | Shape::Int { .. }) => Shape::Float,
            (Value::Str(_) | Value::Text(_), Shape::Unknown | Shape::Str) => Shape::Str,
            (Value::Char(_), Shape::Unknown | Shape::Char) => Shape::Char,
            (Value::List(items), s @ (Shape::Unknown | Shape::List(..))) => {
                let (mut elem, at) = match s {
                    Shape::List(e, at) => (*e, at),
                    _ => (Shape::Unknown, n.pos),
                };
                for (i, item) in items.iter().enumerate() {
                    elem = self.at(Seg::Index(i), |me| me.add(elem, item))?;
                }
                Shape::List(Box::new(elem), at)
            }
            (Value::Map(entries), s @ (Shape::Unknown | Shape::Map(_))) => {
                let first = matches!(s, Shape::Unknown);
                let mut fields = match s {
                    Shape::Map(f) => f,
                    _ => Fields::default(),
                };
                if !first {
                    let present: HashSet<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
                    for f in fields.list.iter_mut() {
                        if !present.contains(f.0.as_str()) {
                            f.2 = false;
                        }
                    }
                }
                for (k, v) in entries {
                    match fields.index.get(k) {
                        Some(&i) => {
                            let old = std::mem::replace(&mut fields.list[i].1, Shape::Unknown);
                            fields.list[i].1 = self.at(Seg::Key(k.clone()), |me| me.add(old, v))?;
                        }
                        None => {
                            let shape =
                                self.at(Seg::Key(k.clone()), |me| me.add(Shape::Unknown, v))?;
                            fields.index.insert(k.clone(), fields.list.len());
                            fields.list.push((k.clone(), shape, first));
                        }
                    }
                }
                Shape::Map(fields)
            }
            (Value::Named(..), _) => {
                return Err(self.err(
                    Some(n),
                    format!("RON's named values can't be inferred; {ADVICE}"),
                ));
            }
            (_, s) => return Err(mismatch(self, &s)),
        })
    }

    /// The Rust type for a shape, generating structs named from `name`.
    fn ty(&mut self, s: &Shape, name: &str) -> Result<Ty, DataError> {
        Ok(match s {
            Shape::Unknown => unreachable!("lists check their element"),
            Shape::Null(at) => {
                let msg = format!("only null here; can't infer a type, so {ADVICE}");
                return Err(self.err_at(*at, msg));
            }
            Shape::Bool => Ty::Bool,
            Shape::Float => Ty::F64,
            Shape::Str => Ty::Str,
            Shape::Char => Ty::Char,
            Shape::Text(TextKind::Float) => Ty::F64,
            Shape::Text(TextKind::Bool) => Ty::Bool,
            Shape::Text(TextKind::Str) => Ty::Str,
            Shape::Int { neg, pos, at } | Shape::Text(TextKind::Int { neg, pos, at }) => {
                if *neg > 1u128 << 63 {
                    return Err(self.err_at(*at, format!("-{neg} doesn't fit i64; {ADVICE}")));
                }
                if *pos > u64::MAX as u128 {
                    return Err(self.err_at(*at, format!("{pos} doesn't fit u64; {ADVICE}")));
                }
                if *pos > i64::MAX as u128 && *neg > 0 {
                    return Err(self.err_at(
                        *at,
                        format!("values from -{neg} to {pos} don't fit one integer type; {ADVICE}"),
                    ));
                }
                Ty::Int(if *pos > i64::MAX as u128 {
                    IntTy::U64
                } else {
                    IntTy::I64
                })
            }
            Shape::Option(inner) => Ty::Option(Box::new(self.ty(inner, name)?)),
            Shape::List(e, at) => match **e {
                Shape::Unknown => {
                    let msg = format!("an empty list; can't infer its element type, so {ADVICE}");
                    return Err(self.err_at(*at, msg));
                }
                _ => Ty::Slice(Box::new(self.at(Seg::Each, |me| me.ty(e, name))?)),
            },
            Shape::Map(fields) => {
                let slot = self.items.len();
                self.items.push(None);
                let mut out: Vec<Field> = Vec::new();
                let mut tokens = Vec::new();
                let mut seen: HashMap<String, &str> = HashMap::new();
                for (key, shape, always) in &fields.list {
                    let Some(snake) = field_name(key)
                        .filter(|f| !["self", "super", "crate"].contains(&f.as_str()))
                    else {
                        return Err(self.err(None, format!(
                            "the key {key:?} can't be a field name; {ADVICE} and use #[bake(rename = {key:?})]"
                        )));
                    };
                    if let Some(other) = seen.insert(snake.clone(), key) {
                        return Err(self.err(None, format!(
                            "the keys {other:?} and {key:?} both become the field {snake}; {ADVICE} and use #[bake(rename = \"...\")]"
                        )));
                    }
                    let mut ty = self.at(Seg::Key(key.clone()), |me| {
                        me.ty(shape, &format!("{name}{}", pascal(&snake)))
                    })?;
                    if !always && !matches!(ty, Ty::Option(_)) {
                        ty = Ty::Option(Box::new(ty));
                    }
                    let ident = match is_keyword(&snake) {
                        true => syn::Ident::new_raw(&snake, Span::call_site()),
                        false => syn::Ident::new(&snake, Span::call_site()),
                    };
                    let tt = ty_tokens(&ty);
                    tokens.push(quote!(pub #ident: #tt));
                    out.push(Field {
                        ident,
                        key: key.clone(),
                        ty,
                        body: false,
                    });
                }
                if self.body && self.path.is_empty() {
                    self.body = false;
                    if out.iter().any(|f| f.ident == "body") {
                        return Err(self.err(None, format!(
                            "the front matter has a key \"body\", which the Markdown body would use; {ADVICE}"
                        )));
                    }
                    let ident = syn::Ident::new("body", Span::call_site());
                    tokens.push(quote!(pub #ident: &'static str));
                    out.push(Field {
                        ident,
                        key: String::new(),
                        ty: Ty::Str,
                        body: true,
                    });
                }
                if self.defs.contains_key(name) {
                    return Err(self.err(
                        None,
                        format!("the inferred type name {name} is used twice; {ADVICE}"),
                    ));
                }
                let ident = format_ident!("{name}");
                let item: syn::Item = syn::parse_quote! {
                    #[derive(
                        ::core::fmt::Debug,
                        ::core::clone::Clone,
                        ::core::marker::Copy,
                        ::core::cmp::PartialEq
                    )]
                    pub struct #ident { #(#tokens),* }
                };
                self.defs.insert(
                    name.to_string(),
                    Def {
                        ident,
                        allow_unknown: false,
                        kind: DefKind::Struct(out),
                    },
                );
                self.items[slot] = Some(item);
                Ty::Named(name.to_string())
            }
        })
    }
}

fn int_shape(s: Shape, negative: bool, magnitude: u128, here: Option<Pos>) -> Shape {
    let Shape::Int { neg, pos, at } = s else {
        unreachable!()
    };
    match negative {
        true if magnitude > neg => Shape::Int {
            neg: magnitude,
            pos,
            at: here,
        },
        false if magnitude > pos => Shape::Int {
            neg,
            pos: magnitude,
            at: here,
        },
        _ => Shape::Int { neg, pos, at },
    }
}

/// The one type name to infer in an item's type, `None` if every name is defined.
pub fn inferable(item_ty: &Ty, existing: &BTreeMap<String, Def>) -> Result<Option<String>, String> {
    let mut names = Vec::new();
    item_ty.named(&mut names);
    let undefined: Vec<&String> = names
        .iter()
        .filter(|n| !existing.contains_key(*n))
        .collect();
    if undefined.is_empty() {
        return Ok(None);
    }
    if let Some(defined) = names.iter().find(|n| existing.contains_key(*n)) {
        return Err(format!(
            "`{defined}` is defined here but `{}` isn't; define every type, or none to infer them",
            undefined[0]
        ));
    }
    match undefined.as_slice() {
        [name] => Ok(Some(name.to_string())),
        _ => Err(format!(
            "inference needs one type name in the item's type, found {}",
            undefined
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join(" and ")
        )),
    }
}

/// Infers the types for a value whose item type names a type the invocation doesn't
/// define. `existing` are the types defined so far.
pub fn infer(
    node: &Node,
    item_ty: &Ty,
    existing: &BTreeMap<String, Def>,
    text: TextRule,
    body: bool,
) -> Result<Inferred, DataError> {
    let name = match inferable(item_ty, existing) {
        Ok(Some(name)) => name,
        Ok(None) => unreachable!("every type is defined"),
        Err(msg) => {
            return Err(DataError {
                pos: None,
                path: String::new(),
                msg,
            });
        }
    };
    let name = &name;
    let root = |msg: String| DataError {
        pos: None,
        path: "$".into(),
        msg,
    };
    let mut me = Infer {
        text,
        body,
        path: Vec::new(),
        defs: BTreeMap::new(),
        items: Vec::new(),
    };
    let shape = me.add(Shape::Unknown, node)?;
    let ty = me.ty(&shape, name)?;
    if ty != *item_ty {
        return Err(root(format!(
            "the data's type is {}, not {}; write that type, or define {name}",
            ty.name(),
            item_ty.name()
        )));
    }
    Ok(Inferred {
        defs: me.defs,
        items: me.items.into_iter().flatten().collect(),
    })
}

/// A key as a snake_case field name: separators and camelCase humps become `_`.
pub fn field_name(key: &str) -> Option<String> {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in key.chars() {
        match c {
            '-' | ' ' | '.' | '_' => out.push('_'),
            c if c.is_ascii_uppercase() => {
                if prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit()) {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            }
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => return None,
        }
        prev = Some(c);
    }
    let valid = !out.is_empty() && out != "_" && !out.starts_with(|c: char| c.is_ascii_digit());
    valid.then_some(out)
}

fn pascal(snake: &str) -> String {
    snake
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| w[..1].to_ascii_uppercase() + &w[1..])
        .collect()
}

fn is_keyword(s: &str) -> bool {
    const KEYWORDS: [&str; 39] = [
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use",
        "where", "while", "abstract", "become", "box", "do", "final",
    ];
    const MORE: [&str; 8] = [
        "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    KEYWORDS.contains(&s) || MORE.contains(&s) || s == "gen"
}

/// The type as Rust source.
pub fn ty_tokens(ty: &Ty) -> TokenStream {
    match ty {
        Ty::Bool => quote!(::core::primitive::bool),
        Ty::Char => quote!(::core::primitive::char),
        Ty::Int(i) => {
            let t = format_ident!("{}", i.name());
            quote!(::core::primitive::#t)
        }
        Ty::F32 => quote!(::core::primitive::f32),
        Ty::F64 => quote!(::core::primitive::f64),
        Ty::Str => quote!(&'static ::core::primitive::str),
        Ty::Unit => quote!(()),
        Ty::Option(t) => {
            let t = ty_tokens(t);
            quote!(::core::option::Option<#t>)
        }
        Ty::Slice(t) => {
            let t = ty_tokens(t);
            quote!(&'static [#t])
        }
        Ty::Array(t, n) => {
            let t = ty_tokens(t);
            quote!([#t; #n])
        }
        Ty::Tuple(ts) => {
            let ts: Vec<_> = ts.iter().map(ty_tokens).collect();
            quote!((#(#ts),*))
        }
        Ty::Named(n) => {
            let t = format_ident!("{n}");
            quote!(#t)
        }
    }
}

#[cfg(all(test, feature = "json"))]
#[path = "infer_tests.rs"]
mod tests;
