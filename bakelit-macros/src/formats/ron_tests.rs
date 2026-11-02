use super::*;
use crate::value::{Pos, Value};

fn v(text: &str) -> Result<Value, String> {
    parse_ron(text).map(|n| n.value).map_err(|e| e.msg)
}
fn int(n: i128) -> Value {
    Value::Int {
        negative: n < 0,
        magnitude: n.unsigned_abs(),
    }
}
fn node(value: Value, line: usize, col: usize) -> Node {
    Node::new(value, Some(Pos { line, col }))
}

#[test]
fn scalars() {
    assert_eq!(v("true").unwrap(), Value::Bool(true));
    assert_eq!(v("-17").unwrap(), int(-17));
    assert_eq!(v("1_000").unwrap(), int(1000));
    assert_eq!(v("0x1F").unwrap(), int(31));
    assert_eq!(v("0o17").unwrap(), int(15));
    assert_eq!(v("0b101").unwrap(), int(5));
    assert_eq!(
        v("340282366920938463463374607431768211455").unwrap(),
        Value::Int {
            negative: false,
            magnitude: u128::MAX
        }
    );
    assert_eq!(
        v("-170141183460469231731687303715884105728").unwrap(),
        int(i128::MIN)
    );
    assert_eq!(v("1.5").unwrap(), Value::Float(1.5));
    assert_eq!(v("-1e3").unwrap(), Value::Float(-1000.0));
    assert_eq!(v("inf").unwrap(), Value::Float(f64::INFINITY));
    assert_eq!(v("-inf").unwrap(), Value::Float(f64::NEG_INFINITY));
    assert!(matches!(v("NaN").unwrap(), Value::Float(f) if f.is_nan()));
    assert_eq!(v("'a'").unwrap(), Value::Char('a'));
    assert_eq!(v("'\\n'").unwrap(), Value::Char('\n'));
    assert_eq!(v("'\\u{e9}'").unwrap(), Value::Char('é'));
    assert_eq!(
        v("\"a\\\"b\\\\\\t\\u{1F600}\"").unwrap(),
        Value::Str("a\"b\\\t😀".into())
    );
    assert_eq!(
        v("r#\"raw \"quoted\"\"#").unwrap(),
        Value::Str("raw \"quoted\"".into())
    );
    assert_eq!(v("r\"x\"").unwrap(), Value::Str("x".into()));
    assert_eq!(v("None").unwrap(), Value::Null);
    assert!(
        matches!(v("Some(5)").unwrap(), Value::Named(n, p) if n == "Some" && matches!(&p.value, Value::List(l) if l[0].value == int(5)))
    );
    assert_eq!(v("()").unwrap(), Value::Null);
}

#[test]
fn structures() {
    assert_eq!(
        v("[1, 2,]").unwrap(),
        Value::List(vec![node(int(1), 1, 2), node(int(2), 1, 5)])
    );
    assert_eq!(
        v("(1, \"x\")").unwrap(),
        Value::List(vec![node(int(1), 1, 2), node(Value::Str("x".into()), 1, 5)])
    );
    assert_eq!(
        v("(a: 1)").unwrap(),
        Value::Map(vec![("a".into(), node(int(1), 1, 5))])
    );
    assert_eq!(
        v("{\"k\": 1, 2: 3}").unwrap(),
        Value::Map(vec![
            ("k".into(), node(int(1), 1, 7)),
            ("2".into(), node(int(3), 1, 13))
        ])
    );
    assert_eq!(
        v("Dot").unwrap(),
        Value::Named(
            "Dot".into(),
            Box::new(Node::new(Value::Null, Some(Pos { line: 1, col: 1 })))
        )
    );
    let Value::Named(name, payload) = v("Circle(r: 1.5, tags: [])").unwrap() else {
        panic!()
    };
    assert_eq!(name, "Circle");
    assert!(matches!(&payload.value, Value::Map(m) if m.len() == 2 && m[0].0 == "r"));
    let Value::Named(name, payload) = v("Pair(1, 2)").unwrap() else {
        panic!()
    };
    assert_eq!(name, "Pair");
    assert!(matches!(&payload.value, Value::List(l) if l.len() == 2));
    let Value::Named(_, payload) = v("Wrap(())").unwrap() else {
        panic!()
    };
    assert!(matches!(&payload.value, Value::List(l) if l.len() == 1));
    let Value::Named(name, _) = v("r#type").unwrap() else {
        panic!()
    };
    assert_eq!(name, "type");
    let n = parse_ron("// comment\n/* outer /* nested */ */ (\n  a: 1,\n)").unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(m[0].1.pos, Some(Pos { line: 3, col: 6 }));
}

