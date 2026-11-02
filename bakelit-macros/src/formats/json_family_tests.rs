// Helpers serve whichever of the three formats are enabled.
#![allow(dead_code, unused_imports)]

use super::*;
use crate::value::{Pos, Value};

fn v(r: Result<Node, DataError>) -> Result<Value, String> {
    r.map(|n| n.value).map_err(|e| e.msg)
}
fn int(n: i128) -> Value {
    Value::Int {
        negative: n < 0,
        magnitude: n.unsigned_abs(),
    }
}

#[cfg(feature = "jsonc")]
#[test]
fn jsonc() {
    let n = parse_jsonc("// settings\n{\n  /* port */ \"port\": 80,\n  \"tags\": [1, 2,],\n}\n")
        .unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(m[0].1.value, int(80));
    assert_eq!(m[0].1.pos, Some(Pos { line: 3, col: 22 }));
    assert!(parse_jsonc("{'a': 1}").is_err());
    assert!(parse_jsonc("{a: 1}").is_err());
    assert!(parse_jsonc("[0x10]").is_err());
    assert!(parse_jsonc("[\"\t\"]").is_err());
    assert_eq!(
        v(parse_jsonc("{\"a\": 1, \"a\": 2}")).unwrap_err(),
        r#"duplicate key "a""#
    );
}

#[cfg(feature = "json5")]
#[test]
fn json5_values() {
    assert_eq!(v(parse_json5("0x1F")).unwrap(), int(31));
    assert_eq!(v(parse_json5("-0x10")).unwrap(), int(-16));
    assert_eq!(v(parse_json5("+1")).unwrap(), int(1));
    assert_eq!(v(parse_json5(".5")).unwrap(), Value::Float(0.5));
    assert_eq!(v(parse_json5("5.")).unwrap(), Value::Float(5.0));
    assert_eq!(
        v(parse_json5("Infinity")).unwrap(),
        Value::Float(f64::INFINITY)
    );
    assert_eq!(
        v(parse_json5("-Infinity")).unwrap(),
        Value::Float(f64::NEG_INFINITY)
    );
    assert!(matches!(v(parse_json5("NaN")).unwrap(), Value::Float(f) if f.is_nan()));
    assert_eq!(
        v(parse_json5("'it\\'s'")).unwrap(),
        Value::Str("it's".into())
    );
    assert_eq!(
        v(parse_json5("\"a\\\nb\"")).unwrap(),
        Value::Str("ab".into())
    );
    assert_eq!(
        v(parse_json5("\"\\x41\\v\"")).unwrap(),
        Value::Str("A\u{b}".into())
    );
    assert_eq!(
        v(parse_json5("\"tab\there\"")).unwrap(),
        Value::Str("tab\there".into())
    );
    let Value::Map(m) = v(parse_json5("{unquoted: 1, $dollar_1: 2, trailing: [1,],}")).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        m.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["unquoted", "$dollar_1", "trailing"]
    );
    assert!(parse_json5("{a: 1, a: 2}").is_err());
}

#[cfg(feature = "jsonl")]
#[test]
fn jsonl() {
    let n = parse_jsonl("{\"a\": 1}\r\n{\"a\":\n2}\n").unwrap_err();
    assert_eq!(n.pos, Some(Pos { line: 2, col: 6 }));
    let n = parse_jsonl("{\"a\": 1}\r\n  {\"a\": 2}\n").unwrap();
    let Value::List(rows) = &n.value else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].pos, Some(Pos { line: 2, col: 3 }));
    let Value::Map(m) = &rows[1].value else {
        panic!()
    };
    assert_eq!(m[0].1.pos, Some(Pos { line: 2, col: 9 }));
    assert_eq!(
        parse_jsonl("1\n2").unwrap().value,
        Value::List(vec![
            Node::new(int(1), Some(Pos { line: 1, col: 1 })),
            Node::new(int(2), Some(Pos { line: 2, col: 1 })),
        ])
    );
    let e = parse_jsonl("1\n\n2\n").unwrap_err();
    assert_eq!(
        (e.pos, e.msg.as_str()),
        (
            Some(Pos { line: 2, col: 1 }),
            "a blank line; JSON Lines has one value on every line"
        )
    );
    assert_eq!(parse_jsonl("").unwrap().value, Value::List(vec![]));
}

