use serde_json::{Map, Value};
use thiserror::Error;

/// A note's properties: its frontmatter as a JSON object.
pub type Properties = Map<String, Value>;

#[derive(Debug, Error)]
pub enum FrontmatterError {
    #[error("the frontmatter is not valid YAML")]
    Yaml(#[source] serde_saphyr::Error),
    #[error("the frontmatter is not a list of properties (a YAML mapping)")]
    NotAMapping,
}

/// A leading `---` block, closed by `---` or `...`.
pub(crate) struct Frontmatter<'a> {
    pub(crate) yaml: &'a str,
    /// Byte offset where the body starts.
    pub(crate) end: usize,
}

impl<'a> Frontmatter<'a> {
    pub(crate) fn find(text: &'a str) -> Option<Self> {
        let mut lines = text.split_inclusive('\n');
        let first = lines.next()?;
        if first.trim_end() != "---" {
            return None;
        }

        let mut offset = first.len();
        for line in lines {
            let line_start = offset;
            offset += line.len();
            if matches!(line.trim_end(), "---" | "...") {
                return Some(Self {
                    yaml: &text[first.len()..line_start],
                    end: offset,
                });
            }
        }

        None
    }
}

/// The note's properties; a note without frontmatter has none.
pub fn properties(text: &str) -> Result<Properties, FrontmatterError> {
    let Some(frontmatter) = Frontmatter::find(text) else {
        return Ok(Properties::new());
    };

    match serde_saphyr::from_str(frontmatter.yaml).map_err(FrontmatterError::Yaml)? {
        Value::Object(properties) => Ok(properties),
        Value::Null => Ok(Properties::new()),
        _ => Err(FrontmatterError::NotAMapping),
    }
}
