use super::*;
use serde::Serialize;

macro_rules! serde_names {
    ($($style:literal),*) => {{
        let mut out = Vec::new();
        $({
            #[derive(Serialize)]
            #[serde(rename_all = $style)]
            struct F { max_conns2: u8, a_b_c: u8, x: u8 }
            #[derive(Serialize)]
            #[serde(rename_all = $style)]
            #[allow(clippy::upper_case_acronyms)]
            enum V { Http2Server, ABC, X }
            let f = serde_json::to_value(F { max_conns2: 0, a_b_c: 0, x: 0 }).unwrap();
            let fields: Vec<String> = f.as_object().unwrap().keys().cloned().collect();
            let variants: Vec<String> = [V::Http2Server, V::ABC, V::X]
                .iter()
                .map(|v| serde_json::to_value(v).unwrap().as_str().unwrap().to_string())
                .collect();
            out.push(($style, fields, variants));
        })*
        out
    }};
}

#[test]
fn rename_all_matches_serde() {
    let cases = serde_names!(
        "lowercase",
        "UPPERCASE",
        "snake_case",
        "SCREAMING_SNAKE_CASE",
        "kebab-case",
        "SCREAMING-KEBAB-CASE",
        "camelCase",
        "PascalCase"
    );
    for (style, fields, variants) in cases {
        let case = Case::from_name(style).unwrap();
        let ours: Vec<String> = ["max_conns2", "a_b_c", "x"]
            .iter()
            .map(|f| case.field(f))
            .collect();
        assert_eq!(ours, fields, "fields under {style}");
        let ours: Vec<String> = ["Http2Server", "ABC", "X"]
            .iter()
            .map(|v| case.variant(v))
            .collect();
        assert_eq!(ours, variants, "variants under {style}");
    }
    assert!(Case::from_name("Kebab").is_none());
}

#[test]
fn suggestions() {
    let c = ["port", "name", "max-conns"];
    assert_eq!(did_you_mean("prot", &c), Some("port"));
    assert_eq!(did_you_mean("PORT", &c), Some("port"));
    assert_eq!(did_you_mean("max_conns", &c), Some("max-conns"));
    assert_eq!(did_you_mean("zzzzzz", &c), None);
    assert_eq!(did_you_mean("a", &["b"]), None);
}
