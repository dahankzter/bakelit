//! The `bake!` invocation: items to bake and the types they use.

use crate::naming::{CASE_NAMES, Case};
use crate::ty::Ty;
use proc_macro2::TokenStream;
use std::collections::BTreeMap;
use syn::spanned::Spanned;
use syn::{
    Attribute, Error, Expr, Fields, Ident, Item, LitStr, StaticMutability, Type, Visibility,
};

pub struct Invocation {
    pub items: Vec<BakeItem>,
    pub defs: BTreeMap<String, Def>,
    /// The type definitions as written, without `#[bake]` attributes.
    pub passthrough: Vec<Item>,
}

pub struct BakeItem {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub is_static: bool,
    pub ident: Ident,
    pub ty_syn: Box<Type>,
    pub ty: Ty,
    pub path: LitStr,
    pub format: Option<Ident>,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub ident: Ident,
    pub allow_unknown: bool,
    pub kind: DefKind,
}

#[derive(Clone, Debug)]
pub enum DefKind {
    Struct(Vec<Field>),
    Tuple(Vec<Ty>),
    Unit,
    Enum(Vec<Variant>),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub ident: Ident,
    /// The key in the data, after `rename` and `rename_all`.
    pub key: String,
    pub ty: Ty,
    pub body: bool,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub ident: Ident,
    pub key: String,
    pub kind: VariantKind,
}

#[derive(Clone, Debug)]
pub enum VariantKind {
    Unit,
    Tuple(Vec<Ty>),
    Struct(Vec<Field>),
}

const NOT_AN_ITEM: &str = "bake! takes static, const, struct and enum items";

pub fn parse_invocation(tokens: TokenStream) -> syn::Result<Invocation> {
    let file: syn::File = syn::parse2(tokens)?;
    let mut inv = Invocation {
        items: Vec::new(),
        defs: BTreeMap::new(),
        passthrough: Vec::new(),
    };
    for item in file.items {
        match item {
            Item::Static(s) => {
                if let StaticMutability::Mut(m) = s.mutability {
                    return Err(Error::new(m.span(), "static mut isn't supported"));
                }
                inv.items
                    .push(bake_item(s.attrs, s.vis, true, s.ident, s.ty, *s.expr)?);
            }
            Item::Const(c) => inv
                .items
                .push(bake_item(c.attrs, c.vis, false, c.ident, c.ty, *c.expr)?),
            Item::Struct(mut s) => {
                no_generics(&s.generics)?;
                let (rename_all, allow_unknown) = type_attrs(&s.attrs)?;
                let kind = match &s.fields {
                    Fields::Named(named) => {
                        DefKind::Struct(fields(named.named.iter(), rename_all)?)
                    }
                    Fields::Unnamed(t) => DefKind::Tuple(
                        t.unnamed
                            .iter()
                            .map(|f| Ty::from_syn(&f.ty))
                            .collect::<syn::Result<_>>()?,
                    ),
                    Fields::Unit => DefKind::Unit,
                };
                add_def(
                    &mut inv,
                    Def {
                        ident: s.ident.clone(),
                        allow_unknown,
                        kind,
                    },
                )?;
                strip(&mut s.attrs);
                s.fields.iter_mut().for_each(|f| strip(&mut f.attrs));
                inv.passthrough.push(Item::Struct(s));
            }
            Item::Enum(mut e) => {
                no_generics(&e.generics)?;
                let (rename_all, allow_unknown) = type_attrs(&e.attrs)?;
                let mut variants = Vec::new();
                for v in &e.variants {
                    let rename = member_attrs(&v.attrs, false)?.0;
                    let name = v.ident.to_string();
                    let key = rename
                        .unwrap_or_else(|| rename_all.map_or(name.clone(), |c| c.variant(&name)));
                    let kind = match &v.fields {
                        Fields::Unit => VariantKind::Unit,
                        Fields::Unnamed(t) => VariantKind::Tuple(
                            t.unnamed
                                .iter()
                                .map(|f| Ty::from_syn(&f.ty))
                                .collect::<syn::Result<_>>()?,
                        ),
                        Fields::Named(named) => {
                            VariantKind::Struct(fields(named.named.iter(), None)?)
                        }
                    };
                    if variants.iter().any(|w: &Variant| w.key == key) {
                        return Err(Error::new(
                            v.ident.span(),
                            format!("two variants read {key:?}"),
                        ));
                    }
                    variants.push(Variant {
                        ident: v.ident.clone(),
                        key,
                        kind,
                    });
                }
                if variants.is_empty() {
                    return Err(Error::new(
                        e.ident.span(),
                        "an enum needs at least one variant",
                    ));
                }
                add_def(
                    &mut inv,
                    Def {
                        ident: e.ident.clone(),
                        allow_unknown,
                        kind: DefKind::Enum(variants),
                    },
                )?;
                strip(&mut e.attrs);
                for v in e.variants.iter_mut() {
                    strip(&mut v.attrs);
                    v.fields.iter_mut().for_each(|f| strip(&mut f.attrs));
                }
                inv.passthrough.push(Item::Enum(e));
            }
            other => return Err(Error::new(other.span(), NOT_AN_ITEM)),
        }
    }
    check_defined(&inv)?;
    Ok(inv)
}

