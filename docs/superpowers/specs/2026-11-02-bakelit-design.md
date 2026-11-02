# bakelit: data files baked into typed Rust statics — design

Date: 2026-11-02. Status: approved by the user's instruction to plan, run and
implement in one go ("Go with the cheaper formats", MessagePack/CBOR skipped
for now). Decisions marked *(my call)* were made by the executor without a
check-in, as instructed.

## Intent

Projects ship data with their binaries: country lists, config defaults, test
fixtures, game data, lookup tables. Today that is `include_str!` plus a parse
at startup (serde into a `LazyLock`), which costs time at run time and turns a
typo in the data into a run-time failure.

bakelit reads the file at compile time, checks it against Rust types, and
emits a `static` or `const` built from literals. Nothing is parsed at run
time, a mistake in the data is a compile error that names the file, line and
path, and the result is `no_std`.

Sibling of `constlit` (compile-time checked literals) and `pickmap` in the
user's macroz workspace; same values: strict by default, errors that say what
to write instead, performance first.

## Goals

- One macro, `bakelit::bake!`, that declares one or more `static`/`const`
  items whose values come from data files.
- Types either written by the user inside the macro (checked strictly) or
  inferred from the data (by strict, documented rules).
- Formats (each behind its own cargo feature): JSON, JSONC, JSON5, JSON Lines,
  YAML, TOML, RON, CSV, TSV, INI, `.env`, Java `.properties`, KDL, and Markdown
  with YAML or TOML front matter.
- Compile errors at the path literal, formatted
  `<file>:<line>:<col>: <path>: <problem>` (line and column where the parser
  provides them, the data path always), with a suggestion when one exists.
- The file is tracked, so editing it rebuilds the crate.
- Runtime crate `no_std`, no `unsafe`, MSRV 1.88, edition 2024.

## Non-goals

- MessagePack and CBOR (deferred by the user), Protobuf, CUE, Dhall, Jsonnet,
  Pkl, HCL, XML, Parquet, Arrow.
- Run-time loading or reloading. Owned types (`String`, `Vec`, `HashMap`):
  everything is `'static` borrowed data built in constants.
- Types defined outside the macro invocation (the macro cannot see them).
  *(my call; a derive-based extension can come later)*

## Anti-goals

- Silent meaning: a value that could be read two ways is an error, not a
  guess. Examples: YAML 1.1 booleans (`yes`, `on`) for a `bool`, `"007"`
  inferred as a number, an unknown key in the data (likely a typo), a float
  into an integer, an integer that doesn't fit, lossy float conversions.
- Reading the build machine's environment: `.env` values are never
  substituted from the process environment.

## Macro surface

```rust
bakelit::bake! {
    pub static CONFIG: Config = "data/app.toml";
    pub const COUNTRIES: &[Country] = "data/countries.csv";
    static EXTRA: Extra = "data/extra.conf" as ini;   // format override

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Config {
        pub name: &'static str,
        pub port: u16,
        pub servers: &'static [Server],
        #[bake(rename = "log-level")]
        pub log_level: Level,
    }

    #[derive(Debug, Clone, Copy)]
    #[bake(rename_all = "kebab-case")]
    pub struct Server { pub host: &'static str, pub max_conns: Option<u32> }

    #[derive(Debug, Clone, Copy, PartialEq)]
    #[bake(rename_all = "lowercase")]
    pub enum Level { Debug, Info, Warn }

    #[derive(Debug)]
    pub struct Country { pub code: &'static str, pub name: &'static str, pub population: u64 }
}
```

- Items: `[vis] static|const NAME: Type = "path" [as format];`, in any order
  with type definitions (`struct` with named fields, tuple `struct`, unit
  `struct`, `enum`). Type definitions are emitted unchanged except that
  `#[bake(...)]` attributes are stripped.
- Paths are relative to `CARGO_MANIFEST_DIR` *(my call: works the same in
  rust-analyzer and in workspaces; `include_str!`'s file-relative rule needs
  `Span::local_file`, which can be missing)*. The format comes from the
  extension unless `as <format>` names it.
- If the item's type mentions a name that is not defined in the invocation and
  is not a supported built-in, the type is inferred from the data (see
  Inference) and the macro generates the definitions. A type that is partly
  defined is an error.

### Types

`bool`, `char`, `u8`…`u128`, `i8`…`i128`, `usize`, `isize`, `f32`, `f64`,
`&'static str` (also `&str` in `const`/`static` position), `Option<T>`,
`&'static [T]`, `[T; N]`, tuples up to 12 elements, `()`, and the structs and
enums defined in the invocation. A mapping can also become
`&'static [(K, V)]` (order as in the file).

Enums: unit variants from strings; tuple, struct and newtype variants
externally tagged (`{"Circle": {"r": 1}}`, YAML/TOML/JSON alike) and in RON's
own syntax (`Circle(r: 1)`).

Attributes (`#[bake(...)]`):
- on structs and enums: `rename_all = "lowercase" | "UPPERCASE" | "snake_case"
  | "SCREAMING_SNAKE_CASE" | "kebab-case" | "SCREAMING-KEBAB-CASE" |
  "camelCase" | "PascalCase"`, `allow_unknown` (ignore extra keys);
- on fields and variants: `rename = "..."`;
- on a `&'static str` field of a front-matter struct: `body` (the Markdown
  after the front matter).

### Conversion rules (strict)

