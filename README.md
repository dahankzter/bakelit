# bakelit

Data files baked into typed Rust statics at compile time.

```rust
bakelit::bake! {
    pub static CONFIG: Config = "config/app.toml";
    pub static COUNTRIES: &[Country] = "data/countries.csv";

    #[derive(Debug)]
    #[bake(rename_all = "kebab-case")]
    pub struct Config {
        pub name: &'static str,
        pub port: u16,
        pub servers: &'static [Server],
        pub log_level: Level,
    }

    #[derive(Debug)]
    pub struct Server { pub host: &'static str, pub max_conns: Option<u32> }

    #[derive(Debug)]
    #[bake(rename_all = "lowercase")]
    pub enum Level { Debug, Info, Warn }

    #[derive(Debug)]
    pub struct Country { pub code: &'static str, pub name: &'static str, pub population: u64 }
}
```

`bake!` reads each file while your crate compiles. It checks the data against the types and
writes the values as Rust literals. At run time nothing is parsed or allocated, and the
result works in `no_std`. A mistake in the data is a compile error that names the file, the
line and the place in the data:

```text
error: config/app.toml:7:8: $.servers[1].port: expected u16, found 70000
error: config/app.toml:3:8: $.prot: unknown key; did you mean "port"?
error: data/flags.yaml:1:10: $.enabled: expected bool, found "yes"; write true or false
```

Editing a data file rebuilds the crate.

## Formats

The format comes from the file's extension. `"file.conf" as ini` names it instead. Each
format has its own cargo feature; `json`, `toml`, `yaml` and `csv` are on by default.

| format | extensions | feature | notes |
|---|---|---|---|
| JSON | `.json` | `json` | RFC 8259; duplicate keys are errors |
| JSONC | `.jsonc` | `jsonc` | JSON with comments and trailing commas |
| JSON5 | `.json5` | `json5` | |
| JSON Lines | `.jsonl`, `.ndjson` | `jsonl` | a list, one value per line |
| YAML | `.yaml`, `.yml` | `yaml` | YAML 1.2 core schema, one document |
| TOML | `.toml` | `toml` | datetimes become strings, in TOML's standard form |
| RON | `.ron` | `ron` | struct and variant names are checked |
| CSV, TSV | `.csv`, `.tsv` | `csv` | a header row; a list of rows |
| INI | `.ini` | `ini` | top-level keys and `[sections]`; see below |
| `.env` | `.env`, `*.env` | `env` | never reads the environment |
| Java properties | `.properties` | `properties` | read as UTF-8 |
| KDL | `.kdl` | `kdl` | KDL 2 |
| Markdown | `.md`, `.markdown` | `frontmatter` | YAML (`---`) or TOML (`+++`) front matter |

Format notes:

- **YAML** reads scalars by the YAML 1.2 core schema, so `yes`, `on` and `1_000` are
  strings. Integers are exact up to `u128`; larger ones are errors, not floats. Anchors
  and aliases work. Merge keys (`<<`), multiple documents, custom tags, and keys that repeat
  (also `a` and `"a"`) are errors.
- **Numbers** that overflow a float (`1e400`) or underflow to zero (`1e-400`) are errors in
  every format. Only a spelled-out infinity (`inf`, `.inf`, `#inf`, `Infinity`) is infinite.
- **JSON and JSONC** take only spaces, tabs and line breaks as whitespace, as RFC 8259 says.
- **RON** keeps `Some(...)`, so `Some(None)` fills an `Option<Option<T>>`.
- **CSV and TSV** strip a UTF-8 byte order mark. A repeated header, a row with the wrong
  number of fields, and text after a closing quote (`"x"y`) are errors. In a one-column file
  a blank line is an error (write `""` for an empty field). TSV has no quoting.
- **INI** follows `rust-ini`: `key = value` lines, `;` and `#` comment lines, and double
  quotes around a value are removed. Backslashes stay as written (`C:\new` is not a
  newline). A `;` after a value is part of the value. A line without `=` and repeated keys
  or sections are errors.
- **`.env`** accepts `KEY=value`, `export KEY=value`, single quotes (literal) and double quotes
  (with `\n`, `\t`, `\"`, `\\` and `\$` escapes). `#` after whitespace starts a comment, so
  `KEY= # note` is empty. `$NAME` and `${NAME}` are errors: bakelit never substitutes values
  from the build machine's environment. Single quotes write a literal `$`.
