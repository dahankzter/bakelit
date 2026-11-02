use super::*;
use crate::value::{Pos, Value};

fn one(text: &str) -> Result<Value, String> {
    parse_toml(text).map(|n| n.value).map_err(|e| e.msg)
}
fn get(text: &str) -> Value {
    let Value::Map(m) = one(text).unwrap() else {
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
fn values() {
    assert_eq!(get("a = 1_000"), int(1000));
    assert_eq!(get("a = -9223372036854775808"), int(i64::MIN as i128));
    assert_eq!(get("a = 9223372036854775807"), int(i64::MAX as i128));
    assert_eq!(get("a = 0xff"), int(255));
    assert_eq!(get("a = 0o17"), int(15));
    assert_eq!(get("a = 0b101"), int(5));
    assert_eq!(get("a = +5"), int(5));
    assert_eq!(get("a = 1.5e3"), Value::Float(1500.0));
    assert_eq!(get("a = -inf"), Value::Float(f64::NEG_INFINITY));
    assert!(matches!(get("a = nan"), Value::Float(f) if f.is_nan()));
    assert_eq!(get("a = 'lit\\n'"), Value::Str("lit\\n".into()));
    assert_eq!(get("a = \"esc\\n\""), Value::Str("esc\n".into()));
    assert_eq!(
        get("a = 1979-05-27T07:32:00Z"),
        Value::Str("1979-05-27T07:32:00Z".into())
    );
    assert_eq!(get("a = 07:32:00"), Value::Str("07:32:00".into()));
    assert_eq!(get("a = true"), Value::Bool(true));
    assert_eq!(
        get("a = [1, \"x\"]"),
        Value::List(vec![
            Node::new(int(1), Some(Pos { line: 1, col: 6 })),
            Node::new(Value::Str("x".into()), Some(Pos { line: 1, col: 9 }))
        ])
    );
}

#[test]
fn tables_and_positions() {
    let text = "title = \"t\"\n\n[owner]\nname = \"n\"\n\n[[servers]]\nhost = \"a\"\n\n[[servers]]\nhost = \"b\"\nport = 81\n";
    let n = parse_toml(text).unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(
        m.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["title", "owner", "servers"]
    );
    let Value::List(servers) = &m[2].1.value else {
        panic!()
    };
    let Value::Map(second) = &servers[1].value else {
        panic!()
    };
    assert_eq!(second[1].1.pos, Some(Pos { line: 11, col: 8 }));
    let Value::Map(m) = one("a.b.c = 1\n").unwrap() else {
        panic!()
    };
    let Value::Map(b) = &m[0].1.value else {
        panic!()
    };
    assert_eq!(b[0].0, "b");
}

#[test]
fn toml_errors() {
    let e = parse_toml("a = 1\na = 2\n").unwrap_err();
    assert_eq!(e.pos.map(|p| p.line), Some(2));
    assert!(parse_toml("a = 18446744073709551615\n").is_err());
    assert!(parse_toml("a = [1,\n").is_err());
}

#[test]
fn review_fixes() {
    assert_eq!(
        one("f = 1e400").unwrap_err(),
        "1e400 is out of range for f64"
    );
    assert_eq!(
        one("f = 1e-400").unwrap_err(),
        "1e-400 is too small for f64; it would be 0"
    );
    assert_eq!(get("f = -inf"), Value::Float(f64::NEG_INFINITY));
}
