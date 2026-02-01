# GeaFlow Rust

本仓库提供 GeaFlow 的 Rust-only 实现，聚焦“生产级核心能力子集”：**Vertex-Centric（BSP）图计算引擎**。

## 范围

- 本地执行（单机并行）
- 分布式执行（TCP RPC 的 driver/worker，可选 master）
- RocksDB 状态后端
- Checkpoint/恢复（按超步边界对齐）
- 参考算法：WCC / PageRank / SSSP

说明：本仓库不再包含 Java 版本的 DSL（SQL+GQL）、Console/Operator/Dashboard 等产品组件。

## 快速开始

```bash
cd geaflow-rust
cargo test
```

更多本地/分布式运行与提交流程见：
- [geaflow-rust/README_cn.md](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/README_cn.md)
