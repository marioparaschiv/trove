use serde::Serialize;

/// How much each signal counts in ranking. Search ranks with [`WEIGHTS`]; `trove eval --fit` searches with others to
/// fit them. Only the ratios within a query's score rank: scaling every term of a score alike changes no order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Weights {
    /// How much a note's meaning score ([`Weights::next_chunk_share`]) counts.
    pub meaning: f32,
    /// How much a note's word coverage counts, each word weighted by how few notes have it
    /// ([`Coverage::weighted`](super::Coverage::weighted)).
    pub words: f32,
    /// How much the plain share of the query's words counts, every word alike
    /// ([`Coverage::share`](super::Coverage::share)).
    pub plain_words: f32,
    /// How much the query's words in a note's title count, weighted as [`Weights::words`]. 0 leaves titles unread.
    pub title: f32,
    /// [`Weights::meaning`] and [`Weights::words`] for a lookup ([`is_lookup`](super::words::is_lookup)), whose exact
    /// match should beat stronger meaning matches.
    pub lookup_meaning: f32,
    pub lookup_words: f32,
    /// How much a note's second best chunk by meaning adds to its best when it stands out too, so a note that matches
    /// in several sections beats one lucky paragraph ([`Signals::next_z`](super::Signals::next_z)).
    pub next_chunk_share: f32,
    /// The graph's share of a blended score. Below one half, so the best direct match stays above anything the graph
    /// adds. The eval ranks best without the graph, which still finds the linked notes it doesn't ask for.
    pub graph_share: f32,
    /// How much the mean of a short query's best chunks counts in the query expanded with them, the query itself the
    /// rest.
    pub feedback: f32,
}

/// The weights search ranks with. Set by hand on the eval: DigUp's meaning and word weights and lookup multipliers, and
/// each addition's weight at its best eval run. `trove eval crates/trove/tests/fixtures/eval-queries.json --fit` fits
/// them on two thirds of the queries; its weights rank the held-out third worse (MRR 0.879 -> 0.845), so these stay.
pub const WEIGHTS: Weights = Weights {
    meaning: 1.0,
    words: 2.0,
    plain_words: 0.0,
    title: 0.0,
    lookup_meaning: 0.5,
    lookup_words: 4.0,
    next_chunk_share: 0.1,
    graph_share: 0.05,
    feedback: 0.2,
};
