pub mod checkpoint_meta;
pub mod rocksdb_graph_state;

use geaflow_common::error::GeaFlowResult;
use geaflow_common::types::{Edge, Vertex};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait GraphState<K, VV, EV>: Send + Sync {
    fn put_vertex(&self, id: &K, value: &VV) -> GeaFlowResult<()>;
    fn get_vertex(&self, id: &K) -> GeaFlowResult<Option<VV>>;

    fn put_vertex_batch(&self, vertices: &[Vertex<K, VV>]) -> GeaFlowResult<()>;
    fn list_vertices(&self) -> GeaFlowResult<Vec<Vertex<K, VV>>>;

    fn put_edge_batch(&self, edges: &[Edge<K, EV>]) -> GeaFlowResult<()>;
    fn get_out_edges(&self, src_id: &K) -> GeaFlowResult<Vec<Edge<K, EV>>>;
}

pub trait SerdeKey: Serialize + DeserializeOwned + Send + Sync + 'static {}
impl<T> SerdeKey for T where T: Serialize + DeserializeOwned + Send + Sync + 'static {}

pub trait SerdeValue: Serialize + DeserializeOwned + Send + Sync + 'static {}
impl<T> SerdeValue for T where T: Serialize + DeserializeOwned + Send + Sync + 'static {}
