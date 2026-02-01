use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Vertex<K, VV> {
    pub id: K,
    pub value: VV,
}

impl<K, VV> Vertex<K, VV> {
    pub fn new(id: K, value: VV) -> Self {
        Self { id, value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge<K, EV> {
    pub src_id: K,
    pub target_id: K,
    pub value: EV,
}

impl<K, EV> Edge<K, EV> {
    pub fn new(src_id: K, target_id: K, value: EV) -> Self {
        Self {
            src_id,
            target_id,
            value,
        }
    }
}
