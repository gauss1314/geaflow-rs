# GeaFlow Rust Runtime
English | [中文](README.md)

This directory contains the GeaFlow Rust runtime: a Rust re-implementation of GeaFlow (Java) core **Vertex-Centric (BSP)** graph computing model.

## 1. Features
- Execution modes: Local (single-machine parallel), Distributed (TCP RPC driver/worker; optional master)
- State backend: RocksDB
- Checkpoint/recovery: aligned at superstep boundaries (see integration tests)
- Algorithms: WCC / PageRank / SSSP
- CLIs: `geaflow-submit`, `geaflow-master`, `geaflow-worker`, `geaflow-driver`, `geaflow-graph500`
- HTTP endpoints:
  - Master: `GET /healthz`, `GET /workers`
  - Driver: `GET /healthz`, `GET /jobs`
  - Graph500: dataset register, job submit, job query (curl; see below)

## 2. Prerequisites
- Rust stable toolchain (edition 2021).
- RocksDB native build dependencies (clang, cmake, pkg-config). If `cargo build` fails at rocksdb-sys, check your C/C++ toolchain first.

## 3. Local mode
Input format (no header CSV):
- Vertices: `vertex_id,vertex_value` (value may be omitted depending on algorithm)
- Edges: `src_id,target_id,edge_value` (value may be omitted)

Run WCC:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-submit -- \
  --mode local \
  --algorithm wcc \
  --vertices ./data/vertices.csv \
  --edges ./data/edges.csv \
  --iterations 10 \
  --parallelism 4
```

Run PageRank:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-submit -- \
  --mode local \
  --algorithm pagerank \
  --vertices ./data/vertices.csv \
  --edges ./data/edges.csv \
  --iterations 10 \
  --alpha 0.85 \
  --parallelism 4
```

## 4. Distributed mode
Start master (optional but recommended):
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-master -- \
  --listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17000
```

Start workers:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-worker -- \
  --listen 127.0.0.1:9001 \
  --state-dir /tmp/geaflow-worker-1 \
  --master 127.0.0.1:7000
```

Start driver:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-driver -- \
  --listen 127.0.0.1:7100 \
  --master 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17100
```

Submit a job (driver protocol is TCP, so use `geaflow-submit`):
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-submit -- \
  --mode distributed \
  --algorithm pagerank \
  --vertices ./data/vertices.csv \
  --edges ./data/edges.csv \
  --iterations 10 \
  --alpha 0.85 \
  --driver 127.0.0.1:7100
```

Docker Compose (bring up master/driver/workers locally):
```bash
cd geaflow-rust
docker compose -f deploy/docker-compose.yml up
```

## 5. Graph500 dataset verification (code + HTTP API)
Extract graph500-22:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --release --bin geaflow-graph500 -- extract \
  --archive ../graph500-test/graph500-22.tar.zst \
  --out-dir /tmp/graph500-22
```

### 5.1 Code path (runner)
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --release --bin geaflow-graph500 -- run-all \
  --dir /tmp/graph500-22 \
  --workers 4 \
  --out-dir /tmp/geaflow-graph500 \
  --wcc-iterations 50 \
  --pr-iterations 10 \
  --alpha 0.85
```

### 5.2 HTTP API path (curl)
Start service:
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --release --bin geaflow-graph500 -- \
  serve --listen 127.0.0.1:18080 --state-dir /tmp/geaflow-graph500-http
```

Register dataset (`.properties` + `.v`):
```bash
curl -sS -X POST http://127.0.0.1:18080/api/v1/datasets/register \
  -H 'Content-Type: application/json' \
  -d '{
    "dataset_id":"graph500-22",
    "properties_path":"/tmp/graph500-22/graph500-22.properties",
    "vertex_path":"/tmp/graph500-22/graph500-22.v"
  }'
```

Submit verification job:
```bash
curl -sS -X POST http://127.0.0.1:18080/api/v1/graph500/jobs \
  -H 'Content-Type: application/json' \
  -d '{
    "dataset_id":"graph500-22",
    "workers":4,
    "wcc_iterations":50,
    "pr_iterations":10,
    "alpha":0.85,
    "run":["wcc","pr"],
    "out_dir":"/tmp/geaflow-graph500-http"
  }'
```

Query job:
```bash
curl -sS http://127.0.0.1:18080/api/v1/graph500/jobs/<job_id>
```

Pass criteria:
- WCC: `mismatches=0` and `unexpected=0` and `missing=0`
- PageRank: `unexpected=0` and `missing=0` and `max_abs_diff <= 1e-6`

## 6. Code entry points
- Local partitioned engine: `geaflow-runtime/src/graph/partitioned_graph.rs`
- Distributed runtime: `geaflow-runtime/src/distributed/*`
- RocksDB state: `geaflow-runtime/src/state/rocksdb_graph_state.rs`
- Algorithms: `geaflow-runtime/src/algorithms/*`
- HTTP: `geaflow-runtime/src/http/mod.rs`
