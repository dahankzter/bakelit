use super::*;
use crate::items::parse_invocation;
use crate::ty::Ty;
use crate::value::{Node, Value};

fn node(value: Value) -> Node {
    Node { value, pos: None }
}
fn int(n: i128) -> Node {
    node(Value::Int {
        negative: n < 0,
        magnitude: n.unsigned_abs(),
    })
}
fn uint(m: u128) -> Node {
    node(Value::Int {
        negative: false,
        magnitude: m,
    })
}
fn float(f: f64) -> Node {
    node(Value::Float(f))
}
fn s(x: &str) -> Node {
    node(Value::Str(x.into()))
}
fn text(x: &str) -> Node {
    node(Value::Text(x.into()))
}
fn null() -> Node {
    node(Value::Null)
}
fn list(v: Vec<Node>) -> Node {
    node(Value::List(v))
}
fn map(v: Vec<(&str, Node)>) -> Node {
    node(Value::Map(
        v.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    ))
}
fn named(name: &str, payload: Node) -> Node {
    node(Value::Named(name.into(), Box::new(payload)))
}

/// Converts `n` into the Rust type `ty`, with the items in `defs` defined.
fn conv(defs: &str, ty: &str, n: &Node) -> Result<String, String> {
    let inv = parse_invocation(defs.parse().unwrap()).unwrap();
    let ty = Ty::from_syn(&syn::parse_str(ty).unwrap()).unwrap();
    let mut ctx = Ctx::new(&inv.defs);
    convert(n, &ty, &mut ctx)
        .map(|t| t.to_string().replace(' ', ""))
        .map_err(|e| format!("{}: {}", e.path, e.msg))
}
fn ok(ty: &str, n: &Node) -> String {
    conv("", ty, n).unwrap()
}
fn err(ty: &str, n: &Node) -> String {
    conv("", ty, n).unwrap_err()
}

#[test]
fn integers() {
    assert_eq!(ok("u8", &int(255)), "255u8");
    assert_eq!(err("u8", &int(256)), "$: expected u8, found 256");
    assert_eq!(err("u32", &int(-1)), "$: expected u32, found -1");
    assert_eq!(ok("i8", &int(-128)), "::core::primitive::i8::MIN");
    assert_eq!(ok("i8", &int(-127)), "-127i8");
    assert_eq!(err("i8", &int(-129)), "$: expected i8, found -129");
    assert_eq!(ok("u128", &uint(u128::MAX)), format!("{}u128", u128::MAX));
    assert_eq!(ok("i128", &int(i128::MIN)), "::core::primitive::i128::MIN");
    assert_eq!(
        ok("u64", &uint(u64::MAX as u128)),
        format!("{}u64", u64::MAX)
    );
    assert_eq!(
        err("i64", &uint(i64::MAX as u128 + 1)),
        "$: expected i64, found 9223372036854775808"
    );
    assert_eq!(ok("usize", &int(7)), "7usize");
    assert_eq!(err("i32", &float(1000.0)), "$: expected i32, found 1000.0");
}

