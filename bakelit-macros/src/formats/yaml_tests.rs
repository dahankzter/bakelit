use super::*;
use crate::value::{Pos, Value};

fn one(text: &str) -> Result<Value, String> {
    parse_yaml(text).map(|n| n.value).map_err(|e| e.msg)
}
fn scalar(raw: &str) -> Value {
    let Value::Map(m) = one(&format!("a: {raw}\n")).unwrap() else {
        panic!()
    };
    m.into_iter().next().unwrap().1.value
}
fn int(n: i128) -> Value {
    Value::Int {
        negative: n < 0,
        magnitude: n.unsigned_abs(),
    }
}

#[test]
fn core_schema() {
    for null in ["", "~", "null", "Null", "NULL"] {
        assert_eq!(scalar(null), Value::Null, "{null:?}");
    }
    for (raw, b) in [
        ("true", true),
        ("True", true),
        ("TRUE", true),
        ("false", false),
        ("FALSE", false),
    ] {
        assert_eq!(scalar(raw), Value::Bool(b));
    }
    for (raw, n) in [
        ("12", 12),
        ("-12", -12),
        ("+12", 12),
        ("012", 12),
        ("0o17", 15),
        ("0x1F", 31),
        ("0", 0),
        ("-0", 0),
    ] {
        assert_eq!(scalar(raw), int(n), "{raw}");
    }
    assert_eq!(
        scalar("18446744073709551615"),
        Value::Int {
            negative: false,
            magnitude: u64::MAX as u128
        }
    );
    assert_eq!(
        scalar("340282366920938463463374607431768211455"),
        Value::Int {
            negative: false,
            magnitude: u128::MAX
        }
    );
    for (raw, f) in [
        ("1.5", 1.5),
        ("-1.5", -1.5),
        (".5", 0.5),
        ("1e3", 1000.0),
        ("1.", 1.0),
        ("+.inf", f64::INFINITY),
        ("-.Inf", f64::NEG_INFINITY),
    ] {
        assert_eq!(scalar(raw), Value::Float(f), "{raw}");
    }
    assert!(matches!(scalar(".nan"), Value::Float(f) if f.is_nan()));
    for s in [
        "yes",
        "no",
        "on",
        "off",
        "1_000",
        "0b101",
        "1:30",
        "2026-11-02",
        "~x",
        "True!",
    ] {
        assert_eq!(scalar(s), Value::Str(s.into()), "{s}");
    }
    assert_eq!(scalar("'5'"), Value::Str("5".into()));
    assert_eq!(scalar("\"true\""), Value::Str("true".into()));
    assert_eq!(scalar("!!str 5"), Value::Str("5".into()));
    assert_eq!(scalar("!!int \"5\""), int(5));
    assert_eq!(scalar("!!float 5"), Value::Float(5.0));
    assert_eq!(one("a: !!int x\n").unwrap_err(), r#""x" isn't an integer"#);
    assert_eq!(
        one("a: !custom x\n").unwrap_err(),
        "the tag !custom isn't supported"
    );
}

#[test]
fn structure_and_positions() {
    let n = parse_yaml(
        "servers:\n  - host: a\n    port: 80\n  - &b {host: b, port: 81}\nbackup: *b\n7: x\n",
    )
    .unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(
        m.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["servers", "backup", "7"]
    );
    let Value::List(servers) = &m[0].1.value else {
        panic!()
    };
    let Value::Map(first) = &servers[0].value else {
        panic!()
    };
    assert_eq!(first[1].1.pos, Some(Pos { line: 3, col: 11 }));
    assert_eq!(m[1].1.value, servers[1].value);
    let text = "a: |\n  line 1\n  line 2\nb: >-\n  folded\n  text\n";
    let Value::Map(m) = one(text).unwrap() else {
        panic!()
    };
    assert_eq!(m[0].1.value, Value::Str("line 1\nline 2\n".into()));
    assert_eq!(m[1].1.value, Value::Str("folded text".into()));
}

#[test]
fn yaml_errors() {
    assert_eq!(
        one("a: 1\n---\nb: 2\n").unwrap_err(),
        "the file has 2 YAML documents; bake! reads one"
    );
    assert_eq!(
        one("base: &b {x: 1}\nd:\n  <<: *b\n").unwrap_err(),
        "merge keys (<<) aren't supported; write the keys out"
    );
    assert_eq!(
        one("? [a, b]\n: 1\n").unwrap_err(),
        "a key must be a scalar"
    );
    assert!(one("a: 1\na: 2\n").is_err());
    assert!(one("a: [1\n").is_err());
    let e = parse_yaml("a: 1\n b: 2\n").unwrap_err();
    assert!(e.pos.is_some());
    assert_eq!(one("").unwrap(), Value::Null);
}

#[test]
fn review_fixes() {
    assert_eq!(one("a: 1\n\"a\": 2\n").unwrap_err(), r#"duplicate key "a""#);
    assert_eq!(one("1: a\n\"1\": b\n").unwrap_err(), r#"duplicate key "1""#);
    assert_eq!(
        one("a: 340282366920938463463374607431768211456\n").unwrap_err(),
        "340282366920938463463374607431768211456 is too large for any integer type"
    );
    assert_eq!(
        one("a: 1e400\n").unwrap_err(),
        "1e400 is out of range for f64"
    );
    assert_eq!(
        one("a: 1e-400\n").unwrap_err(),
        "1e-400 is too small for f64; it would be 0"
    );
    assert_eq!(
        one("!custom k: 1\n").unwrap_err(),
        "the tag !custom isn't supported"
    );
}


