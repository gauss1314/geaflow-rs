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

## 部署与运行
### 单机（Local）
使用 `geaflow-submit` 在本机并行执行（示例：WCC）：
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

### 集群/分布式（Distributed）
分布式模式由 driver/worker 组成（master 可选，推荐用于 worker 注册与运维查询）。典型启动顺序：
1) master（可选） → 2) workers（N 个） → 3) driver → 4) submit 作业

启动示例（本机多进程模拟小集群）：
```bash
cd geaflow-rust

cargo run -p geaflow-runtime --bin geaflow-master -- \
  --listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17000

cargo run -p geaflow-runtime --bin geaflow-worker -- \
  --listen 127.0.0.1:9001 \
  --state-dir /tmp/geaflow-worker-1 \
  --master 127.0.0.1:7000

cargo run -p geaflow-runtime --bin geaflow-driver -- \
  --listen 127.0.0.1:7100 \
  --master 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17100

cargo run -p geaflow-runtime --bin geaflow-submit -- \
  --mode distributed \
  --algorithm pagerank \
  --vertices ./data/vertices.csv \
  --edges ./data/edges.csv \
  --iterations 10 \
  --alpha 0.85 \
  --driver 127.0.0.1:7100
```

运维接口示例：
- `curl -sS http://127.0.0.1:17000/workers`
- `curl -sS http://127.0.0.1:17100/jobs`

Docker Compose 一键启动：
```bash
cd geaflow-rust
docker compose -f deploy/docker-compose.yml up
```

## 文档入口
- Runtime 文档（中文）：[geaflow-rust/README.md](geaflow-rust/README.md)
- Runtime 文档（English）：[geaflow-rust/README_EN.md](geaflow-rust/README_EN.md)