fn bake_item(
    attrs: Vec<Attribute>,
    vis: Visibility,
    is_static: bool,
    ident: Ident,
    ty_syn: Box<Type>,
    expr: Expr,
) -> syn::Result<BakeItem> {
    if let Some(a) = attrs.iter().find(|a| a.path().is_ident("bake")) {
        return Err(Error::new(
            a.span(),
            "#[bake] attributes go on types, fields and variants",
        ));
    }
    let (path, format) = match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(path),
            ..
        }) => (path, None),
        Expr::Cast(cast) => {
            let Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(path),
                ..
            }) = *cast.expr
            else {
                return Err(Error::new(
                    cast.expr.span(),
                    r#"expected a path string literal, like "data/app.json""#,
                ));
            };
            match *cast.ty {
                Type::Path(p) if p.qself.is_none() && p.path.get_ident().is_some() => {
                    (path, p.path.get_ident().cloned())
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "expected a format name after `as`, like `as json`",
                    ));
                }
            }
        }
        other => {
            return Err(Error::new(
                other.span(),
                r#"expected a path string literal, like "data/app.json""#,
            ));
        }
    };
    let ty = Ty::from_syn(&ty_syn)?;
    Ok(BakeItem {
        attrs,
        vis,
        is_static,
        ident,
        ty_syn,
        ty,
        path,
        format,
    })
}

fn no_generics(g: &syn::Generics) -> syn::Result<()> {
    match g.params.is_empty() && g.where_clause.is_none() {
        true => Ok(()),
        false => Err(Error::new(g.span(), "generic types aren't supported")),
    }
}

fn add_def(inv: &mut Invocation, def: Def) -> syn::Result<()> {
    let name = def.ident.to_string();
    if inv.defs.contains_key(&name) {
        return Err(Error::new(
            def.ident.span(),
            format!("type `{name}` is defined twice"),
        ));
    }
    inv.defs.insert(name, def);
    Ok(())
}

fn fields<'a>(
    fs: impl Iterator<Item = &'a syn::Field>,
    rename_all: Option<Case>,
) -> syn::Result<Vec<Field>> {
    let mut out: Vec<Field> = Vec::new();
    for f in fs {
        let ident = f.ident.clone().unwrap();
        let (rename, body) = member_attrs(&f.attrs, true)?;
        let ty = Ty::from_syn(&f.ty)?;
        if body && ty != Ty::Str {
            return Err(Error::new(
                f.ty.span(),
                "#[bake(body)] needs a &'static str field",
            ));
        }
        let name = ident.to_string().trim_start_matches("r#").to_string();
        let key = rename.unwrap_or_else(|| rename_all.map_or(name.clone(), |c| c.field(&name)));
        if !body && out.iter().any(|g| !g.body && g.key == key) {
            return Err(Error::new(
                ident.span(),
                format!("two fields read the key {key:?}"),
            ));
        }
        out.push(Field {
            ident,
            key,
            ty,
            body,
        });
    }
    Ok(out)
}

/// `rename_all` and `allow_unknown` on a struct or enum.
fn type_attrs(attrs: &[Attribute]) -> syn::Result<(Option<Case>, bool)> {
    let (mut rename_all, mut allow_unknown) = (None, false);
    for a in attrs.iter().filter(|a| a.path().is_ident("bake")) {
        a.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let v: LitStr = meta.value()?.parse()?;
                rename_all = Some(Case::from_name(&v.value()).ok_or_else(|| {
                    let names: Vec<String> =
                        CASE_NAMES.iter().map(|(n, _)| format!("{n:?}")).collect();
                    Error::new(
                        v.span(),
                        format!(
                            "unknown rename_all style {:?}; expected one of {}",
                            v.value(),
                            names.join(", ")
                        ),
                    )
                })?);
                Ok(())
            } else if meta.path.is_ident("allow_unknown") {
                allow_unknown = true;
                Ok(())
            } else {
                Err(meta.error(format!(
                    "unknown bake attribute `{}`; expected rename_all or allow_unknown",
                    path_name(&meta.path)
                )))
            }
        })?;
    }
    Ok((rename_all, allow_unknown))
}

/// `rename` (and, on fields, `body`) on a field or variant.
fn member_attrs(attrs: &[Attribute], field: bool) -> syn::Result<(Option<String>, bool)> {
    let (mut rename, mut body) = (None, false);
    for a in attrs.iter().filter(|a| a.path().is_ident("bake")) {
        a.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                rename = Some(meta.value()?.parse::<LitStr>()?.value());
                Ok(())
            } else if field && meta.path.is_ident("body") {
                body = true;
                Ok(())
            } else {
                let expected = if field { "rename or body" } else { "rename" };
                Err(meta.error(format!(
                    "unknown bake attribute `{}`; expected {expected}",
                    path_name(&meta.path)
                )))
            }
        })?;
    }
    Ok((rename, body))
}

fn path_name(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn strip(attrs: &mut Vec<Attribute>) {
    attrs.retain(|a| !a.path().is_ident("bake"));
}

/// Every type a definition mentions must be defined in the invocation.
fn check_defined(inv: &Invocation) -> syn::Result<()> {
    for item in &inv.passthrough {
        let fields: Vec<&syn::Field> = match item {
            Item::Struct(s) => s.fields.iter().collect(),
            Item::Enum(e) => e.variants.iter().flat_map(|v| v.fields.iter()).collect(),
            _ => Vec::new(),
        };
        for f in fields {
            let mut names = Vec::new();
            Ty::from_syn(&f.ty)?.named(&mut names);
            if let Some(n) = names.iter().find(|n| !inv.defs.contains_key(*n)) {
                return Err(Error::new(
                    f.ty.span(),
                    format!("type `{n}` isn't defined in this bake! invocation"),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
