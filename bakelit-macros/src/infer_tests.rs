use super::*;
use crate::formats::json::parse_json;
use crate::ty::Ty;
use crate::value::{Node, Value};

/// The generated definitions (squashed) and the root type, for JSON `text`.
fn infer_json(text: &str, item_ty: &str) -> Result<(Vec<String>, String), String> {
    let node = parse_json(text).unwrap();
    let ty = Ty::from_syn(&syn::parse_str(item_ty).unwrap()).unwrap();
    let inferred = infer(&node, &ty, &Default::default(), TextRule::Guess, false)
        .map_err(|e| format!("{}: {}", e.path, e.msg))?;
    let items = inferred
        .items
        .iter()
        .map(|i| quote::quote!(#i).to_string().replace(' ', ""))
        .collect();
    Ok((
        items,
        inferred.defs.keys().cloned().collect::<Vec<_>>().join(","),
    ))
}

const DERIVE: &str = "#[derive(::core::fmt::Debug,::core::clone::Clone,::core::marker::Copy,::core::cmp::PartialEq)]";

#[test]
fn structs_and_fields() {
    let (items, names) = infer_json(
        r#"{"name": "a", "port": 80, "ratio": 1.5, "on": true, "tags": ["x"],
            "servers": [{"host": "a", "max-conns": 1}, {"host": "b", "w": 2.5}, {"host": "c", "w": 3}],
            "owner": {"id": 18446744073709551615, "maxConns": -1}, "type": "t"}"#,
        "Config",
    )
    .unwrap();
    assert_eq!(names, "Config,ConfigOwner,ConfigServers");
    assert_eq!(
        items,
        [
            format!(
                "{DERIVE}pubstructConfig{{pubname:&'static::core::primitive::str,pubport:::core::primitive::i64,pubratio:::core::primitive::f64,pubon:::core::primitive::bool,pubtags:&'static[&'static::core::primitive::str],pubservers:&'static[ConfigServers],pubowner:ConfigOwner,pubr#type:&'static::core::primitive::str}}"
            ),
            format!(
                "{DERIVE}pubstructConfigServers{{pubhost:&'static::core::primitive::str,pubmax_conns:::core::option::Option<::core::primitive::i64>,pubw:::core::option::Option<::core::primitive::f64>}}"
            ),
            format!(
                "{DERIVE}pubstructConfigOwner{{pubid:::core::primitive::u64,pubmax_conns:::core::primitive::i64}}"
            ),
        ]
    );
}

#[test]
fn nulls_and_lists() {
    assert_eq!(
        infer_json(r#"{"b": null}"#, "R").unwrap_err(),
        "$.b: only null here; can't infer a type, so write the types yourself"
    );
    let (items, _) = infer_json(r#"{"a": [1, null, 2], "c": [[], [1]]}"#, "R").unwrap();
    assert_eq!(
        items,
        [format!(
            "{DERIVE}pubstructR{{puba:&'static[::core::option::Option<::core::primitive::i64>],pubc:&'static[&'static[::core::primitive::i64]]}}"
        )]
    );
    let (items, _) = infer_json(r#"[{"k": 1}, {"k": null}]"#, "&[Row]").unwrap();
    assert_eq!(
        items,
        [format!(
            "{DERIVE}pubstructRow{{pubk:::core::option::Option<::core::primitive::i64>}}"
        )]
    );
}

#[test]
fn inference_errors() {
    let e = |t: &str, ty: &str| infer_json(t, ty).unwrap_err();
    assert_eq!(
        e(r#"{"c": []}"#, "R"),
        "$.c: an empty list; can't infer its element type, so write the types yourself"
    );
    assert_eq!(
        e(r#"{"c": [1, "x"]}"#, "R"),
        r#"$.c[1]: found "x" after integers; write the types yourself"#
    );
    assert_eq!(
        e(r#"{"c": [{"a": 1}, 2]}"#, "R"),
        "$.c[1]: found 2 after a mapping; write the types yourself"
    );
    assert_eq!(
        e(r#"{"c": -9223372036854775809}"#, "R"),
        "$.c: -9223372036854775809 doesn't fit i64; write the types yourself"
    );
    assert_eq!(
        e(r#"{"c": 18446744073709551616}"#, "R"),
        "$.c: 18446744073709551616 doesn't fit u64; write the types yourself"
    );
    assert_eq!(
        e(r#"{"max-conns": 1, "max_conns": 2}"#, "R"),
        r#"$: the keys "max-conns" and "max_conns" both become the field max_conns; write the types yourself and use #[bake(rename = "...")]"#
    );
    assert_eq!(
        e(r#"{"1st": 1}"#, "R"),
        r#"$: the key "1st" can't be a field name; write the types yourself and use #[bake(rename = "1st")]"#
    );
    assert_eq!(
        e(r#"{"self": 1}"#, "R"),
        r#"$: the key "self" can't be a field name; write the types yourself and use #[bake(rename = "self")]"#
    );
    assert_eq!(
        e(r#"[1, 2]"#, "R"),
        "$: the data's type is &[i64], not R; write that type, or define R"
    );
    assert_eq!(
        e(r#"{"a": 1}"#, "&[R]"),
        "$: the data's type is R, not &[R]; write that type, or define R"
    );
    assert_eq!(
        e(r#"5"#, "R"),
        "$: the data's type is i64, not R; write that type, or define R"
    );
}

#[test]
fn field_names() {
    assert_eq!(field_name("maxConns").as_deref(), Some("max_conns"));
    assert_eq!(field_name("Max Conns").as_deref(), Some("max_conns"));
    assert_eq!(field_name("HTTPServer").as_deref(), Some("httpserver"));
    assert_eq!(field_name("a.b").as_deref(), Some("a_b"));
    assert_eq!(field_name("x2y").as_deref(), Some("x2y"));
    assert_eq!(field_name("_x").as_deref(), Some("_x"));
    assert_eq!(field_name("_").as_deref(), None);
    assert_eq!(field_name("é").as_deref(), None);
    assert_eq!(field_name("").as_deref(), None);
}

#[cfg(feature = "csv")]
#[test]
fn csv_columns() {
    let node = crate::formats::csv::parse_csv(
        "id,zip,price,ok,name,note,mixed\n1,007,1.5,true,a,,1\n-2,123,2,false,b,x,y\n",
        b',',
    )
    .unwrap();
    let ty = Ty::from_syn(&syn::parse_str("&[Row]").unwrap()).unwrap();
    let inferred = infer(&node, &ty, &Default::default(), TextRule::Guess, false).unwrap();
    let item = &inferred.items[0];
    assert_eq!(
        quote::quote!(#item).to_string().replace(' ', ""),
        format!(
            "{DERIVE}pubstructRow{{pubid:::core::primitive::i64,pubzip:&'static::core::primitive::str,pubprice:::core::primitive::f64,pubok:::core::primitive::bool,pubname:&'static::core::primitive::str,pubnote:::core::option::Option<&'static::core::primitive::str>,pubmixed:&'static::core::primitive::str}}"
        )
    );
}

#[test]
fn text_without_guessing() {
    let node = Node::new(
        Value::Map(vec![(
            "port".into(),
            Node::new(Value::Text("80".into()), None),
        )]),
        None,
    );
    let ty = Ty::from_syn(&syn::parse_str("Env").unwrap()).unwrap();
    let inferred = infer(&node, &ty, &Default::default(), TextRule::Str, false).unwrap();
    let item = &inferred.items[0];
    assert_eq!(
        quote::quote!(#item).to_string().replace(' ', ""),
        format!("{DERIVE}pubstructEnv{{pubport:&'static::core::primitive::str}}")
    );
}

#[cfg(feature = "ron")]
#[test]
fn review_fixes() {
    let e = |t: &str, ty: &str| {
        let node = parse_json(t).unwrap();
        let ty = Ty::from_syn(&syn::parse_str(ty).unwrap()).unwrap();
        let Err(e) = infer(&node, &ty, &Default::default(), TextRule::Guess, false) else {
            panic!("{t}")
        };
        (
            e.pos.map(|p| (p.line, p.col)),
            format!("{}: {}", e.path, e.msg),
        )
    };
    assert_eq!(
        e(
            "{\"xs\": [{\"a\": 1}, {\"a\": null},\n {\"a\": null}], \"n\": [null]}",
            "R"
        )
        .1,
        "$.n[]: only null here; can't infer a type, so write the types yourself"
    );
    assert_eq!(
        e("[{\"a\": 1},\n {\"a\": 18446744073709551616}]", "&[R]"),
        (
            Some((2, 8)),
            "$[].a: 18446744073709551616 doesn't fit u64; write the types yourself".into()
        )
    );
    assert_eq!(e("{\"xs\":\n  []}", "R").0, Some((2, 3)));
    let node = crate::formats::ron::parse_ron("(a: Some(Some(1)), b: [Some(1), None])").unwrap();
    let ty = Ty::from_syn(&syn::parse_str("R").unwrap()).unwrap();
    let inferred = infer(&node, &ty, &Default::default(), TextRule::Str, false).unwrap();
    let item = &inferred.items[0];
    assert!(quote::quote!(#item).to_string().replace(' ', "").contains(
        "puba:::core::option::Option<::core::option::Option<::core::primitive::i64>>,pubb:&'static[::core::option::Option<::core::primitive::i64>]"
    ));
}
