use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;

use super::{EdgeKind, ExcludedFolder, Graph, NodeKey, resolve::NotePaths};

/// Semantic edges below this cosine similarity are left out. The stored edges keep every section's nearest whatever
/// their similarity, so this can change without re-indexing. On the fixture vault, sections on unrelated topics reach
/// 0.73, while a note on circuit breakers scored 0.80 against the one on retry backoff. The eval sets the final value.
const SEMANTIC_FLOOR: f32 = 0.75;
/// Joins each section to its note.
const MEMBERSHIP_WEIGHT: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node {
    /// A note, by its file id.
    Note(i64),
    Section(Section),
    Tag,
}

impl Node {
    /// The note the node belongs to, by file id; `None` for a tag.
    pub(crate) fn file_id(self) -> Option<i64> {
        match self {
            Self::Note(file_id) => Some(file_id),
            Self::Section(section) => Some(section.file_id),
            Self::Tag => None,
        }
    }
}

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
    /// What each node of `graph` stands for.
    pub(crate) nodes: Vec<Node>,
    chunk_nodes: HashMap<i64, usize>,
    note_nodes: HashMap<i64, usize>,
    /// Which notes link to which, by file id: links and property links that resolve to another note.
    pub(crate) links: BTreeSet<(i64, i64)>,
}

/// A note's share of a walk: the rank of its note node and sections together, and its best-ranked section.
#[derive(Debug, Default)]
pub(crate) struct NoteRank {
    pub(crate) rank: f32,
    /// The chunk of the best-ranked section and its rank.
    pub(crate) best_section: Option<(i64, f32)>,
}

impl VaultGraph {
    pub(crate) fn load(conn: &Connection) -> rusqlite::Result<Self> {
        Self::load_without(conn, None)
    }

    /// The graph without the notes in `excluded`.
    pub(crate) fn load_without(
        conn: &Connection,
        excluded: Option<&ExcludedFolder>,
    ) -> rusqlite::Result<Self> {
        let mut builder = Builder {
            conn,
            excluded,
            graph: Graph::default(),
            nodes: Vec::new(),
            notes: HashMap::new(),
            paths: NotePaths::default(),
            keyed: HashMap::new(),
            chunk_nodes: HashMap::new(),
            headings: HashMap::new(),
            block_texts: HashMap::new(),
        };
        builder.add_notes()?;
        builder.add_sections()?;
        let links = builder.add_stored_edges()?;
        builder.add_shared_links(&links);

        Ok(Self {
            note_nodes: builder
                .notes
                .iter()
                .map(|(&file_id, note)| (file_id, note.node))
                .collect(),
            graph: builder.graph,
            nodes: builder.nodes,
            chunk_nodes: builder.chunk_nodes,
            links,
        })
    }

    pub(crate) fn chunk_node(&self, chunk_id: i64) -> Option<usize> {
        self.chunk_nodes.get(&chunk_id).copied()
    }

    pub(crate) fn note_node(&self, file_id: i64) -> Option<usize> {
        self.note_nodes.get(&file_id).copied()
    }

    /// Each note's share of `ranks`, a walk over [`Self::graph`], by file id.
    pub(crate) fn note_ranks(&self, ranks: &[f32]) -> BTreeMap<i64, NoteRank> {
        let mut notes: BTreeMap<i64, NoteRank> = BTreeMap::new();
        for (&node, &rank) in self.nodes.iter().zip(ranks) {
            let Some(file_id) = node.file_id() else {
                continue;
            };
            let note = notes.entry(file_id).or_default();
            note.rank += rank;
            if let Node::Section(section) = node
                && note.best_section.is_none_or(|(_, best)| rank > best)
            {
                note.best_section = Some((section.chunk_id, rank));
            }
        }

        notes
    }
}

struct Note {
    source_id: i64,
    path: String,
    node: usize,
}

struct Builder<'a> {
    conn: &'a Connection,
    excluded: Option<&'a ExcludedFolder>,
    graph: Graph,
    nodes: Vec<Node>,
    notes: HashMap<i64, Note>,
    paths: NotePaths,
    /// Section and tag nodes by key.
    keyed: HashMap<NodeKey, usize>,
    chunk_nodes: HashMap<i64, usize>,
    /// Each note's chunks in order, with their heading paths.
    headings: HashMap<i64, Vec<(i64, String)>>,
    /// The chunks in order, with their texts, of the notes a block link names.
    block_texts: HashMap<i64, Vec<(i64, String)>>,
}

