//! The Rust types a value can be baked into.

use syn::spanned::Spanned;
use syn::{Error, GenericArgument, PathArguments, Type};

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Bool,
    Char,
    Int(IntTy),
    F32,
    F64,
    Str,
    Unit,
    Option(Box<Ty>),
    Slice(Box<Ty>),
    Array(Box<Ty>, usize),
    Tuple(Vec<Ty>),
    /// A struct or enum defined in the invocation (or to be inferred).
    Named(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IntTy {
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
}

const INTS: [(&str, IntTy); 12] = [
    ("u8", IntTy::U8),
    ("u16", IntTy::U16),
    ("u32", IntTy::U32),
    ("u64", IntTy::U64),
    ("u128", IntTy::U128),
    ("usize", IntTy::Usize),
    ("i8", IntTy::I8),
    ("i16", IntTy::I16),
    ("i32", IntTy::I32),
    ("i64", IntTy::I64),
    ("i128", IntTy::I128),
    ("isize", IntTy::Isize),
];

impl IntTy {
    pub fn name(self) -> &'static str {
        INTS.iter()
            .find(|(_, t)| *t == self)
            .map(|(n, _)| *n)
            .unwrap()
    }

    /// The largest magnitude of a negative value, and the largest value. `usize` and
    /// `isize` are checked as 64-bit; rustc rejects literals too large for the target.
    pub fn limits(self) -> (u128, u128) {
        let bits = match self {
            IntTy::U8 | IntTy::I8 => 8,
            IntTy::U16 | IntTy::I16 => 16,
            IntTy::U32 | IntTy::I32 => 32,
            IntTy::U64 | IntTy::I64 | IntTy::Usize | IntTy::Isize => 64,
            IntTy::U128 | IntTy::I128 => 128,
        };
        if self.signed() {
            (1u128 << (bits - 1), (1u128 << (bits - 1)) - 1)
        } else {
            (0, u128::MAX >> (128 - bits))
        }
    }

    pub fn signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize
        )
    }
}

impl Ty {
    /// The type as an error message writes it.
    pub fn name(&self) -> String {
        match self {
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Int(i) => i.name().into(),
            Ty::F32 => "f32".into(),
            Ty::F64 => "f64".into(),
            Ty::Str => "&str".into(),
            Ty::Unit => "()".into(),
            Ty::Option(t) => format!("Option<{}>", t.name()),
            Ty::Slice(t) => format!("&[{}]", t.name()),
            Ty::Array(t, n) => format!("[{}; {n}]", t.name()),
            Ty::Tuple(ts) => format!(
                "({})",
                ts.iter().map(Ty::name).collect::<Vec<_>>().join(", ")
            ),
            Ty::Named(n) => n.clone(),
        }
    }

    /// Every `Named` type this type mentions.
    pub fn named(&self, out: &mut Vec<String>) {
        match self {
            Ty::Named(n) => out.push(n.clone()),
            Ty::Option(t) | Ty::Slice(t) | Ty::Array(t, _) => t.named(out),
            Ty::Tuple(ts) => ts.iter().for_each(|t| t.named(out)),
            _ => {}
        }
    }

    pub fn from_syn(ty: &Type) -> syn::Result<Ty> {
        match ty {
            Type::Paren(p) => Ty::from_syn(&p.elem),
            Type::Group(g) => Ty::from_syn(&g.elem),
            Type::Reference(r) => {
                if r.mutability.is_some() {
                    return Err(Error::new(ty.span(), "&mut isn't supported"));
                }
                if let Some(l) = &r.lifetime
                    && l.ident != "static"
                {
                    return Err(Error::new(l.span(), "use &'static, or no lifetime"));
                }
                match &*r.elem {
                    Type::Path(p) if p.qself.is_none() && p.path.is_ident("str") => Ok(Ty::Str),
                    Type::Slice(s) => Ok(Ty::Slice(Box::new(Ty::from_syn(&s.elem)?))),
                    _ => Err(Error::new(ty.span(), "use T, &'static str or &'static [T]")),
                }
            }
            Type::Array(a) => {
                let n = match &a.len {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(n),
                        ..
                    }) => n.base10_parse::<usize>()?,
                    other => {
                        return Err(Error::new(
                            other.span(),
                            "the array length must be a number",
                        ));
                    }
                };
                Ok(Ty::Array(Box::new(Ty::from_syn(&a.elem)?), n))
            }
            Type::Tuple(t) if t.elems.is_empty() => Ok(Ty::Unit),
            Type::Tuple(t) if t.elems.len() > 12 => {
                Err(Error::new(ty.span(), "tuples have at most 12 elements"))
            }
            Type::Tuple(t) => Ok(Ty::Tuple(
                t.elems
                    .iter()
                    .map(Ty::from_syn)
                    .collect::<syn::Result<_>>()?,
            )),
            Type::Path(p) if p.qself.is_none() => from_path(&p.path),
            _ => Err(Error::new(ty.span(), "this type isn't supported")),
        }
    }
}

fn from_path(path: &syn::Path) -> syn::Result<Ty> {
    let last = path.segments.last().unwrap();
    let name = last.ident.to_string();
    let simple = path.segments.len() == 1 && path.leading_colon.is_none();
    let option = name == "Option"
        && (simple
            || ["core", "std"].contains(&path.segments[0].ident.to_string().as_str())
                && path.segments.len() == 3
                && path.segments[1].ident == "option");
    if option {
        if let PathArguments::AngleBracketed(args) = &last.arguments
            && let [GenericArgument::Type(t)] = args.args.iter().collect::<Vec<_>>().as_slice()
        {
            return Ok(Ty::Option(Box::new(Ty::from_syn(t)?)));
        }
        return Err(Error::new(last.span(), "Option takes one type"));
    }
    let span = path.span();
    match name.as_str() {
        "String" => return Err(Error::new(span, "String isn't const; use &'static str")),
        "Vec" => return Err(Error::new(span, "Vec isn't const; use &'static [T]")),
        "HashMap" | "BTreeMap" | "IndexMap" => {
            return Err(Error::new(
                span,
                format!("{name} isn't const; use &'static [(K, V)]"),
            ));
        }
        "str" => return Err(Error::new(span, "use &'static str")),
        _ => {}
    }
    if !simple || !matches!(last.arguments, PathArguments::None) {
        return Err(Error::new(span, format!("type `{name}` isn't supported")));
    }
    Ok(match name.as_str() {
        "bool" => Ty::Bool,
        "char" => Ty::Char,
        "f32" => Ty::F32,
        "f64" => Ty::F64,
        _ => match INTS.iter().find(|(n, _)| *n == name) {
            Some((_, i)) => Ty::Int(*i),
            None => Ty::Named(name),
        },
    })
}
