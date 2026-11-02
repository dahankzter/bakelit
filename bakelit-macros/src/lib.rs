//! Procedural macros for [bakelit](https://docs.rs/bakelit). Use the `bakelit` crate.
// With no format enabled, nothing reads the value model.
#![cfg_attr(
    not(any(
        feature = "json",
        feature = "jsonc",
        feature = "json5",
        feature = "jsonl",
        feature = "yaml",
        feature = "toml",
        feature = "ron",
        feature = "csv",
        feature = "ini",
        feature = "env",
        feature = "properties",
        feature = "kdl",
        feature = "frontmatter"
    )),
    allow(dead_code, unused_variables)
)]

mod convert;
mod formats;
mod infer;
mod items;
mod naming;
mod ty;
mod value;

use formats::Format;
use items::{BakeItem, Invocation};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// See the `bakelit` crate.
#[proc_macro]
pub fn bake(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let base = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    expand(input.into(), &base).into()
}

fn expand(input: TokenStream, base: &Path) -> TokenStream {
    let inv = match items::parse_invocation(input) {
        Ok(inv) => inv,
        Err(e) => return e.to_compile_error(),
    };
    let mut out = TokenStream::new();
    for def in &inv.passthrough {
        out.extend(quote!(#def));
    }
    // Types inferred so far, so two items can't generate the same name.
    let mut inferred = BTreeMap::new();
    for item in &inv.items {
        out.extend(bake_item(item, &inv, &mut inferred, base));
    }
    out
}

fn bake_item(
    item: &BakeItem,
    inv: &Invocation,
    inferred: &mut BTreeMap<String, items::Def>,
    base: &Path,
) -> TokenStream {
    let BakeItem {
        attrs,
        vis,
        ident,
        ty_syn,
        ..
    } = item;
    let kw = if item.is_static {
        quote!(static)
    } else {
        quote!(const)
    };
    let (value, extra) = match value(item, inv, inferred, base) {
        Ok((value, abs, types)) => (
            value,
            quote!(#(#types)* const _: &[u8] = ::core::include_bytes!(#abs);),
        ),
        Err(msg) => {
            let err = quote_spanned!(item.path.span()=> ::core::compile_error!(#msg));
            (err, TokenStream::new())
        }
    };
    quote! {
        #(#attrs)*
        #vis #kw #ident: #ty_syn = #value;
        #extra
    }
}

/// The item's value, the absolute path of its file, and any inferred types.
fn value(
    item: &BakeItem,
    inv: &Invocation,
    inferred: &mut BTreeMap<String, items::Def>,
    base: &Path,
) -> Result<(TokenStream, String, Vec<syn::Item>), String> {
    let to_infer = infer::inferable(&item.ty, &inv.defs)?;
    let rel = item.path.value();
    let full = base.join(&rel);
    let bytes = std::fs::read(&full).map_err(|e| format!("can't read {}: {e}", full.display()))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{rel}: the file isn't UTF-8 text"))?;
    let format = match &item.format {
        Some(name) => {
            let name = name.to_string();
            Format::from_name(&name).ok_or_else(|| {
                let names = Format::names();
                match naming::did_you_mean(&name, &names) {
                    Some(n) => format!("unknown format `{name}`; did you mean `{n}`?"),
                    None => format!(
                        "unknown format `{name}`; expected one of {}",
                        names.join(", ")
                    ),
                }
            })?
        }
        None => Format::from_path(&rel).ok_or_else(|| {
            format!(
                "can't tell the format of {rel:?} from its extension; \
                 add `as <format>`, like `{rel:?} as ini`"
            )
        })?,
    };
    if !format.enabled() {
        return Err(format!(
            "reading .{} files needs bakelit's `{}` feature",
            format.extension(),
            format.feature()
        ));
    }
    let (node, body) = formats::parse_document(format, &text).map_err(|e| e.render(&rel))?;
    let mut defs = inv.defs.clone();
    let mut types = Vec::new();
    if to_infer.is_some() {
        let rule = match format {
            Format::Csv | Format::Tsv => infer::TextRule::Guess,
            _ => infer::TextRule::Str,
        };
        let found = infer::infer(&node, &item.ty, &inv.defs, rule, body.is_some())
            .map_err(|e| e.render(&rel))?;
        if let Some(n) = found.defs.keys().find(|n| inv.defs.contains_key(*n)) {
            return Err(format!(
                "the inferred type name {n} is already defined in this invocation; rename that type"
            ));
        }
        if let Some(n) = found.defs.keys().find(|n| inferred.contains_key(*n)) {
            return Err(format!(
                "the inferred type name {n} is used by another item; name this item's type differently"
            ));
        }
        inferred.extend(found.defs.clone());
        defs.extend(found.defs);
        types = found.items;
    }
    let mut ctx = convert::Ctx::new(&defs);
    ctx.body = body;
    let value = convert::convert(&node, &item.ty, &mut ctx).map_err(|e| e.render(&rel))?;
    let abs = full
        .canonicalize()
        .map_err(|e| format!("can't read {}: {e}", full.display()))?;
    let abs = abs
        .to_str()
        .ok_or_else(|| format!("{rel}: the path isn't UTF-8"))?
        .to_string();
    Ok((value, abs, types))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod sweep_tests;
