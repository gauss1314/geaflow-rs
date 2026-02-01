use geaflow_common::types::{Edge, Vertex};
use geaflow_runtime::state::rocksdb_graph_state::RocksDbGraphState;
use geaflow_runtime::state::GraphState;

#[test]
fn test_rocksdb_graph_state_basic() {
    let dir = tempfile::tempdir().unwrap();
    let state = RocksDbGraphState::open(dir.path()).unwrap();

    <RocksDbGraphState as GraphState<i32, i32, i32>>::put_vertex_batch(
        &state,
        &[
            Vertex::new(1i32, 10i32),
            Vertex::new(2i32, 20i32),
            Vertex::new(3i32, 30i32),
        ],
    )
    .unwrap();

    assert_eq!(
        <RocksDbGraphState as GraphState<i32, i32, i32>>::get_vertex(&state, &2).unwrap(),
        Some(20)
    );
    assert_eq!(
        <RocksDbGraphState as GraphState<i32, i32, i32>>::get_vertex(&state, &999).unwrap(),
        None
    );

    <RocksDbGraphState as GraphState<i32, i32, i32>>::put_edge_batch(
        &state,
        &[
            Edge::new(1i32, 2i32, 10i32),
            Edge::new(1i32, 3i32, 100i32),
            Edge::new(2i32, 3i32, 20i32),
        ],
    )
    .unwrap();

    let out_1 =
        <RocksDbGraphState as GraphState<i32, i32, i32>>::get_out_edges(&state, &1).unwrap();
    assert_eq!(out_1.len(), 2);

    let out_2 =
        <RocksDbGraphState as GraphState<i32, i32, i32>>::get_out_edges(&state, &2).unwrap();
    assert_eq!(out_2.len(), 1);

    let out_9 =
        <RocksDbGraphState as GraphState<i32, i32, i32>>::get_out_edges(&state, &9).unwrap();
    assert_eq!(out_9.len(), 0);
}
