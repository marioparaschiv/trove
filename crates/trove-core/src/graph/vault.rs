use std::collections::{BTreeMap, BTreeSet, HashMap, hash_map::Entry};

use rusqlite::Connection;

use super::{EdgeKind, Graph, NodeKey, resolve::NotePaths};

/// Semantic edges below this cosine similarity are left out. The stored edges keep every section's nearest whatever
/// their similarity, so this can change without re-indexing. On the fixture vault, sections on unrelated topics reach
/// 0.73, while a note on circuit breakers scored 0.80 against the one on retry backoff. The eval sets the final value.
const SEMANTIC_FLOOR: f32 = 0.75;
/// Joins each section to its note.
const MEMBERSHIP_WEIGHT: f32 = 1.0;

/// A section node: the chunk it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) chunk_id: i64,
    pub(crate) file_id: i64,
}

/// The graph of the whole index: notes, sections and tags, joined by the stored edges (with link targets resolved),
/// each section to its note, and co-citation and coupling between notes.
pub(crate) struct VaultGraph {
    pub(crate) graph: Graph,
    chunk_nodes: HashMap<i64, usize>,
    sections: HashMap<usize, Section>,
}

impl VaultGraph {
    pub(crate) fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let mut builder = Builder {
            conn,
            graph: Graph::default(),
            notes: HashMap::new(),
            paths: NotePaths::default(),
            nodes: HashMap::new(),
            chunk_nodes: HashMap::new(),
            sections: HashMap::new(),
            headings: HashMap::new(),
        };
        builder.add_notes()?;
        builder.add_sections()?;
        let citations = builder.add_stored_edges()?;
        builder.add_shared_links(&citations);

        Ok(Self {
            graph: builder.graph,
            chunk_nodes: builder.chunk_nodes,
            sections: builder.sections,
        })
    }

    pub(crate) fn chunk_node(&self, chunk_id: i64) -> Option<usize> {
        self.chunk_nodes.get(&chunk_id).copied()
    }

    /// The chunk a node stands for, when it is a section.
    pub(crate) fn section(&self, node: usize) -> Option<Section> {
        self.sections.get(&node).copied()
    }
}

struct Note {
    source_id: i64,
    path: String,
    node: usize,
}

struct Builder<'a> {
    conn: &'a Connection,
    graph: Graph,
    notes: HashMap<i64, Note>,
    paths: NotePaths,
    /// Section and tag nodes.
    nodes: HashMap<NodeKey, usize>,
    chunk_nodes: HashMap<i64, usize>,
    sections: HashMap<usize, Section>,
    /// Each note's chunks in order, with their heading paths.
    headings: HashMap<i64, Vec<(i64, String)>>,
}

