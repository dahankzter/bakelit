//! Checking a value against a type, and writing it as Rust tokens.

use crate::items::{Def, DefKind, Field, VariantKind};
use crate::naming::did_you_mean;
use crate::ty::{IntTy, Ty};
use crate::value::{DataError, Node, Seg, Value, render_path};
use proc_macro2::{Literal, Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeMap;

pub struct Ctx<'a> {
    defs: &'a BTreeMap<String, Def>,
    path: Vec<Seg>,
    /// The Markdown body, for `#[bake(body)]` fields.
    pub body: Option<&'a str>,
}

type Res = Result<TokenStream, DataError>;

impl<'a> Ctx<'a> {
    pub fn new(defs: &'a BTreeMap<String, Def>) -> Self {
        Ctx {
            defs,
            path: Vec::new(),
            body: None,
        }
    }

    fn err(&self, n: &Node, msg: impl Into<String>) -> DataError {
        DataError {
            pos: n.pos,
            path: render_path(&self.path),
            msg: msg.into(),
        }
    }

    fn mismatch(&self, n: &Node, ty: &Ty) -> DataError {
        self.err(n, format!("expected {}, found {}", ty.name(), n.describe()))
    }

    fn at<R>(&mut self, seg: Seg, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(seg);
        let r = f(self);
        self.path.pop();
        r
    }
}