- Missing key: error unless the field is `Option` (then `None`). Unknown key:
  error with did-you-mean, unless `allow_unknown`.
- Integers: exact, range-checked. Floats into integers: error. Integers into
  floats: only if exactly representable. Floats into `f32`: only if the value
  round-trips (else error suggesting `f64`). NaN/infinity only where the
  format has them.
- `char`: a one-character string.
- `null`/`~`/`None` only into `Option`.
- `[T; N]`: exactly N elements.
- String-typed formats (CSV, TSV, INI, `.env`, `.properties`, KDL string
  values are not string-typed): text is parsed into the target type: integers
  in decimal without a sign `+` or leading zeros, `true`/`false` only, floats
  in Rust syntax; an empty field into `Option` is `None`, into `&str` is `""`,
  into anything else an error.

### Inference

When the item's type is not defined, the macro derives one:
- object → `pub struct` named after the item's type (`Config`); nested
  objects → `pub struct` named `{Parent}{Key}` in PascalCase
  (`ConfigServers`); each generated type derives `Debug, Clone, Copy,
  PartialEq`;
- keys must convert to valid snake_case identifiers (else an error asking for
  explicit types with `rename`); fields keep file order;
- integers → `i64` (`u64` above `i64::MAX`, larger is an error); floats →
  `f64`; integers mixed with floats in one array/column → `f64`; strings →
  `&'static str`; booleans → `bool`; `null` → `Option` of the other values'
  type (all-null is an error);
- arrays → `&'static [T]` of the unified element type; objects in an array
  unify by key (a key missing in some → `Option`); empty arrays,
  heterogeneous arrays and arrays of arrays of mixed shape are errors asking
  for an explicit type;
- CSV/TSV columns: all cells integers → `i64`, all numbers → `f64`, all
  `true`/`false` → `bool`, else `&'static str`; empty cells → `Option`;
  numbers with leading zeros keep the column a string;
- INI, `.env`, `.properties`: every value is `&'static str` (no guessing).

## Formats

| format | feature | parser | notes |
|---|---|---|---|
| JSON (`.json`) | `json` | `jsonc-parser`, strict options, plus RFC 8259 checks | duplicate keys are an error; `serde_json` is the test oracle |
| JSONC (`.jsonc`) | `jsonc` | `jsonc-parser` | comments and trailing commas only |
| JSON5 (`.json5`) | `json5` | `jsonc-parser`, JSON5 options | the `json5` crate is the test oracle |
| JSON Lines (`.jsonl`, `.ndjson`) | `jsonl` | as JSON, per line | root is an array; blank lines are errors except a final newline |
| YAML (`.yaml`, `.yml`) | `yaml` | `saphyr`, scalars resolved by the YAML 1.2 core schema from their raw text | one document; anchors and aliases fine; large integers exact |
| TOML (`.toml`) | `toml` | `toml` | datetimes into `&'static str` as written |
| RON (`.ron`) | `ron` | own parser | keeps struct and variant names; the `ron` crate is the test oracle |
| CSV (`.csv`), TSV (`.tsv`) | `csv` | `csv` | header row required; root is `&[Row]` |
| INI (`.ini`) | `ini` | `rust-ini` | top-level keys and sections (as nested structs); duplicate keys are errors |
| `.env` | `env` | own parser | `KEY=value`, `export`, quotes, `#` comments; `${...}` outside single quotes is an error; never reads the environment |
| `.properties` | `properties` | `java-properties` | |
| KDL (`.kdl`) | `kdl` | `kdl`, KDL 2.0 | mapping below |
| Markdown (`.md`) | `frontmatter` (+ `yaml` or `toml`) | `---` YAML or `+++` TOML front matter | `#[bake(body)]` field receives the rest |

KDL mapping: the document and a node's children are mappings from node name
to value; a node with one argument and nothing else is that value; with
several arguments (and nothing else), a list; with properties and/or children,
a mapping (a key in both is an error); a name repeated among siblings is a
list of those values; a node mixing arguments with properties or children is
an error.

Default features: `json`, `toml`, `yaml`, `csv` *(my call)*. A file whose
format's feature is off is an error naming the feature.

## Crates

- `bakelit` (runtime facade, `no_std`): re-exports `bake!`, documents it;
  features forward to the macros crate.
- `bakelit-macros` (proc macro): `syn` (full) for the item and type
  definitions *(my call: unlike constlit this parses Rust items, which is what
  `syn` is for)*, `proc-macro2`, `quote`, and the format parsers behind
  features.
- Expansion: the user's types; each item as
  `static NAME: T = <literal expression>;`; one
  `const _: &[u8] = include_bytes!("<absolute path>");` per file for rebuild
  tracking. All paths in the generated code are absolute (`::core::...`).

## Testing

- Unit tests of the value model, conversion and inference.
- Per-format oracle tests where a reference exists: `serde_json` (JSON
  acceptance and values), the `json5` crate (JSON5), the `ron` crate (RON
  acceptance and structure modulo names); for the others, table tests from
  each format's specification examples.
- A no-panic sweep over random input for every parser.
- `trybuild` UI tests for the error messages; integration tests in the facade
  crate with files under `tests/data/`.
- Checks: tests in every feature combination that matters (`--all-features`,
  default, `--no-default-features`), clippy `-D warnings`, `+1.88`, a
  `thumbv7m-none-eabi` build of the facade, docs, fmt.
