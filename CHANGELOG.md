# Changelog

## 0.1.0

First release.

- `bake!` declares `static` and `const` items whose values come from data files. The data is
  checked at compile time, written as literals, and needs no parsing at run time (`no_std`).
- Formats, one feature each:
  - JSON, JSONC, JSON5 and JSON Lines;
  - YAML (1.2 core schema) and TOML;
  - RON, with struct and variant names;
  - CSV and TSV, INI, `.env` (no environment access, no substitution) and Java properties;
  - KDL 2;
  - Markdown with YAML or TOML front matter and its body.
- Types:
  - integers, floats, `bool`, `char` and `&'static str`;
  - `Option`, slices, arrays and tuples, and `&[(K, V)]` mappings;
  - structs and enums defined in the invocation, with serde-style `rename` and `rename_all`,
    `allow_unknown` and `body`.
- Inference of struct types from the data when none are written.
- Errors name the file, line, column and data path, with suggestions.
