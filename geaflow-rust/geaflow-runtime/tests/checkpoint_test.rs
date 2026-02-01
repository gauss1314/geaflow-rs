use geaflow_common::types::Vertex;
use geaflow_runtime::state::rocksdb_graph_state::RocksDbGraphState;
use geaflow_runtime::state::GraphState;

#[test]
fn test_rocksdb_checkpoint_and_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let state = RocksDbGraphState::open(dir.path()).unwrap();

    <RocksDbGraphState as GraphState<i32, i32, i32>>::put_vertex_batch(
        &state,
        &[Vertex::new(1, 100), Vertex::new(2, 200)],
    )
    .unwrap();

    let checkpoint_dir = tempfile::tempdir().unwrap();
    let checkpoint_path = checkpoint_dir.path().join("cp");
    state.create_checkpoint(&checkpoint_path).unwrap();

    <RocksDbGraphState as GraphState<i32, i32, i32>>::put_vertex(&state, &1, &999).unwrap();

    let recovered = RocksDbGraphState::open(&checkpoint_path).unwrap();
    let v1 = <RocksDbGraphState as GraphState<i32, i32, i32>>::get_vertex(&recovered, &1)
        .unwrap()
        .unwrap();
    assert_eq!(v1, 100);

    let current_v1 = <RocksDbGraphState as GraphState<i32, i32, i32>>::get_vertex(&state, &1)
        .unwrap()
        .unwrap();
    assert_eq!(current_v1, 999);
}
