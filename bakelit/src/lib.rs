//! Data files baked into typed Rust statics at compile time.
//!
//! [`bake!`] reads JSON, JSONC, JSON5, JSON Lines, YAML, TOML, RON, CSV, TSV, INI, `.env`,
//! Java properties, KDL or Markdown front matter while the crate compiles. It checks the
//! data against Rust types, either the ones you write or ones inferred from the data, and
//! emits `static` and `const` items built from literals. Nothing is parsed at run time,
//! and the crate is `no_std`.
//!
//! See the [README](https://github.com/dahankzter/bakelit#readme) for the format, type,
//! conversion and inference rules.
#![no_std]
#![forbid(unsafe_code)]

/// Declares `static` and `const` items whose values come from data files.
///
/// Each item names a file, relative to the crate's `Cargo.toml`, and optionally its format:
/// `static X: T = "path" [as format];`. The structs and enums the items use are defined in
/// the same invocation:
///
/// ```
/// # #[cfg(feature = "json")] mod demo {
/// bakelit::bake! {
///     pub static APP: App = "tests/data/app.json";
///
///     #[derive(Debug)]
///     #[bake(rename_all = "kebab-case")]
///     pub struct App {
///         pub name: &'static str,
///         pub port: u16,
///         pub servers: &'static [Server],
///         pub log_level: Level,
///         // Keys the struct doesn't read are errors unless the struct
///         // has #[bake(allow_unknown)]; this file has more.
///         pub ratio: f32,
///         pub limits: &'static [(&'static str, u32)],
///         pub shape: Shape,
///     }
///
///     #[derive(Debug)]
///     pub struct Server {
///         pub host: &'static str,
///         #[bake(rename = "max-conns")]
///         pub max_conns: Option<u32>,
///     }
///
///     #[derive(Debug, PartialEq)]
///     #[bake(rename_all = "lowercase")]
///     pub enum Level { Debug, Info }
///
///     #[derive(Debug)]
///     pub enum Shape { Circle { r: f64 }, Dot }
/// }
///
/// # pub fn run() {
/// assert_eq!(APP.port, 8080);
/// assert_eq!(APP.servers[1].max_conns, None);
/// assert_eq!(APP.log_level, Level::Info);
/// # }
/// # }
/// # #[cfg(feature = "json")] demo::run();
/// ```
///
/// If the item's type isn't defined, it is inferred from the data:
///
/// ```
/// # #[cfg(feature = "json")] {
/// bakelit::bake! {
///     static DATA: Data = "tests/data/inferred.json";
/// }
/// assert_eq!(DATA.servers[1].port, 8080);
/// assert_eq!(DATA.owner.tags, &["x", "y"]);
/// # }
/// ```
///
/// A mismatch between the data and the types is a compile error at the path:
///
/// ```compile_fail
/// bakelit::bake! {
///     // tests/data/app.json has "port": 8080.
///     static APP: App = "tests/data/app.json";
///     #[bake(allow_unknown)]
///     struct App { port: u8 }
/// }
/// ```
pub use bakelit_macros::bake;
