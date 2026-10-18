use std::{collections::HashSet, sync::Mutex};

use super::IndexerError;
use crate::{
    graph::{EdgeKind, NodeKey},
    index::{self, Index, SectionVector},
};

/// How many nearest sections in other notes each section keeps.
const NEAREST: usize = 10;

/// Drops edges whose note or section is gone, then gives each section that has a vector but no semantic edges its
/// nearest sections in other notes, weighted by cosine similarity. Sections keep their neighbours while their text is
/// unchanged, so a changed note only adds edges for its new sections. The index is not held while comparing vectors.
pub(super) fn update_graph_edges(index: &Mutex<Index>) -> Result<(), IndexerError> {
    let (sections, linked) = {
        let index = index::lock(index);
        drop_gone_ends(&index)?;
        let linked: HashSet<NodeKey> = index
            .connection()
            .prepare("SELECT DISTINCT src FROM edges WHERE kind = ?1")?
            .query_map([EdgeKind::Semantic], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;

        (index.section_vectors()?, linked)
    };

    let keys: Vec<NodeKey> = sections
        .iter()
        .map(|section| NodeKey::section(section.file_id, &section.text_hash))
        .collect();
    let mut edges = Vec::new();
    for (place, section) in sections.iter().enumerate() {
        if linked.contains(&keys[place]) {
            continue;
        }

        for (neighbour, similarity) in nearest(section, &sections) {
            edges.push((&keys[place], &keys[neighbour], similarity));
        }
    }
    if edges.is_empty() {
        return Ok(());
    }

    let mut index = index::lock(index);
    let tx = index.transaction()?;
    {
        let mut insert = tx.prepare(
            "INSERT OR REPLACE INTO edges (src, dst, kind, weight) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (src, dst, similarity) in edges {
            insert.execute((src, dst, EdgeKind::Semantic, similarity))?;
        }
    }

    Ok(tx.commit()?)
}

/// The places in `sections` of the [`NEAREST`] sections in other notes most similar to `section`.
fn nearest(section: &SectionVector, sections: &[SectionVector]) -> Vec<(usize, f32)> {
    let mut similarities: Vec<(usize, f32)> = sections
        .iter()
        .enumerate()
        .filter(|(_, other)| other.file_id != section.file_id)
        .map(|(place, other)| {
            let similarity = section
                .vector
                .iter()
                .zip(&other.vector)
                .map(|(a, b)| a * b)
                .sum();
            (place, similarity)
        })
        .collect();

    let by_similarity = |a: &(usize, f32), b: &(usize, f32)| b.1.total_cmp(&a.1);
    if similarities.len() > NEAREST {
        similarities.select_nth_unstable_by(NEAREST, by_similarity);
        similarities.truncate(NEAREST);
    }

    similarities
}

/// Deletes the edges that touch a note or section no longer in the index.
fn drop_gone_ends(index: &Index) -> rusqlite::Result<()> {
    let conn = index.connection();
    let mut present: HashSet<NodeKey> = conn
        .prepare("SELECT id FROM files")?
        .query_map([], |row| Ok(NodeKey::Note(row.get(0)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut statement = conn.prepare("SELECT file_id, text_hash FROM chunks")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        present.insert(NodeKey::section(row.get(0)?, row.get_ref(1)?.as_blob()?));
    }

    let ends: Vec<NodeKey> = conn
        .prepare("SELECT src FROM edges UNION SELECT dst FROM edges")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let gone: Vec<String> = ends
        .into_iter()
        .filter(|end| {
            matches!(end, NodeKey::Note(_) | NodeKey::Section { .. }) && !present.contains(end)
        })
        .map(|end| end.to_string())
        .collect();
    if gone.is_empty() {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM edges
         WHERE src IN (SELECT value FROM json_each(?1)) OR dst IN (SELECT value FROM json_each(?1))",
        [serde_json::Value::from(gone).to_string()],
    )?;

    Ok(())
}