/// Ours against the `json5` crate: acceptance and values.
#[cfg(feature = "json5")]
#[test]
fn oracle_json5() {
    fn same(n: &Node, j: &serde_json::Value) -> bool {
        use serde_json::Value as J;
        match (&n.value, j) {
            // serde_json::Value has no infinity or NaN; the json5 crate gives null.
            (Value::Float(a), J::Null) => !a.is_finite(),
            (
                Value::Int {
                    negative,
                    magnitude,
                },
                J::Number(b),
            ) => {
                let ours = if *negative {
                    -(*magnitude as f64)
                } else {
                    *magnitude as f64
                };
                b.to_string().parse::<f64>().is_ok_and(|b| b == ours)
            }
            (Value::Float(a), J::Number(b)) => b
                .to_string()
                .parse::<f64>()
                .is_ok_and(|b| b == *a || (b.is_nan() && a.is_nan())),
            (Value::List(a), J::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
            }
            (Value::Map(a), J::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((ka, va), (kb, vb))| ka == kb && same(va, vb))
            }
            (Value::Null, J::Null) => true,
            (Value::Bool(a), J::Bool(b)) => a == b,
            (Value::Str(a), J::String(b)) => a == b,
            _ => false,
        }
    }
    let mut rng = 5u64;
    let mut next = |n: u64| {
        rng = rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) % n
    };
    let scalars = [
        "0",
        "-0",
        "1",
        "+1",
        "-17",
        "1.5",
        ".5",
        "5.",
        "2e3",
        "0x1F",
        "-0xa",
        "Infinity",
        "-Infinity",
        "NaN",
        "true",
        "null",
        "'a'",
        "\"b\"",
        "'it\\'s'",
        "\"\\x41\"",
        "\"a\\\nb\"",
        "\"\\u00e9\"",
        "'\t'",
        "123456789012345678901234",
    ];
    let keys = ["a", "\"b\"", "'c'", "$d", "_e1"];
    let pieces = [
        ",", " ", "\n", "/*c*/", "// c\n", "]", "}", ":", "'", "\\", "0x", ".",
    ];
    let (mut accepted, mut agreed) = (0, 0);
    for _ in 0..15_000 {
        let mut s = String::new();
        fn generate(
            s: &mut String,
            depth: u32,
            next: &mut dyn FnMut(u64) -> u64,
            scalars: &[&str],
            keys: &[&str],
        ) {
            match if depth == 0 { 0 } else { next(3) } {
                0 => s.push_str(scalars[next(scalars.len() as u64) as usize]),
                1 => {
                    s.push('[');
                    for i in 0..next(4) {
                        if i > 0 {
                            s.push(',');
                        }
                        generate(s, depth - 1, next, scalars, keys);
                    }
                    if next(3) == 0 {
                        s.push(',');
                    }
                    s.push(']');
                }
                _ => {
                    s.push('{');
                    for i in 0..next(4) {
                        if i > 0 {
                            s.push(',');
                        }
                        s.push_str(keys[next(keys.len() as u64) as usize]);
                        s.push(':');
                        generate(s, depth - 1, next, scalars, keys);
                    }
                    if next(3) == 0 {
                        s.push(',');
                    }
                    s.push('}');
                }
            }
        }
        generate(&mut s, 3, &mut next, &scalars, &keys);
        if next(4) == 0 {
            let at = next(s.len() as u64 + 1) as usize;
            if s.is_char_boundary(at) {
                s.insert_str(at, pieces[next(pieces.len() as u64) as usize]);
            }
        }
        let ours = parse_json5(&s);
        let theirs = json5::from_str::<serde_json::Value>(&s);
        match (&ours, &theirs) {
            (Ok(n), Ok(j)) => {
                accepted += 1;
                assert!(same(n, j), "{s:?}: {n:?} vs {j}");
            }
            (Err(e), Ok(_)) => {
                let allowed = e.msg.starts_with("duplicate key")
                    || (e.msg.ends_with("is too large for any integer type")
                        && s.contains("123456789012345678901234"))
                    // jsonc-parser reads a comment right after an unquoted key as part of it.
                    || (e.msg == "unexpected token" && ["a/", "$d/", "_e1/"].iter().any(|k| s.contains(k)));
                assert!(allowed, "{s:?}: json5 accepts, we say {}", e.msg);
            }
            (Ok(n), Err(e)) => panic!("{s:?}: we accept {n:?}, json5 says {e}"),
            (Err(_), Err(_)) => agreed += 1,
        }
    }
    assert!(accepted >= 4_000 && agreed > 0, "accepted {accepted}");
}
