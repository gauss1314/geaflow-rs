use crate::state::{GraphState, SerdeKey, SerdeValue};
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use geaflow_common::types::{Edge, Vertex};
use rocksdb::checkpoint::Checkpoint;
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, WriteBatch, DB};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const CF_VERTICES: &str = "vertices";
const CF_EDGES: &str = "edges";

#[derive(Clone)]
pub struct RocksDbGraphState {
    db: Arc<DB>,
}

impl RocksDbGraphState {
    pub fn open(path: impl AsRef<Path>) -> GeaFlowResult<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_VERTICES, Options::default()),
            ColumnFamilyDescriptor::new(CF_EDGES, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb open failed: {e}")))?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn create_checkpoint(&self, checkpoint_dir: impl AsRef<Path>) -> GeaFlowResult<()> {
        let cp = Checkpoint::new(&self.db)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb checkpoint init: {e}")))?;
        cp.create_checkpoint(checkpoint_dir)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb create checkpoint: {e}")))?;
        Ok(())
    }

    fn cf(&self, name: &str) -> GeaFlowResult<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| GeaFlowError::Internal(format!("missing column family: {name}")))
    }

    fn encode<T: serde::Serialize>(v: &T) -> GeaFlowResult<Vec<u8>> {
        bincode::serialize(v).map_err(|e| GeaFlowError::Internal(format!("bincode encode: {e}")))
    }

    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> GeaFlowResult<T> {
        bincode::deserialize(bytes)
            .map_err(|e| GeaFlowError::Internal(format!("bincode decode: {e}")))
    }
}

impl<K, VV, EV> GraphState<K, VV, EV> for RocksDbGraphState
where
    K: SerdeKey + Clone + Eq + std::hash::Hash,
    VV: SerdeValue,
    EV: SerdeValue + Clone,
{
    fn put_vertex(&self, id: &K, value: &VV) -> GeaFlowResult<()> {
        let cf = self.cf(CF_VERTICES)?;
        let k = Self::encode(id)?;
        let v = Self::encode(value)?;
        self.db
            .put_cf(cf, k, v)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb put vertex: {e}")))?;
        Ok(())
    }

    fn get_vertex(&self, id: &K) -> GeaFlowResult<Option<VV>> {
        let cf = self.cf(CF_VERTICES)?;
        let k = Self::encode(id)?;
        let v = self
            .db
            .get_cf(cf, k)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb get vertex: {e}")))?;
        match v {
            None => Ok(None),
            Some(bytes) => Ok(Some(Self::decode(&bytes)?)),
        }
    }

    fn put_vertex_batch(&self, vertices: &[Vertex<K, VV>]) -> GeaFlowResult<()> {
        let cf = self.cf(CF_VERTICES)?;
        let mut batch = WriteBatch::default();
        for v in vertices {
            batch.put_cf(cf, Self::encode(&v.id)?, Self::encode(&v.value)?);
        }
        self.db
            .write(batch)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb write batch (vertices): {e}")))?;
        Ok(())
    }

    fn list_vertices(&self) -> GeaFlowResult<Vec<Vertex<K, VV>>> {
        let cf = self.cf(CF_VERTICES)?;
        let mut out = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for kv in iter {
            let (k, v) = kv.map_err(|e| GeaFlowError::Internal(format!("rocksdb iter: {e}")))?;
            let id: K = Self::decode(&k)?;
            let value: VV = Self::decode(&v)?;
            out.push(Vertex { id, value });
        }
        Ok(out)
    }

    fn put_edge_batch(&self, edges: &[Edge<K, EV>]) -> GeaFlowResult<()> {
        let cf = self.cf(CF_EDGES)?;

        let mut grouped: HashMap<K, Vec<Edge<K, EV>>> = HashMap::new();
        for e in edges {
            grouped.entry(e.src_id.clone()).or_default().push(e.clone());
        }

        let mut batch = WriteBatch::default();
        for (src, mut src_edges) in grouped {
            let key = Self::encode(&src)?;
            let mut existing: Vec<Edge<K, EV>> = match self.db.get_cf(cf, &key) {
                Ok(Some(bytes)) => Self::decode(&bytes)?,
                Ok(None) => Vec::new(),
                Err(e) => return Err(GeaFlowError::Internal(format!("rocksdb get edges: {e}"))),
            };

            existing.append(&mut src_edges);
            batch.put_cf(cf, key, Self::encode(&existing)?);
        }

        self.db
            .write(batch)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb write batch (edges): {e}")))?;
        Ok(())
    }

    fn get_out_edges(&self, src_id: &K) -> GeaFlowResult<Vec<Edge<K, EV>>> {
        let cf = self.cf(CF_EDGES)?;
        let key = Self::encode(src_id)?;
        let v = self
            .db
            .get_cf(cf, key)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb get edges: {e}")))?;
        match v {
            None => Ok(Vec::new()),
            Some(bytes) => Ok(Self::decode(&bytes)?),
        }
    }
}