impl Builder<'_> {
    fn add_notes(&mut self) -> rusqlite::Result<()> {
        let mut statement = self
            .conn
            .prepare("SELECT id, source_id, path FROM files")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let (file_id, source_id, path): (i64, i64, String) =
                (row.get(0)?, row.get(1)?, row.get(2)?);
            self.paths.insert(source_id, &path, file_id);
            let node = self.graph.add_node();
            self.notes.insert(
                file_id,
                Note {
                    source_id,
                    path,
                    node,
                },
            );
        }

        Ok(())
    }

    fn add_sections(&mut self) -> rusqlite::Result<()> {
        let mut statement = self.conn.prepare(
            "SELECT id, file_id, heading_path, text_hash FROM chunks ORDER BY file_id, ordinal",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let (chunk_id, file_id, heading_path): (i64, i64, String) =
                (row.get(0)?, row.get(1)?, row.get(2)?);
            let key = NodeKey::section(file_id, row.get_ref(3)?.as_blob()?);

            // Chunks of one note with the same text are one section.
            let node = match self.nodes.entry(key) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let node = self.graph.add_node();
                    entry.insert(node);
                    self.graph
                        .add_edge(self.notes[&file_id].node, node, MEMBERSHIP_WEIGHT);
                    self.sections.insert(node, Section { chunk_id, file_id });
                    node
                }
            };
            self.chunk_nodes.insert(chunk_id, node);
            self.headings
                .entry(file_id)
                .or_default()
                .push((chunk_id, heading_path));
        }

        Ok(())
    }

    /// Adds the stored edges whose ends still exist, and returns which notes link to which.
    fn add_stored_edges(&mut self) -> rusqlite::Result<BTreeSet<(i64, i64)>> {
        let rows: Vec<(NodeKey, NodeKey, EdgeKind, f32)> = self
            .conn
            .prepare("SELECT src, dst, kind, weight FROM edges")?
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut edges: HashMap<(usize, usize, EdgeKind), f32> = HashMap::new();
        let mut citations = BTreeSet::new();
        for (src, dst, kind, weight) in rows {
            if kind == EdgeKind::Semantic && weight < SEMANTIC_FLOOR {
                continue;
            }
            let Some(from) = self.node(&src) else {
                continue;
            };
            let to = match (&src, &dst) {
                (NodeKey::Note(file_id), NodeKey::Link(target)) => {
                    let Some((target_file, node)) = self.resolve(*file_id, target)? else {
                        continue;
                    };
                    if target_file != *file_id {
                        citations.insert((*file_id, target_file));
                    }
                    node
                }
                _ => match self.node(&dst) {
                    Some(node) => node,
                    None => continue,
                },
            };
            if from == to {
                continue;
            }

            // An edge stored from both ends (two sections each among the other's nearest) counts once.
            let weight_slot = edges
                .entry((from.min(to), from.max(to), kind))
                .or_insert(weight);
            *weight_slot = weight_slot.max(weight);
        }

        for ((a, b, _), weight) in edges {
            self.graph.add_edge(a, b, weight);
        }

        Ok(citations)
    }

    /// The node for a stored key, adding tags as they come; `None` for a note or section that is gone.
    fn node(&mut self, key: &NodeKey) -> Option<usize> {
        match key {
            NodeKey::Note(file_id) => self.notes.get(file_id).map(|note| note.node),
            NodeKey::Section { .. } => self.nodes.get(key).copied(),
            NodeKey::Tag(_) => Some(*self.nodes.entry(key.clone()).or_insert_with(|| {
                self.graph.add_node()
            })),
            NodeKey::Link(_) => None,
        }
    }

    /// The note a link target written in note `from` names, and the node it lands on: the section of its heading or
    /// block when it names one that exists, else the note.
    fn resolve(&self, from: i64, target: &str) -> rusqlite::Result<Option<(i64, usize)>> {
        let note = &self.notes[&from];
        let (path, subpath) = target.split_once('#').unwrap_or((target, ""));
        let Some(file_id) = self.paths.resolve(note.source_id, &note.path, path) else {
            return Ok(None);
        };

        let chunk_id = match subpath.strip_prefix('^') {
            Some(block) => self.block_chunk(file_id, block)?,
            None => heading_chunk(
                self.headings.get(&file_id).map_or(&[], Vec::as_slice),
                subpath,
            ),
        };
        let node = chunk_id
            .and_then(|chunk_id| self.chunk_nodes.get(&chunk_id))
            .copied()
            .unwrap_or(self.notes[&file_id].node);

        Ok(Some((file_id, node)))
    }

    /// The first chunk of a note with a line ending in the block id `^block`.
    fn block_chunk(&self, file_id: i64, block: &str) -> rusqlite::Result<Option<i64>> {
        let marker = format!("^{block}");
        let mut statement = self.conn.prepare_cached(
            "SELECT c.id, t.text FROM chunks c JOIN chunks_fts t ON t.rowid = c.id
             WHERE c.file_id = ?1 ORDER BY c.ordinal",
        )?;
        let mut rows = statement.query([file_id])?;
        while let Some(row) = rows.next()? {
            let text: String = row.get(1)?;
            let has_block = text.lines().any(|line| {
                let line = line.trim_end();
                line == marker
                    || line
                        .strip_suffix(&marker)
                        .is_some_and(|before| before.ends_with(char::is_whitespace))
            });
            if has_block {
                return Ok(Some(row.get(0)?));
            }
        }

        Ok(None)
    }

    /// Co-citation (two notes linked from the same notes) and coupling (two notes linking to the same notes).
    fn add_shared_links(&mut self, citations: &BTreeSet<(i64, i64)>) {
        let mut cited_by_note: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        let mut citing_by_note: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        for &(citing, cited) in citations {
            cited_by_note.entry(citing).or_default().insert(cited);
            citing_by_note.entry(cited).or_default().insert(citing);
        }

        for (a, b, weight) in shared_members(&cited_by_note)
            .into_iter()
            .chain(shared_members(&citing_by_note))
        {
            self.graph
                .add_edge(self.notes[&a].node, self.notes[&b].node, weight);
        }
    }
}

/// The first chunk under a heading named by `subpath` (`Heading`, or `Outer#Inner` where the last part counts):
/// one whose own heading it is, else one inside it.
fn heading_chunk(chunks: &[(i64, String)], subpath: &str) -> Option<i64> {
    let wanted = subpath.rsplit('#').next()?.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }

    let is_wanted = |heading: &str| heading.trim().to_lowercase() == wanted;
    chunks
        .iter()
        .find(|(_, heading_path)| heading_path.rsplit('\n').next().is_some_and(is_wanted))
        .or_else(|| {
            chunks
                .iter()
                .find(|(_, heading_path)| heading_path.split('\n').any(is_wanted))
        })
        .map(|&(chunk_id, _)| chunk_id)
}

/// Pairs of notes that appear together in `groups`, weighted by how many groups they share over the geometric mean
/// of how many groups each is in.
fn shared_members(groups: &BTreeMap<i64, BTreeSet<i64>>) -> Vec<(i64, i64, f32)> {
    let mut memberships: HashMap<i64, u32> = HashMap::new();
    let mut shared: BTreeMap<(i64, i64), u32> = BTreeMap::new();
    for members in groups.values() {
        for (place, &a) in members.iter().enumerate() {
            *memberships.entry(a).or_default() += 1;
            for &b in members.iter().skip(place + 1) {
                *shared.entry((a, b)).or_default() += 1;
            }
        }
    }

    shared
        .into_iter()
        .map(|((a, b), count)| {
            let mean = ((memberships[&a] * memberships[&b]) as f32).sqrt();
            (a, b, count as f32 / mean)
        })
        .collect()
}
