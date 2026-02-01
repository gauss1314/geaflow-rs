use geaflow_api::graph::PGraphWindow;
use geaflow_common::types::{Edge, Vertex};
use geaflow_runtime::algorithms::wcc::WccAlgorithm;
use geaflow_runtime::graph::partitioned_graph::PartitionedGraph;

#[test]
fn test_local_wcc_parallel() {
    let vertices = vec![
        Vertex::new(1u64, 1u64),
        Vertex::new(2, 2),
        Vertex::new(3, 3),
    ];
    let edges = vec![
        Edge::new(1u64, 2u64, 0u8),
        Edge::new(2u64, 1u64, 0u8),
        Edge::new(2u64, 3u64, 0u8),
        Edge::new(3u64, 2u64, 0u8),
    ];

    let graph = PartitionedGraph::new(vertices, edges, 2);
    let algo = WccAlgorithm::new(10);
    let result = graph.compute_algorithm(&algo, 2);

    let mut vertices = result.vertices();
    vertices.sort_by_key(|v| v.id);

    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].value, 1);
    assert_eq!(vertices[1].value, 1);
    assert_eq!(vertices[2].value, 1);
}
