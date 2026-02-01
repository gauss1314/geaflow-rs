# GeaFlow Rust
中文 | [English](README_EN.md)

本仓库提供 GeaFlow 的 Rust-only 实现，聚焦“生产级核心能力子集”：**Vertex-Centric（BSP）图计算引擎**。

## 能力概览
- 单机执行（Local：单机并行）
- 分布式执行（Distributed：TCP RPC driver/worker；master 可选）
- RocksDB 状态后端
- Checkpoint/恢复（按超步边界对齐）
- 参考算法：WCC / PageRank / SSSP
- Graph500 数据集 runner 与 HTTP 接口验证（curl + 代码两种方式）

说明：本仓库不包含 Java 版本的 DSL（SQL+GQL）、Console/Operator/Dashboard 等产品组件。

## 快速开始
```bash
cd geaflow-rust
cargo test
```

## 文档入口
- Runtime 文档（中文）：[geaflow-rust/README.md](geaflow-rust/README.md)
- Runtime 文档（English）：[geaflow-rust/README_EN.md](geaflow-rust/README_EN.md)