#[test]
fn ron_errors() {
    assert_eq!(
        v("#![enable(implicit_some)]\n1").unwrap_err(),
        "RON extensions (#![enable(...)]) aren't supported"
    );
    assert_eq!(v("b\"x\"").unwrap_err(), "byte strings aren't supported");
    assert_eq!(
        v("{[1]: 2}").unwrap_err(),
        "a map key must be a string, a number, a char or a bool"
    );
    assert_eq!(v("(a: 1, a: 2)").unwrap_err(), r#"duplicate key "a""#);
    assert!(v("[1, 2").is_err());
    assert!(v("1 2").is_err());
    assert!(v("").is_err());
    assert!(v("'ab'").is_err());
    assert!(v("\"unterminated").is_err());
    assert!(v("/* open").is_err());
    assert_eq!(v("1..5").unwrap_err(), "ranges aren't supported");
    assert_eq!(
        v("1u8").unwrap_err(),
        "type suffixes aren't supported; write 1"
    );
    assert_eq!(v("1._5").unwrap_err(), "an unexpected underscore in 1._5");
    assert_eq!(v("\"\\b\"").unwrap_err(), "an unknown escape");
    assert_eq!(v("\"a\\\n   b\"").unwrap(), Value::Str("ab".into()));
    assert_eq!(v("1e_5").unwrap(), Value::Float(100000.0));
    let e = parse_ron("(\n  a: ,\n)").unwrap_err();
    assert_eq!(e.pos, Some(Pos { line: 2, col: 6 }));
}

/// Our structure with names dropped against `ron::Value`.
fn same(n: &Node, r: &ron::Value) -> bool {
    use ron::Value as R;
    let num = |r: &ron::Number| -> f64 { (*r).into_f64() };
    match (&n.value, r) {
        (Value::Named(name, payload), R::Option(Some(inner))) if name == "Some" => {
            match &payload.value {
                Value::List(l) if l.len() == 1 => same(&l[0], inner),
                _ => false,
            }
        }
        (Value::Named(_, payload), _) => same(payload, r),
        (Value::Null, R::Unit | R::Option(None)) => true,
        // ron reads `()` as unit but `( /* c */ )` as an empty tuple.
        (Value::Null, R::Seq(items)) => items.is_empty(),
        (_, R::Option(Some(inner))) => same(n, inner),
        (Value::Bool(a), R::Bool(b)) => a == b,
        (Value::Char(a), R::Char(b)) => a == b,
        (Value::Str(a), R::String(b)) => a == b,
        (
            Value::Int {
                negative,
                magnitude,
            },
            R::Number(b),
        ) => {
            let ours = if *negative {
                -(*magnitude as f64)
            } else {
                *magnitude as f64
            };
            ours == num(b)
        }
        (Value::Float(a), R::Number(b)) => {
            *a == num(b) || (a.is_nan() && num(b).is_nan()) || (*a as f32) as f64 == num(b)
        }
        (Value::List(a), R::Seq(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        }
        (Value::Map(a), R::Map(b)) => {
            a.len() == b.len()
                && a.iter().all(|(k, v)| {
                    b.iter().any(|(rk, rv)| {
                        let key = match rk {
                            R::String(s) => s.clone(),
                            R::Number(n) => format!("{}", num(n)),
                            R::Char(c) => c.to_string(),
                            R::Bool(b) => b.to_string(),
                            _ => return false,
                        };
                        (key == *k
                            || key.parse::<f64>().ok() == k.parse::<f64>().ok()
                                && k.parse::<f64>().is_ok())
                            && same(v, rv)
                    })
                })
        }
        _ => false,
    }
}

#[test]
fn oracle_ron() {
    let mut rng = 3u64;
    let mut next = |n: u64| {
        rng = rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) % n
    };
    let scalars = [
        "1",
        "-2",
        "0x1F",
        "0b101",
        "0o17",
        "1_000",
        "1.5",
        "-1e3",
        "inf",
        "-inf",
        "NaN",
        "true",
        "false",
        "'a'",
        "'\\n'",
        "\"s\"",
        "\"\\u{e9}\"",
        "r#\"raw\"#",
        "None",
        "Some(1)",
        "()",
        "Unit",
        "T(1, 2)",
        "S(a: 1)",
        "(a: 1, b: 2)",
        "(1, \"x\")",
        "123456789012345678901234567",
    ];
    let pieces = [
        ",", " ", "//c\n", "/*c*/", ")", "(", ":", "'", "\"", "#", "_", "+", ".", "r",
    ];
    fn generate(s: &mut String, depth: u32, next: &mut dyn FnMut(u64) -> u64, scalars: &[&str]) {
        match if depth == 0 { 0 } else { next(4) } {
            0 => s.push_str(scalars[next(scalars.len() as u64) as usize]),
            1 => {
                s.push('[');
                for i in 0..next(4) {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    generate(s, depth - 1, next, scalars);
                }
                if next(3) == 0 {
                    s.push(',');
                }
                s.push(']');
            }
            2 => {
                s.push_str(["S(", "(", "Variant("][next(3) as usize]);
                for i in 0..next(3) + 1 {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(["a: ", "b: ", "c: "][i as usize % 3]);
                    generate(s, depth - 1, next, scalars);
                }
                s.push(')');
            }
            _ => {
                s.push('{');
                for i in 0..next(3) {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(["\"k\": ", "\"j\": ", "1: "][i as usize % 3]);
                    generate(s, depth - 1, next, scalars);
                }
                s.push('}');
            }
        }
    }
    let (mut accepted, mut rejected) = (0, 0);
    for _ in 0..15_000 {
        let mut s = String::new();
        generate(&mut s, 3, &mut next, &scalars);
        if next(4) == 0 {
            let at = next(s.len() as u64 + 1) as usize;
            if s.is_char_boundary(at) {
                s.insert_str(at, pieces[next(pieces.len() as u64) as usize]);
            }
        }
        let ours = parse_ron(&s);
        let theirs = ron::from_str::<ron::Value>(&s);
        match (&ours, &theirs) {
            (Ok(n), Ok(r)) => {
                accepted += 1;
                assert!(same(n, r), "{s:?}: {n:?} vs {r:?}");
            }
            (Err(e), Ok(_)) => {
                let allowed = e.msg.starts_with("duplicate key")
                    || (e.msg == "ranges aren't supported" && s.contains(".."));
                assert!(allowed, "{s:?}: ron accepts, we say {}", e.msg);
            }
            (Ok(n), Err(e)) => {
                // ron without its integer128 feature rejects integers above u64.
                let allowed = s.contains("123456789012345678901234567");
                assert!(allowed, "{s:?}: we accept {n:?}, ron says {e}");
            }
            (Err(_), Err(_)) => rejected += 1,
        }
    }
    assert!(accepted >= 4_000 && rejected > 0, "accepted {accepted}");
}

#[test]
fn review_fixes() {
    let deep = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
    assert_eq!(v(&deep).unwrap_err(), "nested deeper than 128 levels");
    assert!(v(&format!("{}{}", "[".repeat(100), "]".repeat(100))).is_ok());
    let Value::Named(name, payload) = v("Some(None)").unwrap() else {
        panic!()
    };
    assert_eq!(name, "Some");
    assert!(matches!(&payload.value, Value::List(l) if l.len() == 1 && l[0].value == Value::Null));
    assert!(v("[1,\u{a0}2]").is_err());
    assert_eq!(v("1e400").unwrap_err(), "1e400 is out of range for f64");
    assert_eq!(
        v("1e-400").unwrap_err(),
        "1e-400 is too small for f64; it would be 0"
    );
}
