use geaflow_api::function::{FilterFunction, MapFunction};
use geaflow_api::stream::PStream;
use geaflow_api::window::SizeTumblingWindow;

#[derive(Debug, Clone)]
pub struct LocalStream<T> {
    data: Vec<T>,
}

impl<T> LocalStream<T> {
    pub fn from_vec(data: Vec<T>) -> Self {
        Self { data }
    }

    pub fn window_tumbling(self, window: SizeTumblingWindow) -> LocalWindowedStream<T> {
        LocalWindowedStream {
            data: self.data,
            window_size: window.size.max(1),
        }
    }
}

pub struct LocalWindowedStream<T> {
    data: Vec<T>,
    window_size: usize,
}

impl<T: Clone> LocalWindowedStream<T> {
    pub fn collect_windows(self) -> Vec<Vec<T>> {
        self.data
            .chunks(self.window_size)
            .map(|c| c.to_vec())
            .collect()
    }
}

impl<T> PStream<T> for LocalStream<T> {
    type Output<R> = LocalStream<R>;

    fn map<R, F>(self, func: F) -> Self::Output<R>
    where
        F: MapFunction<T, R>,
    {
        let out = self.data.into_iter().map(|v| func.map(v)).collect();
        LocalStream { data: out }
    }

    fn filter<F>(self, func: F) -> Self
    where
        F: FilterFunction<T>,
    {
        let out = self.data.into_iter().filter(|v| func.filter(v)).collect();
        LocalStream { data: out }
    }

    fn collect(self) -> Vec<T> {
        self.data
    }
}
