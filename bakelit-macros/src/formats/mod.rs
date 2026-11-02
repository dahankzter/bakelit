//! The file formats, each parsing into [`Node`].

use crate::value::{DataError, Node};

#[cfg(feature = "csv")]
pub mod csv;
#[cfg(feature = "env")]
pub mod env;
#[cfg(feature = "ini")]
pub mod ini;
#[cfg(feature = "kdl")]
pub mod kdl;
#[cfg(feature = "frontmatter")]
pub mod markdown;
#[cfg(feature = "properties")]
pub mod properties;

#[cfg(any(
    feature = "json",
    feature = "jsonc",
    feature = "json5",
    feature = "jsonl"
))]
pub mod json;
#[cfg(feature = "ron")]
pub mod ron;
#[cfg(feature = "toml")]
pub mod toml;
#[cfg(feature = "yaml")]
pub mod yaml;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Json,
    Jsonc,
    Json5,
    Jsonl,
    Yaml,
    Toml,
    Ron,
    Csv,
    Tsv,
    Ini,
    Env,
    Properties,
    Kdl,
    Markdown,
}

/// Names accepted after `as`, and file extensions.
const NAMES: [(&str, Format); 17] = [
    ("json", Format::Json),
    ("jsonc", Format::Jsonc),
    ("json5", Format::Json5),
    ("jsonl", Format::Jsonl),
    ("ndjson", Format::Jsonl),
    ("yaml", Format::Yaml),
    ("yml", Format::Yaml),
    ("toml", Format::Toml),
    ("ron", Format::Ron),
    ("csv", Format::Csv),
    ("tsv", Format::Tsv),
    ("ini", Format::Ini),
    ("env", Format::Env),
    ("properties", Format::Properties),
    ("kdl", Format::Kdl),
    ("md", Format::Markdown),
    ("markdown", Format::Markdown),
];

impl Format {
    pub fn names() -> Vec<&'static str> {
        NAMES.iter().map(|(n, _)| *n).collect()
    }

    pub fn from_name(name: &str) -> Option<Format> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
    }

    /// By extension (any case); a file named `.env` is `.env`.
    pub fn from_path(path: &str) -> Option<Format> {
        let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
        if file == ".env" {
            return Some(Format::Env);
        }
        let (stem, ext) = file.rsplit_once('.')?;
        if stem.is_empty() {
            return None;
        }
        Format::from_name(&ext.to_ascii_lowercase())
    }

    /// The extension error messages use.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Markdown => "md",
            f => NAMES
                .iter()
                .find(|(_, g)| *g == f)
                .map(|(n, _)| *n)
                .unwrap(),
        }
    }

    /// The cargo feature that enables this format.
    pub fn feature(self) -> &'static str {
        match self {
            Format::Tsv => "csv",
            Format::Markdown => "frontmatter",
            f => f.extension(),
        }
    }

    pub fn enabled(self) -> bool {
        match self {
            Format::Json => cfg!(feature = "json"),
            Format::Jsonc => cfg!(feature = "jsonc"),
            Format::Json5 => cfg!(feature = "json5"),
            Format::Jsonl => cfg!(feature = "jsonl"),
            Format::Yaml => cfg!(feature = "yaml"),
            Format::Toml => cfg!(feature = "toml"),
            Format::Ron => cfg!(feature = "ron"),
            Format::Csv | Format::Tsv => cfg!(feature = "csv"),
            Format::Ini => cfg!(feature = "ini"),
            Format::Env => cfg!(feature = "env"),
            Format::Properties => cfg!(feature = "properties"),
            Format::Kdl => cfg!(feature = "kdl"),
            Format::Markdown => cfg!(feature = "frontmatter"),
        }
    }
}

/// Parses a file's text, and returns the body of a Markdown file. The format's
/// feature must be enabled.
pub fn parse_document(format: Format, text: &str) -> Result<(Node, Option<&str>), DataError> {
    #[cfg(feature = "frontmatter")]
    if format == Format::Markdown {
        if text.starts_with('\u{feff}') {
            return Err(DataError::at(None, "remove the UTF-8 byte order mark"));
        }
        let (node, body) = markdown::parse_markdown(text)?;
        check_depth(&node)?;
        return Ok((node, Some(body)));
    }
    parse(format, text).map(|node| (node, None))
}

fn check_depth(node: &Node) -> Result<(), DataError> {
    let max = crate::value::MAX_DEPTH;
    match node.depth() > max {
        true => Err(DataError::at(
            node.pos,
            format!("nested deeper than {max} levels"),
        )),
        false => Ok(()),
    }
}

/// Parses a file's text. The format's feature must be enabled.
pub fn parse(format: Format, text: &str) -> Result<Node, DataError> {
    let text = match text.strip_prefix('\u{feff}') {
        Some(rest) if matches!(format, Format::Csv | Format::Tsv) => rest,
        Some(_) => return Err(DataError::at(None, "remove the UTF-8 byte order mark")),
        None => text,
    };
    let node = parse_format(format, text)?;
    check_depth(&node)?;
    Ok(node)
}

fn parse_format(format: Format, text: &str) -> Result<Node, DataError> {
    match format {
        #[cfg(feature = "json")]
        Format::Json => json::parse_json(text),
        #[cfg(feature = "jsonc")]
        Format::Jsonc => json::parse_jsonc(text),
        #[cfg(feature = "json5")]
        Format::Json5 => json::parse_json5(text),
        #[cfg(feature = "jsonl")]
        Format::Jsonl => json::parse_jsonl(text),
        #[cfg(feature = "yaml")]
        Format::Yaml => yaml::parse_yaml(text),
        #[cfg(feature = "toml")]
        Format::Toml => toml::parse_toml(text),
        #[cfg(feature = "ron")]
        Format::Ron => ron::parse_ron(text),
        #[cfg(feature = "csv")]
        Format::Csv => csv::parse_csv(text, b','),
        #[cfg(feature = "csv")]
        Format::Tsv => csv::parse_csv(text, b'\t'),
        #[cfg(feature = "ini")]
        Format::Ini => ini::parse_ini(text),
        #[cfg(feature = "env")]
        Format::Env => env::parse_env(text),
        #[cfg(feature = "properties")]
        Format::Properties => properties::parse_properties(text),
        #[cfg(feature = "kdl")]
        Format::Kdl => kdl::parse_kdl(text),
        #[cfg(feature = "frontmatter")]
        Format::Markdown => markdown::parse_markdown(text).map(|(node, _)| node),
        #[allow(unreachable_patterns)] // With every format enabled.
        _ => unreachable!("{format:?} isn't enabled"),
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod text_tests;