impl Builder<'_> {
    fn add_node(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.graph.add_node()
    }

    fn add_notes(&mut self) -> rusqlite::Result<()> {
        let mut statement = self.conn.prepare("SELECT id, source_id, path FROM files")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let (file_id, source_id, path): (i64, i64, String) =
                (row.get(0)?, row.get(1)?, row.get(2)?);
            if self.excluded.is_some_and(|folder| folder.contains(&path)) {
                continue;
            }
            self.paths.insert(source_id, &path, file_id);
            let node = self.add_node(Node::Note(file_id));
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
            let Some(note_node) = self.notes.get(&file_id).map(|note| note.node) else {
                continue;
            };
            let key = NodeKey::section(file_id, row.get_ref(3)?.as_blob()?);

            // Chunks of one note with the same text are one section.
            let node = match self.keyed.get(&key) {
                Some(&node) => node,
                None => {
                    let node = self.add_node(Node::Section(Section { chunk_id, file_id }));
                    self.keyed.insert(key, node);
                    self.graph.add_edge(note_node, node, MEMBERSHIP_WEIGHT);
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
        self.load_block_texts(&rows)?;

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
                    let Some((target_file, node)) = self.resolve(*file_id, target) else {
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
            NodeKey::Section { .. } => self.keyed.get(key).copied(),
            NodeKey::Tag(_) => Some(match self.keyed.get(key) {
                Some(&node) => node,
                None => {
                    let node = self.add_node(Node::Tag);
                    self.keyed.insert(key.clone(), node);
                    node
                }
            }),
            NodeKey::Link(_) => None,
        }
    }

    /// The note a link target written in note `from` names, and the node it lands on: the section of its heading or
    /// block when it names one that exists, else the note.
    fn resolve(&self, from: i64, target: &str) -> Option<(i64, usize)> {
        let (path, subpath) = target.split_once('#').unwrap_or((target, ""));
        let file_id = self.resolve_note(from, path)?;

        let chunk_id = match subpath.strip_prefix('^') {
            Some(block) => self.block_chunk(file_id, block),
            None => heading_chunk(
                self.headings.get(&file_id).map_or(&[], Vec::as_slice),
                subpath,
            ),
        };
        let node = chunk_id
            .and_then(|chunk_id| self.chunk_nodes.get(&chunk_id))
            .copied()
            .unwrap_or(self.notes[&file_id].node);

        Some((file_id, node))
    }

    /// The note a link path written in note `from` names.
    fn resolve_note(&self, from: i64, path: &str) -> Option<i64> {
        let note = &self.notes[&from];

        self.paths.resolve(note.source_id, &note.path, path)
    }

    /// Loads the chunk texts of every note that a block link among `edges` names, in one query.
    fn load_block_texts(
        &mut self,
        edges: &[(NodeKey, NodeKey, EdgeKind, f32)],
    ) -> rusqlite::Result<()> {
        let file_ids: BTreeSet<i64> = edges
            .iter()
            .filter_map(|(src, dst, _, _)| match (src, dst) {
                (NodeKey::Note(file_id), NodeKey::Link(target))
                    if self.notes.contains_key(file_id) =>
                {
                    let (path, block) = target.split_once("#^")?;
                    (!block.is_empty()).then(|| self.resolve_note(*file_id, path))?
                }
                _ => None,
            })
            .collect();
        if file_ids.is_empty() {
            return Ok(());
        }

        let file_ids = serde_json::to_string(&file_ids).expect("a list of ids serialises");
        let mut statement = self.conn.prepare(
            "SELECT c.file_id, c.id, t.text FROM chunks c JOIN chunks_fts t ON t.rowid = c.id
             WHERE c.file_id IN (SELECT value FROM json_each(?1)) ORDER BY c.file_id, c.ordinal",
        )?;
        let mut rows = statement.query([file_ids])?;
        while let Some(row) = rows.next()? {
            self.block_texts
                .entry(row.get(0)?)
                .or_default()
                .push((row.get(1)?, row.get(2)?));
        }

        Ok(())
    }

    /// The first chunk of a note with a line ending in the block id `^block`.
    fn block_chunk(&self, file_id: i64, block: &str) -> Option<i64> {
        let marker = format!("^{block}");
        let has_block = |text: &str| {
            text.lines().any(|line| {
                let line = line.trim_end();
                line == marker
                    || line
                        .strip_suffix(&marker)
                        .is_some_and(|before| before.ends_with(char::is_whitespace))
            })
        };

        self.block_texts
            .get(&file_id)?
            .iter()
            .find(|(_, text)| has_block(text))
            .map(|&(chunk_id, _)| chunk_id)
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
