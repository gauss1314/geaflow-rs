# GeaFlow Rust（生产级核心能力子集）

本目录是对 GeaFlow（Java）核心 **Vertex-Centric（BSP）图计算模型** 的 Rust 重构实现，重点覆盖：
- 本地分区并行执行（Local）
- Driver/Worker 分布式执行（Distributed，TCP RPC）
- RocksDB 状态后端与 Checkpoint/恢复
- PageRank / WCC 算法与一致性回归测试

## 1. 快速开始

### 1.1 构建与测试

在仓库根目录执行：

```bash
cd geaflow-rust
cargo test
```

### 1.2 输入数据格式

默认使用 **无表头 CSV**：
- 顶点文件：`vertex_id,vertex_value`（第二列可省略，按算法提供默认值）
- 边文件：`src_id,target_id,edge_value`（第三列可省略）

示例：
```text
1,1
2,1
3,1
```

```text
1,2,0
2,1,0
2,3,0
3,2,0
```

## 2. 本地模式（Local）

使用 `geaflow-submit` 直接本地执行：

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

## 3. 分布式模式（Distributed）

### 3.1 启动 Master（可选但推荐）

Master 负责 worker 注册/心跳与存活列表查询：

```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-master -- \
  --listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17000
```

可用的 HTTP 端点：
- `GET /healthz`
- `GET /workers`

### 3.2 启动 Worker

每个 worker 需要独立的监听地址与本地状态目录（RocksDB）。可选开启指标端口：

```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-worker -- \
  --listen 127.0.0.1:9001 \
  --state-dir /tmp/geaflow-worker-1 \
  --metrics-listen 127.0.0.1:19001 \
  --master 127.0.0.1:7000
```

### 3.3 启动 Driver（推荐）

Driver 负责接收 JobSpec 并调度执行：

```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-driver -- \
  --listen 127.0.0.1:7100 \
  --master 127.0.0.1:7000 \
  --http-listen 127.0.0.1:17100
```

可用的 HTTP 端点：
- `GET /healthz`
- `GET /jobs`

### 3.4 提交作业（推荐：提交到 Driver）

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

输出为 `vertex_id,vertex_value`。

### 3.5 直接连接 Worker（兼容模式）

不启动 driver 的情况下，也可以直接连接 worker 执行（内嵌 driver）：

```bash
cd geaflow-rust
cargo run -p geaflow-runtime --bin geaflow-submit -- \
  --mode distributed \
  --algorithm wcc \
  --vertices ./data/vertices.csv \
  --edges ./data/edges.csv \
  --iterations 10 \
  --workers 127.0.0.1:9001,127.0.0.1:9002
```

## 4. Checkpoint 与恢复

状态后端使用 RocksDB，并支持基于 RocksDB Checkpoint 的快照创建与从快照目录恢复打开。可参考集成测试：
- `geaflow-runtime/tests/checkpoint_test.rs`
- `geaflow-runtime/tests/distributed_checkpoint_recovery_test.rs`

## 5. 代码入口

- 本地分区并行引擎：`geaflow-runtime/src/graph/partitioned_graph.rs`
- 单机内存引擎（最小可跑）：`geaflow-runtime/src/graph/mem_graph.rs`
- 分布式 Master/Driver/Worker：`geaflow-runtime/src/distributed/master.rs`、`geaflow-runtime/src/distributed/driver_service.rs`、`geaflow-runtime/src/distributed/driver.rs`、`geaflow-runtime/src/distributed/worker.rs`
- RocksDB 状态：`geaflow-runtime/src/state/rocksdb_graph_state.rs`
- 算法：`geaflow-runtime/src/algorithms/*`
- CLI：`geaflow-runtime/src/bin/geaflow-submit.rs`、`geaflow-runtime/src/bin/geaflow-master.rs`、`geaflow-runtime/src/bin/geaflow-driver.rs`、`geaflow-runtime/src/bin/geaflow-worker.rs`