pub fn convert(n: &Node, ty: &Ty, ctx: &mut Ctx) -> Res {
    match ty {
        Ty::Option(inner) => match &n.value {
            // RON's explicit `Some(x)`.
            Value::Named(name, payload)
                if name == "Some" && matches!(&payload.value, Value::List(l) if l.len() == 1) =>
            {
                let Value::List(l) = &payload.value else {
                    unreachable!()
                };
                let v = convert(&l[0], inner, ctx)?;
                Ok(quote!(::core::option::Option::Some(#v)))
            }
            Value::Null => Ok(quote!(::core::option::Option::None)),
            Value::Text(t) if t.is_empty() => Ok(quote!(::core::option::Option::None)),
            _ => {
                let v = convert(n, inner, ctx)?;
                Ok(quote!(::core::option::Option::Some(#v)))
            }
        },
        Ty::Bool => match &n.value {
            Value::Bool(b) => Ok(quote!(#b)),
            Value::Text(t) if t == "true" || t == "false" => {
                let b = t == "true";
                Ok(quote!(#b))
            }
            Value::Str(s) | Value::Text(s)
                if ["true", "false", "yes", "no", "on", "off", "y", "n"]
                    .contains(&s.to_ascii_lowercase().as_str()) =>
            {
                Err(ctx.err(
                    n,
                    format!("expected bool, found {}; write true or false", n.describe()),
                ))
            }
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::Int(it) => match &n.value {
            Value::Int {
                negative,
                magnitude,
            } => int_lit(*negative, *magnitude, *it).ok_or_else(|| ctx.mismatch(n, ty)),
            Value::Text(t) => text_int(t, *it).map_err(|hint| {
                ctx.err(
                    n,
                    format!("expected {}, found {}{hint}", it.name(), n.describe()),
                )
            }),
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::F32 | Ty::F64 => {
            let f32 = *ty == Ty::F32;
            match &n.value {
                Value::Float(f) => float_lit(*f, f32).map_err(|m| ctx.err(n, m)),
                Value::Int {
                    negative,
                    magnitude,
                } => int_to_float(*negative, *magnitude, f32).map_err(|m| ctx.err(n, m)),
                Value::Text(t) => text_float(t, f32).map_err(|hint| match hint {
                    TextFloat::Hint(h) => ctx.err(
                        n,
                        format!("expected {}, found {}{h}", ty.name(), n.describe()),
                    ),
                    TextFloat::Msg(m) => ctx.err(n, m),
                }),
                _ => Err(ctx.mismatch(n, ty)),
            }
        }
        Ty::Char => match &n.value {
            Value::Char(c) => Ok(quote!(#c)),
            Value::Str(s) | Value::Text(s) if s.chars().count() == 1 => {
                let c = s.chars().next().unwrap();
                Ok(quote!(#c))
            }
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::Str => match &n.value {
            Value::Str(s) | Value::Text(s) => {
                let lit = Literal::string(s);
                Ok(quote!(#lit))
            }
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::Unit => match &n.value {
            Value::Null => Ok(quote!(())),
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::Slice(inner) => match &n.value {
            Value::List(items) => {
                let vals = elements(items, inner, ctx)?;
                Ok(quote!(&[#(#vals),*]))
            }
            Value::Map(entries) if matches!(&**inner, Ty::Tuple(kv) if kv.len() == 2) => {
                let Ty::Tuple(kv) = &**inner else {
                    unreachable!()
                };
                let mut vals = Vec::new();
                for (k, v) in entries {
                    let key = Node::new(Value::Text(k.clone()), v.pos);
                    let (kt, vt) = ctx.at(Seg::Key(k.clone()), |ctx| {
                        Ok::<_, DataError>((convert(&key, &kv[0], ctx)?, convert(v, &kv[1], ctx)?))
                    })?;
                    vals.push(quote!((#kt, #vt)));
                }
                Ok(quote!(&[#(#vals),*]))
            }
            _ => Err(ctx.err(
                n,
                format!(
                    "expected a list of {}, found {}",
                    inner.name(),
                    n.describe()
                ),
            )),
        },
        Ty::Array(inner, len) => match &n.value {
            Value::List(items) if items.len() == *len => {
                let vals = elements(items, inner, ctx)?;
                Ok(quote!([#(#vals),*]))
            }
            Value::List(items) => {
                Err(ctx.err(n, format!("expected {len} elements, found {}", items.len())))
            }
            _ => Err(ctx.mismatch(n, ty)),
        },
        Ty::Tuple(tys) => {
            let vals = tuple(n, tys, ctx)?;
            Ok(paren(&vals))
        }
        Ty::Named(name) => {
            let def = ctx
                .defs
                .get(name)
                .unwrap_or_else(|| panic!("type {name} isn't defined"));
            convert_def(n, def, ctx)
        }
    }
}

fn elements(items: &[Node], ty: &Ty, ctx: &mut Ctx) -> Result<Vec<TokenStream>, DataError> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| ctx.at(Seg::Index(i), |ctx| convert(item, ty, ctx)))
        .collect()
}

/// A list of exactly `tys.len()` values.
fn tuple(n: &Node, tys: &[Ty], ctx: &mut Ctx) -> Result<Vec<TokenStream>, DataError> {
    match &n.value {
        Value::List(items) if items.len() == tys.len() => items
            .iter()
            .zip(tys)
            .enumerate()
            .map(|(i, (item, ty))| ctx.at(Seg::Index(i), |ctx| convert(item, ty, ctx)))
            .collect(),
        Value::List(items) => Err(ctx.err(
            n,
            format!("expected {} elements, found {}", tys.len(), items.len()),
        )),
        _ => Err(ctx.err(
            n,
            format!(
                "expected a list of {} elements, found {}",
                tys.len(),
                n.describe()
            ),
        )),
    }
}

/// `(a, b)`, with the trailing comma a one-element tuple needs.
fn paren(vals: &[TokenStream]) -> TokenStream {
    match vals {
        [one] => quote!((#one,)),
        _ => quote!((#(#vals),*)),
    }
}

fn convert_def(n: &Node, def: &Def, ctx: &mut Ctx) -> Res {
    let ident = &def.ident;
    let name = ident.to_string();
    // RON's `Name(..)`: the name must be this type's.
    let payload = match (&n.value, &def.kind) {
        (Value::Named(_, _), DefKind::Enum(_)) => n,
        (Value::Named(found, payload), _) if *found == name => payload,
        (Value::Named(_, _), _) => {
            return Err(ctx.err(n, format!("expected {name}, found {}", n.describe())));
        }
        _ => n,
    };
    match &def.kind {
        DefKind::Struct(fields) => match &payload.value {
            Value::Map(entries) => {
                let vals = struct_fields(payload, entries, fields, def.allow_unknown, ctx)?;
                Ok(quote!(#ident { #(#vals),* }))
            }
            _ => Err(ctx.err(n, format!("expected {name}, found {}", n.describe()))),
        },
        DefKind::Tuple(tys) => {
            let vals = tuple(payload, tys, ctx)?;
            Ok(quote!(#ident(#(#vals),*)))
        }
        DefKind::Unit => match &payload.value {
            Value::Null => Ok(quote!(#ident)),
            _ => Err(ctx.err(n, format!("expected {name}, found {}", n.describe()))),
        },
        DefKind::Enum(variants) => {
            let keys: Vec<&str> = variants.iter().map(|v| v.key.as_str()).collect();
            let unknown = |ctx: &Ctx, key: &str| {
                let hint = match did_you_mean(key, &keys) {
                    Some(k) => format!("; did you mean {k:?}?"),
                    None => format!("; expected one of {}", quoted(&keys)),
                };
                ctx.err(n, format!("unknown variant {key:?}{hint}"))
            };
            let find = |key: &str| variants.iter().find(|v| v.key == key);
            match &n.value {
                Value::Str(s) | Value::Text(s) => {
                    let v = find(s).ok_or_else(|| unknown(ctx, s))?;
                    let vi = &v.ident;
                    match v.kind {
                        VariantKind::Unit => Ok(quote!(#ident::#vi)),
                        _ => {
                            Err(ctx
                                .err(n, format!("variant {s:?} needs data, like {{{s:?}: ...}}")))
                        }
                    }
                }
                Value::Map(entries) if entries.len() == 1 => {
                    let (key, data) = &entries[0];
                    let v = find(key).ok_or_else(|| unknown(ctx, key))?;
                    let vi = &v.ident;
                    match &v.kind {
                        VariantKind::Unit => {
                            Err(ctx.err(n, format!("variant {key:?} has no data; write {key:?}")))
                        }
                        VariantKind::Tuple(tys) if tys.len() == 1 => {
                            let val =
                                ctx.at(Seg::Key(key.clone()), |ctx| convert(data, &tys[0], ctx))?;
                            Ok(quote!(#ident::#vi(#val)))
                        }
                        VariantKind::Tuple(tys) => {
                            let vals =
                                ctx.at(Seg::Key(key.clone()), |ctx| tuple(data, tys, ctx))?;
                            Ok(quote!(#ident::#vi(#(#vals),*)))
                        }
                        VariantKind::Struct(fields) => {
                            ctx.at(Seg::Key(key.clone()), |ctx| match &data.value {
                                Value::Map(entries) => {
                                    let vals = struct_fields(
                                        data,
                                        entries,
                                        fields,
                                        def.allow_unknown,
                                        ctx,
                                    )?;
                                    Ok(quote!(#ident::#vi { #(#vals),* }))
                                }
                                _ => Err(ctx.err(
                                    data,
                                    format!(
                                        "expected the fields of {name}::{vi}, found {}",
                                        data.describe()
                                    ),
                                )),
                            })
                        }
                    }
                }
                Value::Map(entries) => {
                    let example = variants
                        .iter()
                        .find(|v| !matches!(v.kind, VariantKind::Unit))
                        .unwrap_or(&variants[0]);
                    Err(ctx.err(n, format!("expected one variant, like {{{:?}: ...}}, found a mapping with {} keys", example.key, entries.len())))
                }
                Value::Named(key, data) => {
                    let v = find(key).ok_or_else(|| unknown(ctx, key))?;
                    let vi = &v.ident;
                    match (&v.kind, &data.value) {
                        (VariantKind::Unit, Value::Null) => Ok(quote!(#ident::#vi)),
                        (VariantKind::Unit, _) => {
                            Err(ctx.err(n, format!("variant {key:?} has no data; write {key}")))
                        }
                        (VariantKind::Tuple(tys), _) => {
                            let vals = tuple(data, tys, ctx)?;
                            Ok(quote!(#ident::#vi(#(#vals),*)))
                        }
                        (VariantKind::Struct(fields), Value::Map(entries)) => {
                            let vals =
                                struct_fields(data, entries, fields, def.allow_unknown, ctx)?;
                            Ok(quote!(#ident::#vi { #(#vals),* }))
                        }
                        (VariantKind::Struct(_), _) => Err(ctx.err(
                            n,
                            format!("expected {key}(field: ...), found {}", n.describe()),
                        )),
                    }
                }
                _ => Err(ctx.err(n, format!("expected {name}, found {}", n.describe()))),
            }
        }
    }
}

fn quoted(keys: &[&str]) -> String {
    keys.iter()
        .map(|k| format!("{k:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `field: value` for each field, from the entries of a mapping.
fn struct_fields(
    map: &Node,
    entries: &[(String, Node)],
    fields: &[Field],
    allow_unknown: bool,
    ctx: &mut Ctx,
) -> Result<Vec<TokenStream>, DataError> {
    let keys: Vec<&str> = fields
        .iter()
        .filter(|f| !f.body)
        .map(|f| f.key.as_str())
        .collect();
    let known: std::collections::HashSet<&str> = keys.iter().copied().collect();
    let by_key: std::collections::HashMap<&str, &Node> =
        entries.iter().map(|(k, v)| (k.as_str(), v)).collect();
    if !allow_unknown
        && let Some((key, node)) = entries.iter().find(|(k, _)| !known.contains(k.as_str()))
    {
        let hint = match did_you_mean(key, &keys) {
            _ if fields.iter().any(|f| f.body && f.key == *key) => {
                "; the #[bake(body)] field has this name and holds the Markdown body, so rename the field".into()
            }
            Some(k) => format!("; did you mean {k:?}?"),
            None if keys.is_empty() => "; this type has no fields".into(),
            None => format!("; expected one of {}", quoted(&keys)),
        };
        return Err(ctx.at(Seg::Key(key.clone()), |ctx| {
            ctx.err(node, format!("unknown key{hint}"))
        }));
    }
    let mut out = Vec::new();
    for f in fields {
        let fi = &f.ident;
        let val = if f.body {
            if !ctx.path.is_empty() {
                return Err(ctx.err(
                    map,
                    "#[bake(body)] works only on the item's own struct, not a nested one",
                ));
            }
            match ctx.body {
                Some(b) => {
                    let lit = Literal::string(b);
                    quote!(#lit)
                }
                None => return Err(ctx.err(map, "#[bake(body)] needs a Markdown file")),
            }
        } else {
            match by_key.get(f.key.as_str()) {
                Some(v) => ctx.at(Seg::Key(f.key.clone()), |ctx| convert(v, &f.ty, ctx))?,
                None if matches!(f.ty, Ty::Option(_)) => quote!(::core::option::Option::None),
                None => return Err(ctx.err(map, format!("missing key {:?}", f.key))),
            }
        };
        out.push(quote!(#fi: #val));
    }
    Ok(out)
}

fn int_lit(negative: bool, magnitude: u128, it: IntTy) -> Option<TokenStream> {
    let (neg_limit, max) = it.limits();
    let name = it.name();
    if negative {
        if magnitude > neg_limit {
            return None;
        }
        // `isize::MIN` would quietly differ by target; a literal is checked by rustc.
        if magnitude == neg_limit && it != IntTy::Isize {
            let t = format_ident!("{name}");
            return Some(quote!(::core::primitive::#t::MIN));
        }
        let lit = syn::LitInt::new(&format!("{magnitude}{name}"), Span::call_site());
        Some(quote!(-#lit))
    } else {
        (magnitude <= max).then(|| {
            let lit = syn::LitInt::new(&format!("{magnitude}{name}"), Span::call_site());
            quote!(#lit)
        })
    }
}

/// An integer written as text: decimal digits, an optional `-`. The error is a hint
/// (`; write 80`) or empty.
fn text_int(t: &str, it: IntTy) -> Result<TokenStream, String> {
    let (sign, digits) = match t.strip_prefix(['-', '+']) {
        Some(d) => (&t[..1], d),
        None => ("", t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(String::new());
    }
    let trimmed = match digits.trim_start_matches('0') {
        "" => "0",
        d => d,
    };
    let magnitude: u128 = trimmed.parse().map_err(|_| String::new())?;
    let negative = sign == "-" && magnitude != 0;
    let lit = int_lit(negative, magnitude, it).ok_or_else(String::new)?;
    if sign == "+" || trimmed.len() != digits.len() {
        return Err(format!(
            "; write {}{trimmed}",
            if negative { "-" } else { "" }
        ));
    }
    Ok(lit)
}

fn float_lit(f: f64, f32: bool) -> Result<TokenStream, String> {
    let t = format_ident!("{}", if f32 { "f32" } else { "f64" });
    if f.is_nan() {
        return Ok(quote!(::core::primitive::#t::NAN));
    }
    if f.is_infinite() {
        return Ok(match f > 0.0 {
            true => quote!(::core::primitive::#t::INFINITY),
            false => quote!(::core::primitive::#t::NEG_INFINITY),
        });
    }
    let lit = if f32 {
        if (f as f32) as f64 != f && format!("{}", f as f32) != format!("{f}") {
            return Err(format!("{f} doesn't fit f32 exactly; use f64"));
        }
        Literal::f32_suffixed((f as f32).abs())
    } else {
        Literal::f64_suffixed(f.abs())
    };
    Ok(match f.is_sign_negative() {
        true => quote!(-#lit),
        false => quote!(#lit),
    })
}

fn int_to_float(negative: bool, magnitude: u128, f32: bool) -> Result<TokenStream, String> {
    const TWO_128: f64 = 340_282_366_920_938_463_463_374_607_431_768_211_456.0;
    let exact = if f32 {
        let f = magnitude as f32;
        f.is_finite() && (f as f64) < TWO_128 && f as u128 == magnitude
    } else {
        let f = magnitude as f64;
        f < TWO_128 && f as u128 == magnitude
    };
    let written = format!("{}{magnitude}", if negative { "-" } else { "" });
    if !exact {
        return Err(format!(
            "{written} isn't exactly representable as {}",
            if f32 { "f32" } else { "f64" }
        ));
    }
    let f = magnitude as f64;
    // `negative` with a zero magnitude is "-0".
    float_lit(if negative { -f } else { f }, f32)
}

enum TextFloat {
    Hint(String),
    Msg(String),
}

/// A float written as text: `-1.5`, `2`, `1e3`. No `inf` or `NaN`.
fn text_float(t: &str, f32: bool) -> Result<TokenStream, TextFloat> {
    let body = t.strip_prefix('-').unwrap_or(t);
    let shape = |s: &str| {
        let (mantissa, exp) = match s.split_once(['e', 'E']) {
            Some((m, e)) => (m, Some(e)),
            None => (s, None),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        !int.is_empty()
            && int.bytes().all(|b| b.is_ascii_digit())
            && frac.bytes().all(|b| b.is_ascii_digit())
            && exp.is_none_or(|e| {
                let e = e.strip_prefix(['+', '-']).unwrap_or(e);
                !e.is_empty() && e.bytes().all(|b| b.is_ascii_digit())
            })
    };
    if !shape(body) {
        let fixed = match t.strip_prefix('+') {
            Some(rest) if shape(rest) => Some(rest.to_string()),
            _ if body.starts_with('.') && shape(&format!("0{body}")) => {
                Some(t.replacen('.', "0.", 1))
            }
            _ => None,
        };
        return Err(TextFloat::Hint(
            fixed.map_or(String::new(), |f| format!("; write {f}")),
        ));
    }
    // Leading zeros are an error, as for integers: "007" reads as 7 only by guessing.
    let whole = body.split(['.', 'e', 'E']).next().unwrap_or(body);
    if whole.len() > 1 && whole.starts_with('0') {
        let trimmed = whole.trim_start_matches('0');
        let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
        let sign = if t.starts_with('-') { "-" } else { "" };
        return Err(TextFloat::Hint(format!(
            "; write {sign}{}",
            body.replacen(whole, trimmed, 1)
        )));
    }
    if body.bytes().all(|b| b.is_ascii_digit()) {
        let magnitude: u128 = body
            .parse()
            .map_err(|_| TextFloat::Msg(format!("{t} is out of range")))?;
        return int_to_float(t.starts_with('-'), magnitude, f32).map_err(TextFloat::Msg);
    }
    let f: f64 = t.parse().map_err(|_| TextFloat::Hint(String::new()))?;
    let f = crate::value::check_float(t, f).map_err(TextFloat::Msg)?;
    float_lit(f, f32).map_err(TextFloat::Msg)
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
