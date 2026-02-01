use std::iter::Iterator;

pub trait Function: Send + Sync + 'static {}

pub trait MapFunction<T, R>: Function {
    fn map(&self, value: T) -> R;
}

impl<T, R, F> MapFunction<T, R> for F
where
    F: Fn(T) -> R + Send + Sync + 'static,
    T: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn map(&self, value: T) -> R {
        (self)(value)
    }
}

pub trait FilterFunction<T>: Function {
    fn filter(&self, value: &T) -> bool;
}

impl<T, F> FilterFunction<T> for F
where
    F: Fn(&T) -> bool + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    fn filter(&self, value: &T) -> bool {
        (self)(value)
    }
}

/// Context for Vertex Centric Compute
pub trait VertexCentricComputeFuncContext<K, VV, EV, M> {
    fn vertex_value(&self) -> Option<&VV>;
    fn set_new_vertex_value(&mut self, value: VV);
    fn send_message(&mut self, target_id: K, message: M);
    fn edges(&self) -> Box<dyn Iterator<Item = &crate::graph::Edge<K, EV>> + '_>;
    fn iteration(&self) -> u64;
}

pub trait VertexCentricComputeFunction<K, VV, EV, M>: Function {
    fn init(&mut self, _context: &mut dyn VertexCentricComputeFuncContext<K, VV, EV, M>) {}

    fn compute(
        &mut self,
        vertex_id: &K,
        message_iterator: &mut dyn Iterator<Item = M>,
        context: &mut dyn VertexCentricComputeFuncContext<K, VV, EV, M>,
    );

    fn finish(&mut self, _context: &mut dyn VertexCentricComputeFuncContext<K, VV, EV, M>) {}
}

pub trait VertexCentricComputeAlgorithm<K, VV, EV, M>: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn iterations(&self) -> u64;
    fn create_function(&self) -> Box<dyn VertexCentricComputeFunction<K, VV, EV, M>>;
}

impl<F> Function for F where F: Send + Sync + 'static {}
