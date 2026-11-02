use super::*;
use crate::value::{Pos, Value};

fn parse(text: &str) -> Result<Node, String> {
    parse_json(text).map_err(|e| e.msg)
}

/// Whether our node equals serde_json's value (numbers by value, maps in order).
pub(crate) fn same(n: &Node, j: &serde_json::Value) -> bool {
    use serde_json::Value as J;
    match (&n.value, j) {
        (Value::Null, J::Null) => true,
        (Value::Bool(a), J::Bool(b)) => a == b,
        (Value::Str(a), J::String(b)) => a == b,
        (
            Value::Int {
                negative,
                magnitude,
            },
            J::Number(b),
        ) => {
            let ours = if *negative {
                format!("-{magnitude}")
            } else {
                magnitude.to_string()
            };
            // serde_json keeps "-0" as written; we read it as 0.
            ours == b.to_string() || (b.to_string() == "-0" && *magnitude == 0)
        }
        (Value::Float(a), J::Number(b)) => b.to_string().parse::<f64>().is_ok_and(|b| b == *a),
        (Value::List(a), J::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        }
        (Value::Map(a), J::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((ka, va), (kb, vb))| ka == kb && same(va, vb))
        }
        _ => false,
    }
}

#[test]
fn values_and_positions() {
    let n = parse("{\n  \"a\": [1, -2.5, true, null],\n  \"b\": \"x\\u00e9\"\n}").unwrap();
    let Value::Map(m) = &n.value else { panic!() };
    assert_eq!(m[0].0, "a");
    let Value::List(l) = &m[0].1.value else {
        panic!()
    };
    assert_eq!(
        l[0].value,
        Value::Int {
            negative: false,
            magnitude: 1
        }
    );
    assert_eq!(l[0].pos, Some(Pos { line: 2, col: 9 }));
    assert_eq!(l[1].value, Value::Float(-2.5));
    assert_eq!(m[1].1.value, Value::Str("xé".into()));
    assert_eq!(m[1].1.pos, Some(Pos { line: 3, col: 8 }));
    assert_eq!(
        parse("340282366920938463463374607431768211455")
            .unwrap()
            .value,
        Value::Int {
            negative: false,
            magnitude: u128::MAX
        }
    );
    assert_eq!(
        parse("-0").unwrap().value,
        Value::Int {
            negative: false,
            magnitude: 0
        }
    );
    assert_eq!(parse("1E2").unwrap().value, Value::Float(100.0));
}

#[test]
fn strictness() {
    for bad in [
        "{\"a\": 01}",
        "[1,]",
        "\"\\ud800\"",
        "\"tab\there\"",
        "1.",
        "-",
        "[1 2]",
        "{\"a\": 1} x",
        "// c\n1",
        "'a'",
        "{a: 1}",
        "0x10",
        "+1",
        ".5",
        "NaN",
        "",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} accepted");
        assert!(
            serde_json::from_str::<serde_json::Value>(bad).is_err(),
            "{bad:?}: serde accepts"
        );
    }
    assert_eq!(
        parse("{\"a\": 1, \"a\": 2}").unwrap_err(),
        r#"duplicate key "a""#
    );
    assert_eq!(parse("1e400").unwrap_err(), "1e400 is out of range for f64");
    assert_eq!(
        parse("340282366920938463463374607431768211456").unwrap_err(),
        "340282366920938463463374607431768211456 is too large for any integer type"
    );
}

fn gen_json(rng: &mut u64, depth: u32, out: &mut String) {
    let pick = |rng: &mut u64, n: u64| {
        *rng = rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) % n
    };
    let scalars = [
        "0",
        "-0",
        "1",
        "-17",
        "1.5",
        "2e3",
        "-1E-2",
        "123456789012345678901234",
        "true",
        "false",
        "null",
        "\"a\"",
        "\"\"",
        "\"\\n\\u00e9\\\"\"",
        "\"é\"",
        "\"\\ud83d\\ude00\"",
    ];
    match if depth == 0 { 0 } else { pick(rng, 3) } {
        0 => out.push_str(scalars[pick(rng, scalars.len() as u64) as usize]),
        1 => {
            out.push('[');
            for i in 0..pick(rng, 4) {
                if i > 0 {
                    out.push(',');
                }
                gen_json(rng, depth - 1, out);
            }
            out.push(']');
        }
        _ => {
            out.push('{');
            for i in 0..pick(rng, 4) {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(["\"a\"", "\"b\"", "\"c d\""][pick(rng, 3) as usize]);
                out.push(':');
                gen_json(rng, depth - 1, out);
            }
            out.push('}');
        }
    }
    if pick(rng, 40) == 0 {
        let noise = [
            ",", " ", "\t", "\u{1}", "0", "\"", "}", "]", "/", "e", "-", "\\",
        ];
        let at = pick(rng, out.len() as u64 + 1) as usize;
        if out.is_char_boundary(at) {
            out.insert_str(at, noise[pick(rng, noise.len() as u64) as usize]);
        }
    }
}

#[test]
fn oracle_serde_json() {
    let mut rng = 7u64;
    let (mut accepted, mut compared) = (0, 0);
    for _ in 0..20_000 {
        let mut s = String::new();
        gen_json(&mut rng, 3, &mut s);
        let theirs = serde_json::from_str::<serde_json::Value>(&s);
        let ours = parse_json(&s);
        compared += 1;
        match (&ours, &theirs) {
            (Ok(n), Ok(j)) => {
                accepted += 1;
                assert!(same(n, j), "{s:?}: {n:?} vs {j}");
            }
            (Err(e), Ok(_)) => {
                let allowed = e.msg.starts_with("duplicate key")
                    || (e.msg.ends_with("is too large for any integer type") && s.contains("123456789012345678901234"))
                    // serde_json's arbitrary_precision keeps any exponent as text.
                    || (e.msg.ends_with("is out of range for f64") && s.contains(['e', 'E']));
                assert!(allowed, "{s:?}: serde accepts, we say {}", e.msg);
            }
            (Ok(n), Err(e)) => panic!("{s:?}: we accept {n:?}, serde says {e}"),
            (Err(_), Err(_)) => {}
        }
    }
    assert!(
        compared == 20_000 && accepted >= 5_000,
        "accepted {accepted}"
    );
}

#[test]
fn review_fixes() {
    for ws in ["\u{a0}", "\u{b}", "\u{c}", "\u{2003}"] {
        assert_eq!(
            parse(&format!("[1,{ws}2]")).unwrap_err(),
            "only spaces, tabs and line breaks may separate JSON values",
            "{ws:?}"
        );
    }
    assert_eq!(
        parse_json("[\"\u{a0}\"]").unwrap().value,
        Value::List(vec![Node::new(
            Value::Str("\u{a0}".into()),
            Some(Pos { line: 1, col: 2 })
        )])
    );
    assert_eq!(
        parse("1e-400").unwrap_err(),
        "1e-400 is too small for f64; it would be 0"
    );
    assert_eq!(parse("0e-400").unwrap().value, Value::Float(0.0));
}
