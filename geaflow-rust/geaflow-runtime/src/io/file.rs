use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use geaflow_common::types::{Edge, Vertex};
use std::path::Path;

pub fn read_vertices_u64_u64(
    path: impl AsRef<Path>,
    default_value: u64,
) -> GeaFlowResult<Vec<Vertex<u64, u64>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| GeaFlowError::Io(e.into()))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| GeaFlowError::Internal(format!("csv read: {e}")))?;
        let id: u64 = rec
            .get(0)
            .ok_or_else(|| GeaFlowError::InvalidArgument("vertex id missing".to_string()))?
            .trim()
            .parse()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex id parse: {e}")))?;
        let value: u64 = rec
            .get(1)
            .map(|s| s.trim().parse())
            .transpose()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex value parse: {e}")))?
            .unwrap_or(default_value);
        out.push(Vertex::new(id, value));
    }
    Ok(out)
}

pub fn read_vertices_u64_u64_id_default(
    path: impl AsRef<Path>,
) -> GeaFlowResult<Vec<Vertex<u64, u64>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| GeaFlowError::Io(e.into()))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| GeaFlowError::Internal(format!("csv read: {e}")))?;
        let id: u64 = rec
            .get(0)
            .ok_or_else(|| GeaFlowError::InvalidArgument("vertex id missing".to_string()))?
            .trim()
            .parse()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex id parse: {e}")))?;
        let value: u64 = rec
            .get(1)
            .map(|s| s.trim().parse())
            .transpose()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex value parse: {e}")))?
            .unwrap_or(id);
        out.push(Vertex::new(id, value));
    }
    Ok(out)
}

pub fn read_vertices_u64_f64(
    path: impl AsRef<Path>,
    default_value: f64,
) -> GeaFlowResult<Vec<Vertex<u64, f64>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| GeaFlowError::Io(e.into()))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| GeaFlowError::Internal(format!("csv read: {e}")))?;
        let id: u64 = rec
            .get(0)
            .ok_or_else(|| GeaFlowError::InvalidArgument("vertex id missing".to_string()))?
            .trim()
            .parse()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex id parse: {e}")))?;
        let value: f64 = rec
            .get(1)
            .map(|s| s.trim().parse())
            .transpose()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("vertex value parse: {e}")))?
            .unwrap_or(default_value);
        out.push(Vertex::new(id, value));
    }
    Ok(out)
}

pub fn read_edges_u64_u8(
    path: impl AsRef<Path>,
    default_value: u8,
) -> GeaFlowResult<Vec<Edge<u64, u8>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| GeaFlowError::Io(e.into()))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| GeaFlowError::Internal(format!("csv read: {e}")))?;
        let src: u64 = rec
            .get(0)
            .ok_or_else(|| GeaFlowError::InvalidArgument("edge src missing".to_string()))?
            .trim()
            .parse()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("edge src parse: {e}")))?;
        let target: u64 = rec
            .get(1)
            .ok_or_else(|| GeaFlowError::InvalidArgument("edge target missing".to_string()))?
            .trim()
            .parse()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("edge target parse: {e}")))?;
        let value: u8 = rec
            .get(2)
            .map(|s| s.trim().parse())
            .transpose()
            .map_err(|e| GeaFlowError::InvalidArgument(format!("edge value parse: {e}")))?
            .unwrap_or(default_value);
        out.push(Edge::new(src, target, value));
    }
    Ok(out)
}
