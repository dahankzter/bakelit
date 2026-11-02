use super::*;

fn parse_err(src: &str) -> String {
    match parse_invocation(src.parse().unwrap()) {
        Ok(_) => panic!("{src:?} parsed"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn items_and_defs() {
    let inv = parse_invocation(
        r#"
        #[doc = "x"]
        pub static A: Cfg = "a.json";
        const B: &[u8] = "b.conf" as json;
        #[derive(Debug)]
        #[bake(rename_all = "kebab-case", allow_unknown)]
        pub struct Cfg { #[bake(rename = "x-y")] pub xy: u8, pub max_conns: Option<u32> }
        enum E { A, #[bake(rename = "bee")] B(u8), C { d: u8 } }
        struct T(u8);
        struct U;
        "#
        .parse()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(inv.items.len(), 2);
    assert_eq!(inv.items[0].ident.to_string(), "A");
    assert!(inv.items[0].is_static);
    assert_eq!(inv.items[1].format.as_ref().unwrap().to_string(), "json");
    let cfg = &inv.defs["Cfg"];
    assert!(cfg.allow_unknown);
    let DefKind::Struct(fields) = &cfg.kind else {
        panic!()
    };
    assert_eq!(
        fields.iter().map(|f| f.key.as_str()).collect::<Vec<_>>(),
        ["x-y", "max-conns"]
    );
    let DefKind::Enum(variants) = &inv.defs["E"].kind else {
        panic!()
    };
    assert_eq!(
        variants.iter().map(|v| v.key.as_str()).collect::<Vec<_>>(),
        ["A", "bee", "C"]
    );
    // The emitted definitions keep other attributes and drop #[bake].
    let emitted = inv
        .passthrough
        .iter()
        .map(|i| quote::quote!(#i).to_string())
        .collect::<String>();
    assert!(emitted.contains("derive"));
    assert!(!emitted.contains("bake"));
}

#[test]
fn invocation_errors() {
    assert_eq!(
        parse_err(r#"static mut A: u8 = "a.json";"#),
        "static mut isn't supported"
    );
    assert_eq!(
        parse_err(r#"static A: u8 = 5;"#),
        r#"expected a path string literal, like "data/app.json""#
    );
    assert_eq!(
        parse_err(r#"static A: u8 = "a" as json::x;"#),
        "expected a format name after `as`, like `as json`"
    );
    assert_eq!(
        parse_err("struct S<T> { t: T }"),
        "generic types aren't supported"
    );
    assert_eq!(
        parse_err("fn f() {}"),
        "bake! takes static, const, struct and enum items"
    );
    assert_eq!(
        parse_err("struct S { s: String }"),
        "String isn't const; use &'static str"
    );
    assert_eq!(
        parse_err("struct S { s: Vec<u8> }"),
        "Vec isn't const; use &'static [T]"
    );
    assert_eq!(
        parse_err("struct S { s: std::collections::HashMap<u8, u8> }"),
        "HashMap isn't const; use &'static [(K, V)]"
    );
    assert_eq!(
        parse_err("struct S { s: &'a str }"),
        "use &'static, or no lifetime"
    );
    assert_eq!(parse_err("struct S { s: &mut u8 }"), "&mut isn't supported");
    assert_eq!(
        parse_err("struct S { s: Foo }"),
        "type `Foo` isn't defined in this bake! invocation"
    );
    assert_eq!(
        parse_err("struct S { s: Box<u8> }"),
        "type `Box` isn't supported"
    );
    assert_eq!(
        parse_err("#[bake(rename_all = \"Kebab\")] struct S;"),
        r#"unknown rename_all style "Kebab"; expected one of "lowercase", "UPPERCASE", "snake_case", "SCREAMING_SNAKE_CASE", "kebab-case", "SCREAMING-KEBAB-CASE", "camelCase", "PascalCase""#
    );
    assert_eq!(
        parse_err("struct S { #[bake(name = \"x\")] s: u8 }"),
        "unknown bake attribute `name`; expected rename or body"
    );
    assert_eq!(
        parse_err("#[bake(rename = \"x\")] struct S;"),
        "unknown bake attribute `rename`; expected rename_all or allow_unknown"
    );
    assert_eq!(
        parse_err("struct S { #[bake(rename = \"x\")] a: u8, #[bake(rename = \"x\")] b: u8 }"),
        r#"two fields read the key "x""#
    );
    assert_eq!(
        parse_err("struct S; struct S;"),
        "type `S` is defined twice"
    );
    assert_eq!(
        parse_err("union S { a: u8 }"),
        "bake! takes static, const, struct and enum items"
    );
}

#[test]
fn empty_enum() {
    assert_eq!(parse_err("enum E {}"), "an enum needs at least one variant");
}
