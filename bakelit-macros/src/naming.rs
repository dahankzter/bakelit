//! serde's `rename_all` styles and did-you-mean suggestions.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Case {
    Lower,
    Upper,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
    Camel,
    Pascal,
}

pub const CASE_NAMES: [(&str, Case); 8] = [
    ("lowercase", Case::Lower),
    ("UPPERCASE", Case::Upper),
    ("snake_case", Case::Snake),
    ("SCREAMING_SNAKE_CASE", Case::ScreamingSnake),
    ("kebab-case", Case::Kebab),
    ("SCREAMING-KEBAB-CASE", Case::ScreamingKebab),
    ("camelCase", Case::Camel),
    ("PascalCase", Case::Pascal),
];

impl Case {
    pub fn from_name(name: &str) -> Option<Case> {
        CASE_NAMES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
    }

    /// A snake_case field name in this style, as serde renames fields.
    pub fn field(self, field: &str) -> String {
        match self {
            Case::Lower | Case::Snake => field.to_string(),
            Case::Upper | Case::ScreamingSnake => field.to_ascii_uppercase(),
            Case::Pascal => {
                let mut pascal = String::new();
                let mut capitalize = true;
                for ch in field.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else if capitalize {
                        pascal.push(ch.to_ascii_uppercase());
                        capitalize = false;
                    } else {
                        pascal.push(ch);
                    }
                }
                pascal
            }
            Case::Camel => lower_first(&Case::Pascal.field(field)),
            Case::Kebab => field.replace('_', "-"),
            Case::ScreamingKebab => Case::ScreamingSnake.field(field).replace('_', "-"),
        }
    }

    /// A PascalCase variant name in this style, as serde renames variants.
    pub fn variant(self, variant: &str) -> String {
        match self {
            Case::Pascal => variant.to_string(),
            Case::Lower => variant.to_ascii_lowercase(),
            Case::Upper => variant.to_ascii_uppercase(),
            Case::Camel => lower_first(variant),
            Case::Snake => {
                let mut snake = String::new();
                for (i, ch) in variant.char_indices() {
                    if i > 0 && ch.is_uppercase() {
                        snake.push('_');
                    }
                    snake.push(ch.to_ascii_lowercase());
                }
                snake
            }
            Case::ScreamingSnake => Case::Snake.variant(variant).to_ascii_uppercase(),
            Case::Kebab => Case::Snake.variant(variant).replace('_', "-"),
            Case::ScreamingKebab => Case::ScreamingSnake.variant(variant).replace('_', "-"),
        }
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// The closest candidate: the same ignoring case and `_`/`-`, or within two edits.
pub fn did_you_mean<'a>(word: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let fold = |s: &str| s.to_ascii_lowercase().replace(['_', '-'], "");
    if let Some(c) = candidates.iter().find(|c| fold(c) == fold(word)) {
        return Some(c);
    }
    candidates
        .iter()
        .map(|c| (distance(word, c), *c))
        .filter(|(d, c)| *d <= 2 && *d < word.chars().count().max(c.chars().count()))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
