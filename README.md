# GeaFlow Rust

This repository provides a Rust-only implementation of a production-oriented core subset of GeaFlow: **vertex-centric (BSP) graph computing**.

## Scope

- Local execution (single-machine, parallel)
- Distributed execution (TCP RPC driver/worker, optional master)
- RocksDB state backend
- Checkpoint & recovery (aligned at superstep boundaries)
- Reference algorithms: WCC / PageRank / SSSP

Out of scope in this Rust-only repo: Java-based DSL (SQL+GQL), console/operator/dashboard, and other Java product components.

## Quick Start

```bash
cd geaflow-rust
cargo test
```

For local/distributed run commands and JobSpec submission examples, see:
- [geaflow-rust README (CN)](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/README_cn.md)
