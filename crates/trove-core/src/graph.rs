mod node_key;
mod resolve;
mod vault;

pub(crate) use node_key::{EdgeKind, NodeKey};
pub(crate) use vault::VaultGraph;

/// Chance that a step of the walk jumps back to the seeds.
pub const RESTART: f32 = 0.15;
/// Power iterations of personalised PageRank.
pub const ITERATIONS: usize = 20;

/// An undirected graph with weighted edges, for walks over notes, sections and tags.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    neighbours: Vec<Vec<(usize, f32)>>,
}

impl Graph {
    pub fn new(nodes: usize) -> Self {
        Self {
            neighbours: vec![Vec::new(); nodes],
        }
    }

    pub fn len(&self) -> usize {
        self.neighbours.len()
    }

    pub fn is_empty(&self) -> bool {
        self.neighbours.is_empty()
    }

    /// Adds a node with no edges and returns it.
    pub fn add_node(&mut self) -> usize {
        self.neighbours.push(Vec::new());
        self.neighbours.len() - 1
    }

    /// Adds an edge between `a` and `b`. Edges added twice between the same nodes add up.
    pub fn add_edge(&mut self, a: usize, b: usize, weight: f32) {
        self.neighbours[a].push((b, weight));
        self.neighbours[b].push((a, weight));
    }

    /// Personalised PageRank: how often a walk that follows edges by weight, and jumps back to the seeds with
    /// probability [`RESTART`], visits each node. Seeds are chosen in proportion to their weights; a walk at a node with
    /// no edges jumps back to them too. All zero when the seeds weigh nothing.
    pub fn personalized_pagerank(&self, seeds: &[(usize, f32)]) -> Vec<f32> {
        let total: f32 = seeds.iter().map(|(_, weight)| weight).sum();
        let mut restart = vec![0.0; self.len()];
        if total <= 0.0 {
            return restart;
        }
        for &(node, weight) in seeds {
            restart[node] += weight / total;
        }

        let strengths: Vec<f32> = self
            .neighbours
            .iter()
            .map(|edges| edges.iter().map(|(_, weight)| weight).sum())
            .collect();

        let mut rank = restart.clone();
        for _ in 0..ITERATIONS {
            let mut next: Vec<f32> = restart.iter().map(|share| RESTART * share).collect();
            let mut stranded = 0.0;
            for (node, edges) in self.neighbours.iter().enumerate() {
                if strengths[node] <= 0.0 {
                    stranded += rank[node];
                    continue;
                }

                let flow = (1.0 - RESTART) * rank[node] / strengths[node];
                for &(neighbour, weight) in edges {
                    next[neighbour] += flow * weight;
                }
            }

            for (value, share) in next.iter_mut().zip(&restart) {
                *value += (1.0 - RESTART) * stranded * share;
            }
            rank = next;
        }

        rank
    }
}
