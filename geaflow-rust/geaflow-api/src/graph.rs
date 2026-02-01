use crate::function::{VertexCentricComputeAlgorithm, VertexCentricComputeFunction};
pub use geaflow_common::types::{Edge, Vertex};

/// Represents a windowed graph stream
pub trait PGraphWindow<K, VV, EV>: Sized {
    /// Trigger vertex centric computation
    fn compute<M, F>(self, compute_function: F, parallelism: usize) -> Self
    where
        F: VertexCentricComputeFunction<K, VV, EV, M>,
        M: Send + Sync + 'static + Clone;

    fn compute_algorithm<M, A>(self, algorithm: &A, parallelism: usize) -> Self
    where
        A: VertexCentricComputeAlgorithm<K, VV, EV, M>,
        M: Send + Sync + 'static + Clone;

    /// Get the vertices as a collection (for simple verification)
    fn vertices(&self) -> Vec<Vertex<K, VV>>;

    /// Get the edges as a collection
    fn edges(&self) -> Vec<Edge<K, EV>>;
}
