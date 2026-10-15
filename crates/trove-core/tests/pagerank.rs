use trove_core::graph::Graph;

/// A triangle 0-1-2 whose edge 1-2 weighs 2, and node 3 hanging off node 2. Seeded at node 0, the exact ranks solve
/// `r = 0.15 s + 0.85 r P` (P the weights normalised per row, s the seed), which comes to
/// `(111080, 101541, 127840, 27166) / 367627`. The graph is not bipartite, so 20 iterations get within 1e-4.
#[test]
fn personalized_pagerank_matches_the_exact_ranks_of_a_small_graph() {
    let mut graph = Graph::default();
    for _ in 0..4 {
        graph.add_node();
    }
    graph.add_edge(0, 1, 1.0);
    graph.add_edge(1, 2, 2.0);
    graph.add_edge(0, 2, 1.0);
    graph.add_edge(2, 3, 1.0);

    let ranks = graph.personalized_pagerank(&[(0, 1.0)]);

    assert_eq!(ranks.len(), 4, "{ranks:?}");
    let exact = [111080.0, 101541.0, 127840.0, 27166.0].map(|count| count / 367627.0);
    for (rank, exact) in ranks.iter().zip(exact) {
        assert!((rank - exact).abs() < 1e-4, "{ranks:?} against {exact}");
    }
}