#[test]
fn integers_from_text() {
    assert_eq!(ok("u16", &text("80")), "80u16");
    assert_eq!(ok("i16", &text("-80")), "-80i16");
    assert_eq!(
        err("u16", &text("+80")),
        r#"$: expected u16, found "+80"; write 80"#
    );
    assert_eq!(
        err("u16", &text("080")),
        r#"$: expected u16, found "080"; write 80"#
    );
    assert_eq!(err("u16", &text("8.0")), r#"$: expected u16, found "8.0""#);
    assert_eq!(
        err("u16", &text("")),
        "$: expected u16, found an empty field"
    );
    assert_eq!(ok("u8", &text("0")), "0u8");
}

#[test]
fn review_fixes() {
    assert_eq!(
        ok("isize", &int(i64::MIN as i128)),
        "-9223372036854775808isize"
    );
    assert_eq!(
        err("f64", &text("007")),
        r#"$: expected f64, found "007"; write 7"#
    );
    assert_eq!(ok("f32", &text("-0")), "-0f32");
    assert_eq!(ok("f64", &text("-0.0")), "-0f64");
    assert_eq!(ok("f32", &float(0.10000000149011612)), "0.1f32");
    assert_eq!(
        err("f64", &text("1e-400")),
        "$: 1e-400 is too small for f64; it would be 0"
    );
    assert_eq!(
        conv(
            "enum E { A }",
            "Option<Option<u8>>",
            &named("Some", list(vec![null()]))
        )
        .unwrap(),
        "::core::option::Option::Some(::core::option::Option::None)"
    );
    assert_eq!(
        err("u16", &named("Some", list(vec![int(80)]))),
        "$: expected u16, found Some(...)"
    );
    assert_eq!(
        ok("Option<u16>", &named("Some", list(vec![int(80)]))),
        "::core::option::Option::Some(80u16)"
    );
}

#[test]
fn floats() {
    assert_eq!(ok("f64", &float(1.5)), "1.5f64");
    assert_eq!(ok("f64", &float(-1.5)), "-1.5f64");
    assert_eq!(ok("f64", &int(5)), "5f64");
    assert_eq!(
        err("f64", &int(9_007_199_254_740_993)),
        "$: 9007199254740993 isn't exactly representable as f64"
    );
    assert_eq!(
        err("f32", &int(16_777_217)),
        "$: 16777217 isn't exactly representable as f32"
    );
    assert_eq!(ok("f32", &float(0.1)), "0.1f32");
    assert_eq!(
        err("f32", &float(0.123456789)),
        "$: 0.123456789 doesn't fit f32 exactly; use f64"
    );
    assert_eq!(
        ok("f64", &float(f64::INFINITY)),
        "::core::primitive::f64::INFINITY"
    );
    assert_eq!(
        ok("f64", &float(f64::NEG_INFINITY)),
        "::core::primitive::f64::NEG_INFINITY"
    );
    assert_eq!(ok("f32", &float(f64::NAN)), "::core::primitive::f32::NAN");
    assert_eq!(ok("f64", &text("2.5")), "2.5f64");
    assert_eq!(ok("f64", &text("2")), "2f64");
    assert_eq!(err("f64", &text("inf")), r#"$: expected f64, found "inf""#);
    assert_eq!(
        err("f64", &text("+2")),
        r#"$: expected f64, found "+2"; write 2"#
    );
}

#[test]
fn bools_chars_strings() {
    assert_eq!(ok("bool", &node(Value::Bool(true))), "true");
    assert_eq!(ok("bool", &text("false")), "false");
    assert_eq!(
        err("bool", &s("yes")),
        r#"$: expected bool, found "yes"; write true or false"#
    );
    assert_eq!(
        err("bool", &text("TRUE")),
        r#"$: expected bool, found "TRUE"; write true or false"#
    );
    assert_eq!(err("bool", &int(1)), "$: expected bool, found 1");
    assert_eq!(ok("char", &s("é")), "'é'");
    assert_eq!(ok("char", &node(Value::Char('x'))), "'x'");
    assert_eq!(err("char", &s("ab")), r#"$: expected char, found "ab""#);
    assert_eq!(ok("&'static str", &s("a\"b")), r#""a\"b""#);
    assert_eq!(ok("&str", &text("")), r#""""#);
    assert_eq!(err("&str", &int(5)), "$: expected &str, found 5");
    assert_eq!(err("&str", &null()), "$: expected &str, found null");
    assert_eq!(err("&str", &list(vec![])), "$: expected &str, found a list");
    assert_eq!(
        err("&str", &map(vec![])),
        "$: expected &str, found a mapping"
    );
}

#[test]
fn options_and_collections() {
    assert_eq!(ok("Option<u8>", &null()), "::core::option::Option::None");
    assert_eq!(
        ok("Option<u8>", &int(5)),
        "::core::option::Option::Some(5u8)"
    );
    assert_eq!(ok("Option<u8>", &text("")), "::core::option::Option::None");
    assert_eq!(err("u8", &null()), "$: expected u8, found null");
    assert_eq!(
        ok("&'static [u8]", &list(vec![int(1), int(2)])),
        "&[1u8,2u8]"
    );
    assert_eq!(ok("&[u8]", &list(vec![])), "&[]");
    assert_eq!(
        err("[u8; 3]", &list(vec![int(1), int(2)])),
        "$: expected 3 elements, found 2"
    );
    assert_eq!(ok("[u8; 2]", &list(vec![int(1), int(2)])), "[1u8,2u8]");
    assert_eq!(
        ok("(u8, &str)", &list(vec![int(1), s("a")])),
        r#"(1u8,"a")"#
    );
    assert_eq!(
        err("(u8, &str)", &list(vec![int(1)])),
        "$: expected 2 elements, found 1"
    );
    assert_eq!(ok("()", &null()), "()");
    assert_eq!(
        ok("&[(&str, u32)]", &map(vec![("a", int(1)), ("b", int(2))])),
        r#"&[("a",1u32),("b",2u32)]"#
    );
    assert_eq!(
        ok("&[(u8, bool)]", &map(vec![("7", node(Value::Bool(true)))])),
        "&[(7u8,true)]"
    );
    assert_eq!(
        err("&[u8]", &list(vec![int(1), s("x")])),
        r#"$[1]: expected u8, found "x""#
    );
    assert_eq!(err("&[u8]", &int(1)), "$: expected a list of u8, found 1");
}

const STRUCTS: &str = r#"
    struct S { port: u16, name: Option<&'static str> }
    #[bake(allow_unknown)]
    struct Loose { port: u16 }
    struct R { #[bake(rename = "max-conns")] max: u32 }
    #[bake(rename_all = "kebab-case")]
    struct K { max_conns: u32, r#type: u8 }
    struct T(u8, &'static str);
    struct U;
    struct Outer { servers: &'static [S] }
"#;

#[test]
fn structs() {
    let c = |ty: &str, n: &Node| conv(STRUCTS, ty, n);
    assert_eq!(
        c("S", &map(vec![("port", int(80))])).unwrap(),
        "S{port:80u16,name:::core::option::Option::None}"
    );
    assert_eq!(
        c("S", &map(vec![])).unwrap_err(),
        r#"$: missing key "port""#
    );
    assert_eq!(
        c("S", &map(vec![("port", int(1)), ("prot", int(2))])).unwrap_err(),
        r#"$.prot: unknown key; did you mean "port"?"#
    );
    assert_eq!(
        c("S", &map(vec![("port", int(1)), ("zzzzzz", int(2))])).unwrap_err(),
        r#"$.zzzzzz: unknown key; expected one of "port", "name""#
    );
    assert_eq!(
        c("Loose", &map(vec![("port", int(1)), ("x", int(2))])).unwrap(),
        "Loose{port:1u16}"
    );
    assert_eq!(
        c("R", &map(vec![("max-conns", int(3))])).unwrap(),
        "R{max:3u32}"
    );
    assert_eq!(
        c("R", &map(vec![("max_conns", int(3))])).unwrap_err(),
        r#"$.max_conns: unknown key; did you mean "max-conns"?"#
    );
    assert_eq!(
        c("K", &map(vec![("max-conns", int(3)), ("type", int(1))])).unwrap(),
        "K{max_conns:3u32,r#type:1u8}"
    );
    assert_eq!(
        c("T", &list(vec![int(1), s("a")])).unwrap(),
        r#"T(1u8,"a")"#
    );
    assert_eq!(c("U", &null()).unwrap(), "U");
    assert_eq!(
        c(
            "Outer",
            &map(vec![(
                "servers",
                list(vec![
                    map(vec![("port", int(1))]),
                    map(vec![("port", s("x"))])
                ])
            )])
        )
        .unwrap_err(),
        r#"$.servers[1].port: expected u16, found "x""#
    );
    assert_eq!(c("S", &int(1)).unwrap_err(), "$: expected S, found 1");
    assert_eq!(
        c("S", &named("S", map(vec![("port", int(1))]))).unwrap(),
        "S{port:1u16,name:::core::option::Option::None}"
    );
    assert_eq!(
        c("S", &named("T", map(vec![("port", int(1))]))).unwrap_err(),
        "$: expected S, found T(...)"
    );
    assert_eq!(
        c("T", &named("T", list(vec![int(1), s("a")]))).unwrap(),
        r#"T(1u8,"a")"#
    );
    assert_eq!(c("U", &named("U", null())).unwrap(), "U");
    assert_eq!(
        c("&[(&str, S)]", &map(vec![("odd key", map(vec![]))])).unwrap_err(),
        r#"$["odd key"]: missing key "port""#
    );
}

const ENUMS: &str = r#"
    enum L { Debug, Info }
    #[bake(rename_all = "lowercase")]
    enum Low { Debug, #[bake(rename = "information")] Info }
    enum Shape { Circle { r: f64 }, Pair(u8, u8), Wrap(u8), Dot }
"#;

#[test]
fn enums() {
    let c = |ty: &str, n: &Node| conv(ENUMS, ty, n);
    assert_eq!(c("L", &s("Info")).unwrap(), "L::Info");
    assert_eq!(c("L", &text("Debug")).unwrap(), "L::Debug");
    assert_eq!(
        c("L", &s("info")).unwrap_err(),
        r#"$: unknown variant "info"; did you mean "Info"?"#
    );
    assert_eq!(c("Low", &s("debug")).unwrap(), "Low::Debug");
    assert_eq!(c("Low", &s("information")).unwrap(), "Low::Info");
    assert_eq!(
        c(
            "Shape",
            &map(vec![("Circle", map(vec![("r", float(1.5))]))])
        )
        .unwrap(),
        "Shape::Circle{r:1.5f64}"
    );
    assert_eq!(
        c("Shape", &map(vec![("Pair", list(vec![int(1), int(2)]))])).unwrap(),
        "Shape::Pair(1u8,2u8)"
    );
    assert_eq!(
        c("Shape", &map(vec![("Wrap", int(5))])).unwrap(),
        "Shape::Wrap(5u8)"
    );
    assert_eq!(c("Shape", &s("Dot")).unwrap(), "Shape::Dot");
    assert_eq!(
        c("Shape", &s("Circle")).unwrap_err(),
        r#"$: variant "Circle" needs data, like {"Circle": ...}"#
    );
    assert_eq!(
        c("Shape", &map(vec![("Dot", null())])).unwrap_err(),
        r#"$: variant "Dot" has no data; write "Dot""#
    );
    assert_eq!(
        c("Shape", &map(vec![("Wrap", int(1)), ("Dot", null())])).unwrap_err(),
        r#"$: expected one variant, like {"Circle": ...}, found a mapping with 2 keys"#
    );
    assert_eq!(
        c("Shape", &map(vec![("Circle", map(vec![("r", s("x"))]))])).unwrap_err(),
        r#"$.Circle.r: expected f64, found "x""#
    );
    assert_eq!(
        c("Shape", &named("Circle", map(vec![("r", float(2.0))]))).unwrap(),
        "Shape::Circle{r:2f64}"
    );
    assert_eq!(c("Shape", &named("Dot", null())).unwrap(), "Shape::Dot");
    assert_eq!(
        c("Shape", &named("Pair", list(vec![int(1), int(2)]))).unwrap(),
        "Shape::Pair(1u8,2u8)"
    );
    assert_eq!(
        c("Shape", &named("Wrap", list(vec![int(5)]))).unwrap(),
        "Shape::Wrap(5u8)"
    );
    assert_eq!(
        c("Shape", &named("Square", null())).unwrap_err(),
        r#"$: unknown variant "Square"; expected one of "Circle", "Pair", "Wrap", "Dot""#
    );
}