- **KDL** maps a node to a key:
  - one argument gives that value; several give a list;
  - properties and children give a mapping;
  - children all named `-` give a list (also at the top level);
  - a node with nothing gives null;
  - siblings that share a name give a list of their values;
  - a node that mixes arguments with properties or children, and type annotations, are errors.
- **Markdown**: a field marked `#[bake(body)]` on the item's own struct gets the text after
  the front matter.
- Data nested deeper than 128 levels is an error in every format.

## Types

| Rust type | data |
|---|---|
| `bool` | `true` or `false` |
| `u8` ... `u128`, `i8` ... `i128`, `usize`, `isize` | integers, range-checked |
| `f32`, `f64` | numbers; see the rules below |
| `char` | a one-character string, or a RON char |
| `&'static str` | strings |
| `Option<T>` | `null`, a missing key, or an empty CSV field gives `None` |
| `&'static [T]`, `[T; N]` | lists (an array needs exactly `N` elements) |
| `(A, B, ...)` | lists of that length |
| `&'static [(K, V)]` | mappings, in file order |
| structs | mappings, or RON's `Name(field: ...)` |
| tuple and unit structs | lists, and null |
| enums | a unit variant from its name. A variant with data from `{"Variant": data}` or RON's `Variant(...)` |

Define the structs and enums inside `bake!`, next to the items that use them. `bake!` emits
them unchanged, except that it removes the `#[bake]` attributes. `String`, `Vec` and
`HashMap` can't be built in a constant; use `&'static str`, `&'static [T]` and
`&'static [(K, V)]`.

### Attributes

- `#[bake(rename = "...")]` on a field or variant: the key in the data.
- `#[bake(rename_all = "...")]` on a struct or enum, with serde's styles:
  - `lowercase`, `UPPERCASE`;
  - `snake_case`, `SCREAMING_SNAKE_CASE`;
  - `kebab-case`, `SCREAMING-KEBAB-CASE`;
  - `camelCase`, `PascalCase`.
- `#[bake(allow_unknown)]` on a struct or enum: ignore keys that no field reads.
- `#[bake(body)]` on a `&'static str` field: the body of a Markdown file.

### Conversion rules

bakelit is strict on purpose. A value that could be read two ways is an error, not a guess:

- A key the struct doesn't have is an error, with a did-you-mean when one is close.
- A missing key is an error unless the field is an `Option`.
- An integer must fit its type exactly.
- A float never fills an integer.
- An integer fills a float only if the float holds it exactly.
- An `f32` must hold the number exactly or print the same as it. `0.1` fits, `0.123456789`
  doesn't.
- Text formats (CSV, TSV, INI, `.env`, `.properties`) hold text, and it's parsed into the
  field's type:
  - numbers are decimal, with no `+` and no leading zeros;
  - booleans are `true` and `false` only;
  - an empty field is `None` in an `Option`, `""` in a `&str`, and an error in anything else.

## Inference

If an item's type isn't defined in the invocation, bakelit derives it from the data:

```rust
bakelit::bake! {
    pub static DATA: Data = "data/servers.json";
}
// DATA.servers[0].host, DATA.owner.name, ...
```

The rules for generated types:

- A mapping becomes a `pub struct` with fields in file order. Each field is the key in
  snake_case (`maxConns` and `max-conns` both become `max_conns`). A nested mapping becomes
  a struct named `{Parent}{Key}`, such as `DataOwner`.
- Integers become `i64`, or `u64` above `i64::MAX`. Floats become `f64`, and a list that
  mixes integers and floats becomes `f64`.
- Strings become `&'static str`.
- Lists become `&'static [T]`. Mappings in a list merge, and a key missing from some of them
  becomes an `Option`.
- `null` makes an `Option` of the type the other values have.
- CSV and TSV columns become `i64`, `f64` or `bool` when every cell agrees, and `&'static str`
  otherwise. A column with `007` stays text. Empty cells make the column an `Option`.
- INI, `.env` and `.properties` values are always `&'static str`.
- Generated types derive `Debug`, `Clone`, `Copy` and `PartialEq`.

Anything that would be a guess is an error that asks you to write the types:

- an empty list, or a list of mixed kinds;
- a value that is only ever `null`;
- a key that can't be a field name, or two keys that become the same field.

## Notes

- Paths are relative to the crate's `Cargo.toml` (`CARGO_MANIFEST_DIR`).
- MSRV 1.88. The `bakelit` crate is `no_std` and has no `unsafe` code.

## License

MIT or Apache-2.0, at your option.
