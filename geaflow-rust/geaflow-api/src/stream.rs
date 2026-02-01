use crate::function::{FilterFunction, MapFunction};

pub trait PStream<T>: Sized {
    type Output<R>: PStream<R>;

    fn map<R, F>(self, func: F) -> Self::Output<R>
    where
        F: MapFunction<T, R>;

    fn filter<F>(self, func: F) -> Self
    where
        F: FilterFunction<T>;

    fn collect(self) -> Vec<T>;
}
