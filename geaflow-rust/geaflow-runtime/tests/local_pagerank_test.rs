use geaflow_api::graph::PGraphWindow;
use geaflow_common::types::{Edge, Vertex};
use geaflow_runtime::algorithms::pagerank::PageRankAlgorithm;
use geaflow_runtime::graph::partitioned_graph::PartitionedGraph;

#[test]
fn test_local_pagerank_two_cycle() {
    let vertices = vec![Vertex::new(1u64, 1.0f64), Vertex::new(2, 1.0)];
    let edges = vec![Edge::new(1u64, 2u64, 0u8), Edge::new(2u64, 1u64, 0u8)];

    let graph = PartitionedGraph::new(vertices, edges, 2);
    let algo = PageRankAlgorithm::new(3, 0.85);
    let result = graph.compute_algorithm(&algo, 2);

    let mut vertices = result.vertices();
    vertices.sort_by_key(|v| v.id);

    assert_eq!(vertices.len(), 2);
    assert!((vertices[0].value - 1.0).abs() < 1e-9);
    assert!((vertices[1].value - 1.0).abs() < 1e-9);
}
