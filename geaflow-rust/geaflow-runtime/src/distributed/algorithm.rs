use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use serde::{Deserialize, Serialize};

pub type OutMessage = (Vec<u8>, Vec<u8>);
pub type Outbox = Vec<OutMessage>;
pub type ComputeOutput = (Option<Vec<u8>>, Outbox);
pub type ComputeResult = GeaFlowResult<ComputeOutput>;

pub trait DistributedAlgorithm: Send {
    fn name(&self) -> &str;
    fn iterations(&self) -> u64;

    fn compute_vertex(
        &mut self,
        vertex_id: &[u8],
        vertex_value: Option<&[u8]>,
        out_edges: &[(Vec<u8>, Vec<u8>)],
        messages: &[Vec<u8>],
        iteration: u64,
    ) -> ComputeResult;
}

fn encode<T: serde::Serialize>(v: &T) -> GeaFlowResult<Vec<u8>> {
    bincode::serialize(v).map_err(|e| GeaFlowError::Internal(format!("bincode encode: {e}")))
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> GeaFlowResult<T> {
    bincode::deserialize(bytes).map_err(|e| GeaFlowError::Internal(format!("bincode decode: {e}")))
}

#[derive(Debug, Clone)]
pub struct WccAlgorithm {
    iterations: u64,
}

impl WccAlgorithm {
    pub fn new(iterations: u64) -> Self {
        Self { iterations }
    }
}

impl DistributedAlgorithm for WccAlgorithm {
    fn name(&self) -> &str {
        "wcc"
    }

    fn iterations(&self) -> u64 {
        self.iterations
    }

    fn compute_vertex(
        &mut self,
        vertex_id: &[u8],
        vertex_value: Option<&[u8]>,
        out_edges: &[(Vec<u8>, Vec<u8>)],
        messages: &[Vec<u8>],
        iteration: u64,
    ) -> ComputeResult {
        let vid: u64 = decode(vertex_id)?;
        let mut current: u64 = vertex_value.map(decode).transpose()?.unwrap_or(vid);

        if iteration == 1 {
            current = vid;
            let msg = encode(&current)?;
            let out = out_edges
                .iter()
                .map(|(t, _)| (t.clone(), msg.clone()))
                .collect();
            return Ok((Some(encode(&current)?), out));
        }

        let mut min_comp = current;
        for m in messages {
            let v: u64 = decode(m)?;
            if v < min_comp {
                min_comp = v;
            }
        }

        if min_comp < current {
            let msg = encode(&min_comp)?;
            let out = out_edges
                .iter()
                .map(|(t, _)| (t.clone(), msg.clone()))
                .collect();
            Ok((Some(encode(&min_comp)?), out))
        } else {
            Ok((None, Vec::new()))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRankParams {
    pub alpha: f64,
}

#[derive(Debug, Clone)]
pub struct PageRankAlgorithm {
    iterations: u64,
    alpha: f64,
    teleport: Option<f64>,
}

impl PageRankAlgorithm {
    pub fn new(iterations: u64, alpha: f64) -> Self {
        Self {
            iterations,
            alpha,
            teleport: None,
        }
    }

    pub fn from_params(iterations: u64, params: &[u8]) -> GeaFlowResult<Self> {
        let p: PageRankParams = decode(params)?;
        Ok(Self::new(iterations, p.alpha))
    }
}

impl DistributedAlgorithm for PageRankAlgorithm {
    fn name(&self) -> &str {
        "pagerank"
    }

    fn iterations(&self) -> u64 {
        self.iterations
    }

    fn compute_vertex(
        &mut self,
        _vertex_id: &[u8],
        vertex_value: Option<&[u8]>,
        out_edges: &[(Vec<u8>, Vec<u8>)],
        messages: &[Vec<u8>],
        iteration: u64,
    ) -> ComputeResult {
        let out_degree = out_edges.len() as f64;
        let vertex_value: f64 = vertex_value.map(decode).transpose()?.unwrap_or(0.0);
        if self.teleport.is_none() && vertex_value > 0.0 {
            self.teleport = Some((1.0 - self.alpha) * vertex_value);
        }

        if iteration == 1 {
            if out_degree > 0.0 {
                let msg = encode(&(vertex_value / out_degree))?;
                let out = out_edges
                    .iter()
                    .map(|(t, _)| (t.clone(), msg.clone()))
                    .collect();
                return Ok((None, out));
            }
            return Ok((None, Vec::new()));
        }

        let mut sum = 0.0;
        for m in messages {
            let v: f64 = decode(m)?;
            sum += v;
        }
        let pr = sum * self.alpha + self.teleport.unwrap_or(1.0 - self.alpha);

        let mut out = Vec::new();
        if out_degree > 0.0 {
            let msg = encode(&(pr / out_degree))?;
            out = out_edges
                .iter()
                .map(|(t, _)| (t.clone(), msg.clone()))
                .collect();
        }

        Ok((Some(encode(&pr)?), out))
    }
}
