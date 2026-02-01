use geaflow_api::stream::PStream;
use geaflow_api::window::SizeTumblingWindow;
use geaflow_runtime::stream::LocalStream;

#[test]
fn test_local_stream_map_filter_collect() {
    let out = LocalStream::from_vec(vec![1i32, 2, 3, 4])
        .map(|x| x * 10)
        .filter(|x: &i32| *x >= 30)
        .collect();
    assert_eq!(out, vec![30, 40]);
}

#[test]
fn test_local_stream_tumbling_window_by_count() {
    let windows = LocalStream::from_vec(vec![1i32, 2, 3, 4, 5])
        .window_tumbling(SizeTumblingWindow { size: 2 })
        .collect_windows();
    assert_eq!(windows, vec![vec![1, 2], vec![3, 4], vec![5]]);
}
