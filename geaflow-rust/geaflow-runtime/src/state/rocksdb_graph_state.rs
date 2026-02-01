use crate::state::{GraphState, SerdeKey, SerdeValue};
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use geaflow_common::types::{Edge, Vertex};
use rocksdb::checkpoint::Checkpoint;
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, WriteBatch, DB};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const CF_VERTICES: &str = "vertices";
const CF_EDGES: &str = "edges";
static EDGE_BATCH_NONCE: AtomicU64 = AtomicU64::new(1);

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

    pub fn dump_vertices_csv_u64_u64(&self, output_path: impl AsRef<Path>) -> GeaFlowResult<()> {
        let cf = self.cf(CF_VERTICES)?;
        let mut f = std::fs::File::create(output_path.as_ref()).map_err(GeaFlowError::Io)?;
        use std::io::Write;
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for kv in iter {
            let (k, v) = kv.map_err(|e| GeaFlowError::Internal(format!("rocksdb iter: {e}")))?;
            let id_bytes: Vec<u8> = Self::decode(&k)?;
            let value_bytes: Vec<u8> = Self::decode(&v)?;
            let id: u64 = bincode::deserialize(&id_bytes)
                .map_err(|e| GeaFlowError::Internal(format!("decode id: {e}")))?;
            let value: u64 = bincode::deserialize(&value_bytes)
                .map_err(|e| GeaFlowError::Internal(format!("decode value: {e}")))?;
            writeln!(&mut f, "{id},{value}")
                .map_err(|e| GeaFlowError::Internal(format!("write csv: {e}")))?;
        }
        Ok(())
    }

    pub fn dump_vertices_csv_u64_f64(&self, output_path: impl AsRef<Path>) -> GeaFlowResult<()> {
        let cf = self.cf(CF_VERTICES)?;
        let mut f = std::fs::File::create(output_path.as_ref()).map_err(GeaFlowError::Io)?;
        use std::io::Write;
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for kv in iter {
            let (k, v) = kv.map_err(|e| GeaFlowError::Internal(format!("rocksdb iter: {e}")))?;
            let id_bytes: Vec<u8> = Self::decode(&k)?;
            let value_bytes: Vec<u8> = Self::decode(&v)?;
            let id: u64 = bincode::deserialize(&id_bytes)
                .map_err(|e| GeaFlowError::Internal(format!("decode id: {e}")))?;
            let value: f64 = bincode::deserialize(&value_bytes)
                .map_err(|e| GeaFlowError::Internal(format!("decode value: {e}")))?;
            writeln!(&mut f, "{id},{value}")
                .map_err(|e| GeaFlowError::Internal(format!("write csv: {e}")))?;
        }
        Ok(())
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
        let nonce = EDGE_BATCH_NONCE.fetch_add(1, Ordering::Relaxed);
        let mut batch = WriteBatch::default();
        for (i, e) in edges.iter().enumerate() {
            let mut key = Self::encode(&e.src_id)?;
            key.extend_from_slice(&nonce.to_le_bytes());
            key.extend_from_slice(&(i as u32).to_le_bytes());
            batch.put_cf(cf, key, Self::encode(e)?);
        }

        self.db
            .write(batch)
            .map_err(|e| GeaFlowError::Internal(format!("rocksdb write batch (edges): {e}")))?;
        Ok(())
    }

    fn get_out_edges(&self, src_id: &K) -> GeaFlowResult<Vec<Edge<K, EV>>> {
        let cf = self.cf(CF_EDGES)?;
        let prefix = Self::encode(src_id)?;
        let iter = self
            .db
            .iterator_cf(cf, IteratorMode::From(&prefix, rocksdb::Direction::Forward));
        let mut out = Vec::new();
        for kv in iter {
            let (k, v) = kv.map_err(|e| GeaFlowError::Internal(format!("rocksdb iter: {e}")))?;
            if !k.starts_with(&prefix) {
                break;
            }
            let e: Edge<K, EV> = Self::decode(&v)?;
            out.push(e);
        }
        Ok(out)
    }
}
