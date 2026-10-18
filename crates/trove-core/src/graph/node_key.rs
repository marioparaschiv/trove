use std::fmt;

use rusqlite::{
    ToSql,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef},
};
use thiserror::Error;

/// A node as the `edges` table names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum NodeKey {
    /// A note, by its `files` id.
    Note(i64),
    /// A chunk, by its note and its text hash in hex rather than its id: chunk ids change whenever a note is
    /// re-chunked, while an unchanged section keeps its text.
    Section { file_id: i64, text_hash: String },
    /// A tag, lowercased.
    Tag(String),
    /// A link target as written (`Folder/Note#Heading`), resolved when the graph is built because the note it names
    /// may not be indexed yet or may be renamed later.
    Link(String),
}

impl NodeKey {
    pub(crate) fn section(file_id: i64, text_hash: &[u8]) -> Self {
        Self::Section {
            file_id,
            text_hash: hex::encode(text_hash),
        }
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note(file_id) => write!(f, "note:{file_id}"),
            Self::Section { file_id, text_hash } => write!(f, "section:{file_id}:{text_hash}"),
            Self::Tag(tag) => write!(f, "tag:{tag}"),
            Self::Link(target) => write!(f, "link:{target}"),
        }
    }
}

#[derive(Debug, Error)]
#[error("{0:?} is not a graph node key")]
struct InvalidNodeKey(String);

impl FromSql for NodeKey {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        let invalid = || FromSqlError::Other(Box::new(InvalidNodeKey(text.to_owned())));
        let (kind, rest) = text.split_once(':').ok_or_else(invalid)?;

        match kind {
            "note" => rest.parse().map(Self::Note).map_err(|_| invalid()),
            "section" => {
                let (file_id, text_hash) = rest.split_once(':').ok_or_else(invalid)?;
                Ok(Self::Section {
                    file_id: file_id.parse().map_err(|_| invalid())?,
                    text_hash: text_hash.to_owned(),
                })
            }
            "tag" => Ok(Self::Tag(rest.to_owned())),
            "link" => Ok(Self::Link(rest.to_owned())),
            _ => Err(invalid()),
        }
    }
}

impl ToSql for NodeKey {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_string()))
    }
}

/// The kinds of edge stored in the `edges` table. Co-citation and coupling follow from the links, so they are worked
/// out when the graph is built instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EdgeKind {
    /// Wikilinks, embeds and markdown links, from a note to a link target.
    Link,
    /// A link inside a property, from a note to a link target.
    PropertyLink,
    /// From a note to a tag it carries.
    Tag,
    /// From a section to one of its nearest sections in other notes, weighted by cosine similarity.
    Semantic,
}

impl EdgeKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::PropertyLink => "property_link",
            Self::Tag => "tag",
            Self::Semantic => "semantic",
        }
    }
}

#[derive(Debug, Error)]
#[error("{0:?} is not a stored edge kind")]
struct InvalidEdgeKind(String);

impl FromSql for EdgeKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        [Self::Link, Self::PropertyLink, Self::Tag, Self::Semantic]
            .into_iter()
            .find(|kind| kind.name() == text)
            .ok_or_else(|| FromSqlError::Other(Box::new(InvalidEdgeKind(text.to_owned()))))
    }
}

impl ToSql for EdgeKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.name()))
    }
}
