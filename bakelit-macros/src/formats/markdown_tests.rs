use super::*;
use crate::value::{Pos, Value};

#[test]
fn front_matter() {
    let (node, body) =
        parse_markdown("---\ntitle: Hello\ntags: [a]\n---\n# Heading\n\nText.\n").unwrap();
    let Value::Map(m) = &node.value else { panic!() };
    assert_eq!(m[0].0, "title");
    assert_eq!(m[0].1.pos, Some(Pos { line: 2, col: 8 }));
    assert_eq!(body, "# Heading\n\nText.\n");
    let (node, body) = parse_markdown("+++\r\ntitle = \"T\"\r\n+++\r\nbody").unwrap();
    let Value::Map(m) = &node.value else { panic!() };
    assert_eq!(m[0].1.pos, Some(Pos { line: 2, col: 9 }));
    assert_eq!(body, "body");
    let (node, body) = parse_markdown("---\n---\n").unwrap();
    assert_eq!(node.value, Value::Map(vec![]));
    assert_eq!(body, "");
}

#[test]
fn markdown_errors() {
    let e = |t: &str| parse_markdown(t).unwrap_err().msg;
    assert_eq!(
        e("# no front matter\n"),
        "the file has no front matter; start it with --- (YAML) or +++ (TOML)"
    );
    assert_eq!(
        e("---\ntitle: x\n"),
        "the front matter's --- is never closed"
    );
    assert_eq!(e("---\n- a\n---\n"), "the front matter must be a mapping");
    let e = parse_markdown("---\nok: 1\nbad: [\n---\n").unwrap_err();
    assert_eq!(e.pos.map(|p| p.line), Some(4));
}
