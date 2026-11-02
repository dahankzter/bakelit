//! Random text through every enabled parser, inference and conversion: nothing panics.

use crate::convert::{Ctx, convert};
use crate::formats::{self, Format};
use crate::infer::{TextRule, infer};
use crate::ty::Ty;

const ALPHABET: &str = "{}[]():,=\"'#-*~!&|>\n\t \\/abc019.eE+_é$@`<%;r";
const FORMATS: [Format; 14] = [
    Format::Json,
    Format::Jsonc,
    Format::Json5,
    Format::Jsonl,
    Format::Yaml,
    Format::Toml,
    Format::Ron,
    Format::Csv,
    Format::Tsv,
    Format::Ini,
    Format::Env,
    Format::Properties,
    Format::Kdl,
    Format::Markdown,
];

#[test]
fn parsers_never_panic() {
    let alphabet: Vec<char> = ALPHABET.chars().collect();
    let mut state = 21u64;
    let mut next = |n: u64| {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) % n
    };
    for format in FORMATS.into_iter().filter(|f| f.enabled()) {
        let (mut parsed, mut baked) = (0, 0);
        for _ in 0..30_000 {
            let len = next(24);
            let mut text: String = (0..len)
                .map(|_| alphabet[next(alphabet.len() as u64) as usize])
                .collect();
            if format == Format::Markdown && next(2) == 0 {
                text = format!("---\n{text}\n---\nbody");
            }
            let Ok((node, body)) = formats::parse_document(format, &text) else {
                continue;
            };
            parsed += 1;
            for root in ["Root", "&[Root]"] {
                let ty = Ty::from_syn(&syn::parse_str(root).unwrap()).unwrap();
                let rule = if matches!(format, Format::Csv | Format::Tsv) {
                    TextRule::Guess
                } else {
                    TextRule::Str
                };
                if let Ok(found) = infer(&node, &ty, &Default::default(), rule, body.is_some()) {
                    let mut ctx = Ctx::new(&found.defs);
                    ctx.body = body;
                    convert(&node, &ty, &mut ctx).unwrap_or_else(|e| {
                        panic!("{format:?} {text:?}: inferred types don't fit: {}", e.msg)
                    });
                    baked += 1;
                }
            }
        }
        // Front matter needs the yaml or toml feature too.
        let can_parse =
            format != Format::Markdown || cfg!(feature = "yaml") || cfg!(feature = "toml");
        assert!(parsed > 0 || !can_parse, "{format:?}: nothing parsed");
        let _ = baked;
    }
}

/// Deeply nested input, on a thread with rustc's 8 MB stack: an error, not a crash.
#[test]
fn deep_nesting_is_an_error() {
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(|| {
            let n = 3000;
            let cases: [(Format, String); 10] = [
                (Format::Json, format!("{}{}", "[".repeat(n), "]".repeat(n))),
                (Format::Jsonc, format!("{}{}", "[".repeat(n), "]".repeat(n))),
                (Format::Json5, format!("{}{}", "[".repeat(n), "]".repeat(n))),
                (Format::Jsonl, format!("{}{}", "[".repeat(n), "]".repeat(n))),
                (Format::Yaml, format!("{}{}", "[".repeat(n), "]".repeat(n))),
                (
                    Format::Toml,
                    format!("a = {}{}", "[".repeat(n), "]".repeat(n)),
                ),
                (Format::Ron, format!("{}{}", "(".repeat(n), ")".repeat(n))),
                (Format::Kdl, format!("{}{}", "n {".repeat(n), "}".repeat(n))),
                (
                    Format::Markdown,
                    format!("---\n{}{}\n---\n", "[".repeat(n), "]".repeat(n)),
                ),
                (
                    Format::Json,
                    format!("{}1{}", "[".repeat(129), "]".repeat(129)),
                ),
            ];
            for (format, text) in cases.iter().filter(|(f, _)| f.enabled()) {
                if *format == Format::Markdown && !cfg!(feature = "yaml") {
                    continue;
                }
                assert!(
                    formats::parse_document(*format, text).is_err(),
                    "{format:?} accepted depth {n}"
                );
            }
            if Format::Json.enabled() {
                let ok = format!("{}1{}", "[".repeat(127), "]".repeat(127));
                assert!(formats::parse(Format::Json, &ok).is_ok());
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
