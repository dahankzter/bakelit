// The helpers serve whichever format features are enabled.
#![allow(dead_code)]

use super::*;
use std::path::PathBuf;

/// A fresh directory under the target dir holding `files`.
fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/bakelit-unit")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    for (f, text) in files {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    d
}

fn run(base: &std::path::Path, src: &str) -> String {
    expand(src.parse().unwrap(), base).to_string()
}

/// The message of the first compile_error! in an expansion.
fn error(expansion: &str) -> String {
    let start = expansion
        .find("compile_error !")
        .unwrap_or_else(|| panic!("no error in {expansion}"));
    let rest = &expansion[start..];
    let lit: syn::LitStr =
        syn::parse_str(&rest[rest.find('"').unwrap()..=rest.find("\")").unwrap()]).unwrap();
    lit.value()
}

#[cfg(feature = "json")]
#[test]
fn expands_items_and_tracks_files() {
    let d = dir(
        "expand",
        &[("a.json", "5"), ("sub dir/größe.json", "[1, 2]")],
    );
    let out = run(
        &d,
        r#"pub static A: u8 = "a.json"; const B: &[u16] = "sub dir/größe.json";"#,
    );
    assert!(out.contains("pub static A : u8 = 5u8 ;"), "{out}");
    assert!(
        out.contains("const B : & [u16] = & [1u16 , 2u16] ;"),
        "{out}"
    );
    let abs = d.join("sub dir/größe.json").canonicalize().unwrap();
    assert!(
        out.contains(&format!("include_bytes ! ({:?})", abs.to_str().unwrap())),
        "{out}"
    );
}

#[cfg(feature = "json")]
#[test]
fn file_and_format_errors() {
    let d = dir(
        "errors",
        &[
            ("a.conf", "x"),
            ("bad.json", "{\n  \"port\": 70000\n}"),
            ("bom.json", "\u{feff}1"),
        ],
    );
    let missing = d.join("nope.json");
    assert_eq!(
        error(&run(&d, r#"static A: u8 = "nope.json";"#)),
        format!(
            "can't read {}: No such file or directory (os error 2)",
            missing.display()
        )
    );
    assert_eq!(
        error(&run(&d, r#"static A: u8 = "a.conf";"#)),
        r#"can't tell the format of "a.conf" from its extension; add `as <format>`, like `"a.conf" as ini`"#
    );
    assert_eq!(
        error(&run(&d, r#"static A: u8 = "a.conf" as jsn;"#)),
        "unknown format `jsn`; did you mean `json`?"
    );
    assert_eq!(
        error(&run(
            &d,
            r#"struct C { port: u16 } static A: C = "bad.json";"#
        )),
        "bad.json:2:11: $.port: expected u16, found 70000"
    );
    assert_eq!(
        error(&run(&d, r#"static A: u8 = "bom.json";"#)),
        "bom.json: remove the UTF-8 byte order mark"
    );
    assert_eq!(
        error(&run(
            &d,
            r#"struct Known; static P: (Known, Unknown) = "nope.json";"#
        )),
        "`Known` is defined here but `Unknown` isn't; define every type, or none to infer them"
    );
    assert_eq!(
        error(&run(&d, r#"static P: (A, B) = "nope.json";"#)),
        "inference needs one type name in the item's type, found A and B"
    );
    // Other items still expand next to an error.
    let out = run(&d, r#"struct C { port: u16 } static A: C = "bad.json";"#);
    assert!(out.contains("struct C"), "{out}");
}

#[test]
fn feature_names() {
    for (f, feature) in [
        (Format::Json, "json"),
        (Format::Kdl, "kdl"),
        (Format::Tsv, "csv"),
        (Format::Markdown, "frontmatter"),
    ] {
        assert_eq!(f.feature(), feature);
    }
    assert_eq!(Format::from_path("x/.env"), Some(Format::Env));
    assert_eq!(Format::from_path("prod.env"), Some(Format::Env));
    assert_eq!(Format::from_path("a.YML"), Some(Format::Yaml));
    assert_eq!(Format::from_path("a.ndjson"), Some(Format::Jsonl));
    assert_eq!(Format::from_path("README"), None);
}

#[cfg(not(feature = "kdl"))]
#[test]
fn disabled_feature() {
    let d = dir("feature", &[("a.kdl", "a 1")]);
    assert_eq!(
        error(&run(&d, r#"static A: u8 = "a.kdl";"#)),
        "reading .kdl files needs bakelit's `kdl` feature"
    );
}

#[cfg(all(feature = "frontmatter", feature = "yaml", feature = "json"))]
#[test]
fn markdown_body() {
    let d = dir(
        "markdown",
        &[
            ("a.json", "{}"),
            ("p.md", "---\nbody: x\n---\ntext"),
            ("q.md", "---\nt: 1\n---\ntext"),
        ],
    );
    assert_eq!(
        error(&run(
            &d,
            r#"struct S { #[bake(body)] b: &'static str } static A: S = "a.json";"#
        )),
        "a.json:1:1: $: #[bake(body)] needs a Markdown file"
    );
    assert_eq!(
        error(&run(&d, r#"static P: P = "p.md";"#)),
        r#"p.md: $: the front matter has a key "body", which the Markdown body would use; write the types yourself"#
    );
    let out = run(&d, r#"static Q: Q = "q.md";"#);
    assert!(out.contains(r#"body : "text""#), "{out}");
}

#[cfg(all(feature = "json", feature = "frontmatter", feature = "yaml"))]
#[test]
fn review_fixes() {
    let d = dir(
        "review",
        &[
            ("c.json", r#"{"servers": {"x": 1}}"#),
            ("bom.md", "\u{feff}---\nt: 1\n---\n"),
            ("b.md", "---\nbody: x\n---\ntext"),
            ("n.md", "---\ninner: {t: 1}\n---\ntext"),
        ],
    );
    assert_eq!(
        error(&run(
            &d,
            r#"static C: Config = "c.json"; struct ConfigServers { y: u8 }"#
        )),
        "the inferred type name ConfigServers is already defined in this invocation; rename that type"
    );
    assert_eq!(
        error(&run(&d, r#"static M: M = "bom.md";"#)),
        "bom.md: remove the UTF-8 byte order mark"
    );
    assert_eq!(
        error(&run(
            &d,
            r#"struct P { #[bake(body)] body: &'static str } static P0: P = "b.md";"#
        )),
        "b.md:2:7: $.body: unknown key; the #[bake(body)] field has this name and holds the Markdown body, so rename the field"
    );
    assert_eq!(
        error(&run(
            &d,
            r#"struct P { inner: I } struct I { t: u8, #[bake(body)] b: &'static str } static P0: P = "n.md";"#
        )),
        "n.md:2:8: $.inner: #[bake(body)] works only on the item's own struct, not a nested one"
    );
    assert!(
        run(&d, r#"enum E {} static A: E = "c.json";"#)
            .contains("an enum needs at least one variant")
    );
}
