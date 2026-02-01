# GeaFlow Rust
English | [中文](README.md)

This repository provides a Rust-only, production-oriented core subset of GeaFlow: **vertex-centric (BSP) graph computing**.

## Features
- Local execution (single-machine, parallel)
- Distributed execution (TCP RPC driver/worker; optional master)
- RocksDB state backend
- Checkpoint & recovery (aligned at superstep boundaries)
- Reference algorithms: WCC / PageRank / SSSP
- Graph500 dataset runner and HTTP API for end-to-end verification

Out of scope in this Rust-only repo: Java-based DSL (SQL+GQL), console/operator/dashboard, and other Java product components.

## Quick Start
```bash
cd geaflow-rust
cargo test
```

## Documentation
- Runtime docs (Chinese): [geaflow-rust/README.md](geaflow-rust/README.md)
- Runtime docs (English): [geaflow-rust/README_EN.md](geaflow-rust/README_EN.md)
