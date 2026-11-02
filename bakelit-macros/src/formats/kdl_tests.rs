use super::*;
use crate::value::{Pos, Value};

fn v(text: &str) -> Result<Value, String> {
    parse_kdl(text).map(|n| n.value).map_err(|e| e.msg)
}
fn int(n: i128) -> Value {
    Value::Int {
        negative: n < 0,
        magnitude: n.unsigned_abs(),
    }
}
fn keys(v: &Value) -> Vec<&str> {
    let Value::Map(m) = v else { panic!("{v:?}") };
    m.iter().map(|(k, _)| k.as_str()).collect()
}
fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    let Value::Map(m) = v else { panic!("{v:?}") };
    &m.iter().find(|(k, _)| k == key).unwrap().1.value
}

#[test]
fn mapping_rules() {
    let doc = v(r#"
        name "demo"
        port 8080
        ratio 0.5
        debug #true
        nothing #null
        tags "a" "b"
        server host="a" port=80
        db {
            name "main"
            pool 4
        }
        mixed size=1 {
            depth 2
        }
        peer "x"
        peer "y"
        list {
            - "one"
        }
        flag
    "#)
    .unwrap();
    assert_eq!(
        keys(&doc),
        [
            "name", "port", "ratio", "debug", "nothing", "tags", "server", "db", "mixed", "peer",
            "list", "flag"
        ]
    );
    assert_eq!(get(&doc, "name"), &Value::Str("demo".into()));
    assert_eq!(get(&doc, "port"), &int(8080));
    assert_eq!(get(&doc, "ratio"), &Value::Float(0.5));
    assert_eq!(get(&doc, "debug"), &Value::Bool(true));
    assert_eq!(get(&doc, "nothing"), &Value::Null);
    assert!(matches!(get(&doc, "tags"), Value::List(l) if l.len() == 2));
    assert_eq!(keys(get(&doc, "server")), ["host", "port"]);
    assert_eq!(keys(get(&doc, "db")), ["name", "pool"]);
    assert_eq!(keys(get(&doc, "mixed")), ["size", "depth"]);
    assert!(matches!(get(&doc, "peer"), Value::List(l) if l.len() == 2));
    assert!(matches!(get(&doc, "list"), Value::List(l) if l.len() == 1));
    assert_eq!(get(&doc, "flag"), &Value::Null);
}

#[test]
fn positions_and_errors() {
    let n = parse_kdl("a 1\n  é \"x\"\n").unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(m[1].1.pos, Some(Pos { line: 2, col: 5 }));
    assert_eq!(
        v("a 1 b=2").unwrap_err(),
        "a node with arguments can't also have properties or children"
    );
    assert_eq!(
        v("a 1 { b 2 }").unwrap_err(),
        "a node with arguments can't also have properties or children"
    );
    assert_eq!(
        v("a b=1 { b 2 }").unwrap_err(),
        r#"the key "b" is both a property and a child"#
    );
    assert_eq!(v("a x=1 x=2").unwrap_err(), r#"duplicate key "x""#);
    assert_eq!(
        v("a (u8)5").unwrap_err(),
        "type annotations like (u8) aren't supported"
    );
    assert_eq!(
        v("(t)a 5").unwrap_err(),
        "type annotations like (t) aren't supported"
    );
    assert_eq!(
        v("list { - 1; x 2 }").unwrap_err(),
        r#"a list's children are all named "-""#
    );
    assert_eq!(
        v("a true").unwrap_err(),
        "this looks like KDL 1; bakelit reads KDL 2"
    );
    assert!(parse_kdl("a {").unwrap_err().pos.is_some());
    assert_eq!(
        v("a 1; a { b 2 }").map(|v| matches!(get(&v, "a"), Value::List(_))),
        Ok(true)
    );
}

#[test]
fn review_fixes() {
    let deep = format!("{}{}", "n {".repeat(3000), "}".repeat(3000));
    assert_eq!(v(&deep).unwrap_err(), "nested deeper than 128 levels");
    let e = parse_kdl("ääää \"x\"\nport \"y\n").unwrap_err();
    assert_eq!(e.pos.map(|p| p.line), Some(2));
    let n = parse_kdl("node port = \"y\"").unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    let Value::Map(props) = &m[0].1.value else {
        panic!()
    };
    assert_eq!(props[0].1.pos, Some(Pos { line: 1, col: 13 }));
    assert!(matches!(v("- 1\n- 2").unwrap(), Value::List(l) if l.len() == 2));
    assert_eq!(v("a 1e400").unwrap_err(), "1e400 is out of range for f64");
    assert_eq!(
        v("a 1e-400").unwrap_err(),
        "1e-400 is too small for f64; it would be 0"
    );
    assert_eq!(
        v("a #inf").unwrap(),
        Value::Map(vec![(
            "a".into(),
            Node::new(Value::Float(f64::INFINITY), Some(Pos { line: 1, col: 3 }))
        )])
    );
}
