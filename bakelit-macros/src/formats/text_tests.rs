//! CSV, TSV, INI, .env and .properties: every value is untyped text.
#![allow(dead_code, unused_imports)]

use crate::value::{DataError, Node, Pos, Value};

fn t(s: &str) -> Value {
    Value::Text(s.into())
}
/// A map's keys and text values.
fn pairs(n: &Node) -> Vec<(String, String)> {
    let Value::Map(m) = &n.value else {
        panic!("{n:?}")
    };
    m.iter()
        .map(|(k, v)| match &v.value {
            Value::Text(s) => (k.clone(), s.clone()),
            other => (k.clone(), format!("{other:?}")),
        })
        .collect()
}
fn kv(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
fn msg(r: Result<Node, DataError>) -> (Option<usize>, String) {
    let e = r.unwrap_err();
    (e.pos.map(|p| p.line), e.msg)
}

#[cfg(feature = "csv")]
mod csv_tests {
    use super::*;
    use crate::formats::csv::parse_csv;

    #[test]
    fn rows() {
        let text = "\u{feff}name,note\r\n\"Smith, J\",\"two\nlines\"\r\nDoe,\r\n\r\n";
        let text = text.strip_prefix('\u{feff}').unwrap();
        let n = parse_csv(text, b',').unwrap();
        let Value::List(rows) = &n.value else {
            panic!()
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(
            pairs(&rows[0]),
            kv(&[("name", "Smith, J"), ("note", "two\nlines")])
        );
        assert_eq!(pairs(&rows[1]), kv(&[("name", "Doe"), ("note", "")]));
        assert_eq!(rows[1].pos.map(|p| p.line), Some(4));
        let Value::Map(m) = &rows[1].value else {
            panic!()
        };
        assert_eq!(m[0].1.pos, Some(Pos { line: 4, col: 1 }));
        let tsv = parse_csv("a\tb\n1\t\"x\"\n", b'\t').unwrap();
        let Value::List(rows) = &tsv.value else {
            panic!()
        };
        assert_eq!(pairs(&rows[0]), kv(&[("a", "1"), ("b", "\"x\"")]));
    }

    #[test]
    fn csv_errors() {
        assert_eq!(
            msg(parse_csv("a,b\n1,2\n3\n", b',')),
            (Some(3), "this row has 1 field, the header has 2".into())
        );
        assert_eq!(
            msg(parse_csv("a,b,a\n1,2,3\n", b',')),
            (Some(1), r#"the header has "a" twice"#.into())
        );
        assert_eq!(
            msg(parse_csv("", b',')),
            (None, "the file has no header row".into())
        );
        assert_eq!(parse_csv("a,b\n", b',').unwrap().value, Value::List(vec![]));
    }
}

#[cfg(feature = "ini")]
mod ini_tests {
    use super::*;
    use crate::formats::ini::parse_ini;

    #[test]
    fn sections() {
        let n = parse_ini("top = 1\n; comment\n[server]\nhost = a ; not a comment\nport=80\n\n[db]\nname = \"quoted\"\n").unwrap();
        let Value::Map(m) = &n.value else { panic!() };
        assert_eq!(
            m.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["top", "server", "db"]
        );
        assert_eq!(m[0].1.value, t("1"));
        assert_eq!(
            pairs(&m[1].1),
            kv(&[("host", "a ; not a comment"), ("port", "80")])
        );
        assert_eq!(pairs(&m[2].1), kv(&[("name", "quoted")]));
    }

    #[test]
    fn ini_errors() {
        assert_eq!(
            msg(parse_ini("[s]\nk = 1\nk = 2\n")).1,
            r#"duplicate key "k" in [s]"#
        );
        assert_eq!(
            msg(parse_ini("[s]\na = 1\n[s]\nb = 2\n")).1,
            "the section [s] appears twice"
        );
        assert_eq!(
            msg(parse_ini("s = 1\n[s]\nb = 2\n")).1,
            r#"the key "s" and the section [s] have the same name"#
        );
        assert!(parse_ini("[open\n").is_err());
    }
}

#[cfg(feature = "env")]
mod env_tests {
    use super::*;
    use crate::formats::env::parse_env;

    #[test]
    fn values() {
        let text = "# comment\nA=1\nexport B=two words  # trailing\n\nC='lit $HOME \\n'\nD=\"esc \\n \\\" \\\\ \\$\"\nE=\"multi\nline\"\nF=\nG = spaced\nH=a#b\n";
        let n = parse_env(text).unwrap();
        assert_eq!(
            pairs(&n),
            kv(&[
                ("A", "1"),
                ("B", "two words"),
                ("C", "lit $HOME \\n"),
                ("D", "esc \n \" \\ $"),
                ("E", "multi\nline"),
                ("F", ""),
                ("G", "spaced"),
                ("H", "a#b"),
            ])
        );
        let Value::Map(m) = &n.value else { panic!() };
        assert_eq!(m[1].1.pos, Some(Pos { line: 3, col: 10 }));
        assert_eq!(m[4].1.pos, Some(Pos { line: 7, col: 3 }));
        assert_eq!(m[6].1.pos, Some(Pos { line: 10, col: 5 }));
    }

    #[test]
    fn env_errors() {
        let sub = "variable substitution isn't supported; use single quotes for a literal $";
        assert_eq!(msg(parse_env("A=${HOME}\n")), (Some(1), sub.into()));
        assert_eq!(msg(parse_env("A=$HOME\n")), (Some(1), sub.into()));
        assert_eq!(msg(parse_env("A=\"x $Y\"\n")), (Some(1), sub.into()));
        assert_eq!(
            msg(parse_env("A=1\nA=2\n")),
            (Some(2), r#"duplicate key "A""#.into())
        );
        assert_eq!(
            msg(parse_env("1A=x\n")),
            (
                Some(1),
                "a key is letters, digits and _, starting with a letter or _, like API_KEY".into()
            )
        );
        assert_eq!(
            msg(parse_env("A\n")),
            (Some(1), "expected KEY=value".into())
        );
        assert_eq!(
            msg(parse_env("A=\"open\n")),
            (Some(1), "a quoted value that never ends".into())
        );
        assert_eq!(
            msg(parse_env("A='x' y\n")),
            (Some(1), "unexpected text after the closing quote".into())
        );
        assert_eq!(
            parse_env("A=price $5\n").map(|n| pairs(&n)).unwrap(),
            kv(&[("A", "price $5")])
        );
    }

    #[test]
    fn env_never_panics() {
        let alphabet: Vec<char> = "AB=_ \n'\"\\$#{}xé\r\t1".chars().collect();
        let mut state = 9u64;
        for _ in 0..30_000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let len = (state >> 59) as usize;
            let s: String = (0..len)
                .map(|i| alphabet[((state >> (i % 50)) as usize + i * 7) % alphabet.len()])
                .collect();
            let _ = parse_env(&s);
        }
    }
}

#[cfg(feature = "properties")]
mod properties_tests {
    use super::*;
    use crate::formats::properties::parse_properties;

    #[test]
    fn values() {
        let n = parse_properties("# c\n! c\na=1\nb : two\nc three\nlong = one \\\n    two\nuni = caf\\u00e9\nkey\\:colon = x\n").unwrap();
        assert_eq!(
            pairs(&n),
            kv(&[
                ("a", "1"),
                ("b", "two"),
                ("c", "three"),
                ("long", "one two"),
                ("uni", "café"),
                ("key:colon", "x")
            ])
        );
        let Value::Map(m) = &n.value else { panic!() };
        assert_eq!(m[3].1.pos.map(|p| p.line), Some(6));
        assert_eq!(
            msg(parse_properties("a=1\na=2\n")),
            (Some(2), r#"duplicate key "a""#.into())
        );
    }
}

#[cfg(feature = "ini")]
#[test]
fn ini_review_fixes() {
    use crate::formats::ini::parse_ini;
    let n = parse_ini("path=C:\\new\\table\n").unwrap();
    assert_eq!(pairs(&n), kv(&[("path", "C:\\new\\table")]));
    assert_eq!(
        msg(parse_ini("a=1\njunk\nb=2\n")).1,
        r#"the line "junk" has no "=""#
    );
    assert_eq!(
        msg(parse_ini("a=1\n  continued\nb=2\n")).1,
        r#"the line "continued" has no "=""#
    );
}

#[cfg(feature = "env")]
#[test]
fn env_review_fixes() {
    use crate::formats::env::parse_env;
    assert_eq!(
        pairs(&parse_env("A= # set me\nB=#x\nC=  \n").unwrap()),
        kv(&[("A", ""), ("B", "#x"), ("C", "")])
    );
}

#[cfg(feature = "csv")]
#[test]
fn csv_review_fixes() {
    use crate::formats::csv::parse_csv;
    assert_eq!(
        msg(parse_csv("a\nx\n\ny\n", b',')),
        (
            Some(3),
            "a blank line; in a one-column file write an empty field as \"\"".into()
        )
    );
    assert_eq!(
        msg(parse_csv("a,b\n\"x\"y,1\n", b',')),
        (
            Some(2),
            "text after a closing quote; quote the whole field".into()
        )
    );
    let n = parse_csv("a\nx\n\"\"\ny\n", b',').unwrap();
    assert!(matches!(&n.value, Value::List(rows) if rows.len() == 3));
    let n = parse_csv("a,b\n1,2\n\n3,4\n", b',').unwrap();
    assert!(matches!(&n.value, Value::List(rows) if rows.len() == 2));
}
