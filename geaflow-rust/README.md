# GeaFlow Rust Runtime
中文 | [English](README_EN.md)

本目录提供 GeaFlow Rust runtime：对 GeaFlow（Java）核心 **Vertex-Centric（BSP）图计算模型** 的 Rust 重构实现，覆盖生产级核心能力子集。

## 1. 功能清单
- 运行模式：Local（单机并行）、Distributed（TCP RPC driver/worker；master 可选）
- 状态后端：RocksDB
- Checkpoint/恢复：按超步边界对齐（见集成测试）
- 算法：WCC / PageRank / SSSP
- CLI：`geaflow-submit`、`geaflow-master`、`geaflow-worker`、`geaflow-driver`、`geaflow-graph500`
- HTTP 接口：
  - Master：`GET /healthz`、`GET /workers`
  - Driver：`GET /healthz`、`GET /jobs`
  - Graph500：提供 dataset 注册、任务提交、任务查询（curl 调用，见下文）

## 2. 环境依赖与安装
### 2.1 Rust 工具链
- 推荐 Rust stable（项目使用 edition 2021）。

### 2.2 RocksDB 编译依赖（本机开发）
RocksDB 依赖本地 C/C++ 工具链。常见环境需要：clang、cmake、pkg-config（以及系统 SDK）。若 `cargo build` 在 rocksdb-sys 处失败，请优先检查上述依赖是否齐全。

## 3. 单机模式（Local）
### 3.1 输入数据格式
默认使用无表头 CSV：
- 顶点：`vertex_id,vertex_value`（第二列可省略；算法可提供默认值）
- 边：`src_id,target_id,edge_value`（第三列可省略）

### 3.2 运行示例（WCC）
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

### 3.3 运行示例（PageRank）
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

## 4. 分布式模式（Distributed）
### 4.1 启动 Master（可选但推荐）
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-master -- \
  --listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17000
```

可用 HTTP：
- `curl -sS http://127.0.0.1:17000/healthz`
- `curl -sS http://127.0.0.1:17000/workers`

### 4.2 启动 Worker
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-worker -- \
  --listen 127.0.0.1:9001 \
  --state-dir /tmp/geaflow-worker-1 \
  --master 127.0.0.1:7000
```

### 4.3 启动 Driver（推荐）
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-driver -- \
  --listen 127.0.0.1:7100 \
  --master 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17100
```

可用 HTTP：
- `curl -sS http://127.0.0.1:17100/healthz`
- `curl -sS http://127.0.0.1:17100/jobs`

### 4.4 提交作业（推荐：提交到 Driver）
Driver 的 Job 提交协议是 TCP 自定义协议（非 HTTP）。推荐使用 CLI `geaflow-submit`：
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

### 4.5 Docker Compose（本机一键起分布式）
```bash
cd geaflow-rust
docker compose -f deploy/docker-compose.yml up
```

## 5. Graph500 数据集测试（代码方式 + 接口方式）
### 5.1 数据集解压（以 graph500-22 为例）
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --release --bin geaflow-graph500 -- extract \
  --archive ../graph500-test/graph500-22.tar.zst \
  --out-dir /tmp/graph500-22
```

### 5.2 代码方式：直接运行并比对真值（WCC + PageRank）
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

### 5.3 接口方式：HTTP API + curl 完成同样验证
#### 5.3.1 启动 Graph500 HTTP 服务
```bash
cd geaflow-rust
cargo run -p geaflow-runtime --release --bin geaflow-graph500 -- \
  serve --listen 127.0.0.1:18080 --state-dir /tmp/geaflow-graph500-http
```

#### 5.3.2 注册数据集（.properties + .v）
```bash
curl -sS -X POST http://127.0.0.1:18080/api/v1/datasets/register \
  -H 'Content-Type: application/json' \
  -d '{
    "dataset_id":"graph500-22",
    "properties_path":"/tmp/graph500-22/graph500-22.properties",
    "vertex_path":"/tmp/graph500-22/graph500-22.v"
  }'
```

#### 5.3.3 提交验证任务（WCC + PR）
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

#### 5.3.4 查询任务结果
```bash
curl -sS http://127.0.0.1:18080/api/v1/graph500/jobs/<job_id>
```

判定通过标准：
- WCC：`mismatches=0` 且 `unexpected=0` 且 `missing=0`
- PageRank：`unexpected=0` 且 `missing=0` 且 `max_abs_diff <= 1e-6`

## 6. 关键代码入口
- 本地分区并行引擎：`geaflow-runtime/src/graph/partitioned_graph.rs`
- 分布式 Master/Driver/Worker：`geaflow-runtime/src/distributed/*`
- RocksDB 状态：`geaflow-runtime/src/state/rocksdb_graph_state.rs`
- 算法：`geaflow-runtime/src/algorithms/*`
- HTTP：`geaflow-runtime/src/http/mod.rs`
