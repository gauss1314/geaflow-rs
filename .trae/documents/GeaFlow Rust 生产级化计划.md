# GeaFlow Rust 生产级可用：任务分析与实施计划

当前 Rust 侧只实现了一个“单机内存版”的 Vertex-Centric（BSP）原型：`InMemoryGraph` 循环迭代、收集消息直到收敛/上限，并用 SSSP 测试验证了最小闭环（可参考 [mem_graph.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/graph/mem_graph.rs#L1-L139)、[sssp_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/sssp_test.rs#L1-L65)）。要“生产级可用”，必须补齐：并行分区执行、持久化状态、容错（checkpoint/recovery）、可观测性、标准化作业提交/配置、以及与 Java 示例/测试的一致性（至少 PageRank/WCC 这类 VertexCentric 示例：见 [PageRank.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/main/java/org/apache/geaflow/example/graph/statical/compute/pagerank/PageRank.java#L55-L166)、[WeakConnectedComponents.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/main/java/org/apache/geaflow/example/graph/statical/compute/weakconnectedcomponents/WeakConnectedComponents.java#L54-L167)、[PageRankTest.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/test/java/org/apache/geaflow/example/graph/PageRankTest.java#L37-L185)）。

下面给出“以 Rust 复刻 Java VertexCentric 作业执行体验 + 具备生产化运行能力”的实现计划。

## 1. 目标与边界
- **目标**：把 `geaflow-rust` 提升为“可部署、可观测、可恢复、可横向扩展”的 Vertex-Centric 图计算引擎；并提供与 Java 示例一致的算法实现与测试（至少 PageRank + WCC）。
- **边界**：不在本轮直接完整移植 Java 的 Console/Dashboard、DSL(SQL/GQL) 解析与全套分布式资源管理；优先把“核心计算引擎 + 作业提交 + 容错/状态/网络”做到生产级。

## 2. 现状差距（需要补齐的生产能力）
- **执行层**：当前按顶点串行 loop；`parallelism` 未生效；没有分区、shuffle、barrier。
- **状态层**：只有 HashMap；没有持久化/增量快照/恢复。
- **容错语义**：没有 checkpoint、重放、失败恢复；收敛条件不严谨（硬编码上限 30）。
- **作业模型**：缺少 `Environment/Pipeline/TaskContext` 这类与 Java 接近的提交入口（Java 示例 `Environment → Pipeline → submit → execute`）。
- **I/O 与部署**：无标准输入输出（文件/Socket 等），无二进制发布方式。
- **可观测性**：缺 metrics/tracing/log 规范。
- **测试保障**：仅 1 个算法测试；缺一致性回归、故障注入、基准。

## 3. 总体架构（Rust 侧生产化形态）
- **API 层（geaflow-api）**：稳定对外接口（GraphWindow/GraphCompute/ComputeFunction/Context），尽量对齐 Java 的抽象（例如 `PGraphWindow → compute() → compute(parallelism) → get_vertices()` 的调用体验可参考 [PGraphWindow.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-core/geaflow-api/src/main/java/org/apache/geaflow/api/graph/PGraphWindow.java#L27-L65) 与 [PGraphCompute.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-core/geaflow-api/src/main/java/org/apache/geaflow/api/graph/compute/PGraphCompute.java#L20-L48)）。
- **Runtime（geaflow-runtime）**：实现两种模式：
  1) **Local 多线程模式**：分区并行（线程池/async），支持 message shuffle（进程内 channel）与 barrier；
  2) **Distributed 模式**：Driver/Worker 进程，基于 RPC（gRPC/自定义协议）进行分区分配、shuffle、superstep barrier、心跳与重试。
- **State（新增 geaflow-state 或并入 runtime）**：提供 `StateBackend` 抽象：InMemory/RocksDB；支持 checkpoint（全量/增量）与恢复。
- **IO（新增 geaflow-io）**：文件/Socket 连接器（优先与现有脚本/示例类似的输入形态），并支持结果落盘。

## 4. 具体实施任务（按可验证增量交付）

### 4.1 API 稳定化与 Java 对齐
- 引入 `Environment / Pipeline / PipelineTaskContext` 结构，使 Rust 端可用类似 Java 的“提交作业”方式（对齐 Java 示例链路：见 [PageRank.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/main/java/org/apache/geaflow/example/graph/statical/compute/pagerank/PageRank.java#L73-L110)）。
- 拆分当前 `PGraphWindow.compute()` 的职责：
  - `PGraphWindow::compute(algo) -> PGraphCompute`（构建计算节点）
  - `PGraphCompute::with_parallelism(n) -> Self`
  - `PGraphCompute::get_vertices()/sink()`
- 明确 `Context` 的只读/可变借用边界，避免算法写法被 Rust 借用规则限制（例如 edge 遍历与 send_message 的并发借用问题）。

### 4.2 分区并行执行（Local 模式）
- 实现 `Partitioner`：按 vertex_id hash 分区；边按 src 分区或双向分发策略可配置。
- 实现 superstep 引擎：
  - 每步并行执行分区内顶点 compute；
  - 分区间消息聚合与 barrier；
  - 可配置终止：收敛（无消息）/最大迭代（与 Java 算法的 iterations 对齐）。
- 支持可选的 `Combiner`（类似 Pregel message combine）减少网络/内存。

### 4.3 生产级状态后端（RocksDB）
- 定义 `StateBackend` trait：`get_vertex/put_vertex/scan_adj/put_edge` 等。
- 实现 `RocksDbStateBackend`：
  - 顶点表、边表（按分区/窗口隔离）；
  - 序列化策略（serde+bincode/自定义编码）；
  - 支持批量写（WriteBatch）提升吞吐。

### 4.4 Checkpoint / Recovery（容错语义）
- 引入 `CheckpointCoordinator`（Driver 或 Local 引擎内）：
  - 定期生成 checkpoint；
  - barrier 对齐；
  - snapshot 元数据管理。
- 在 RocksDB 后端实现：
  - 全量 checkpoint（先做）；
  - 增量 checkpoint（基于 SST/sequence 或变更日志，后续优化）。
- 恢复流程：从最近 checkpoint 恢复 state，重放必要的消息/输入以保证一致性。

### 4.5 分布式模式（Driver/Worker）
- 新增 `geaflow-server`（或 runtime/bin）：
  - **Driver**：分区分配、superstep 调度、barrier、checkpoint 协调、失败探测。
  - **Worker**：分区执行、状态读写、消息收发。
- RPC 协议：
  - heartbeats / register / assign-partitions
  - send-messages / fetch-messages
  - superstep-barrier / checkpoint-prepare/commit
- 先保证“多 worker 能跑 PageRank/WCC 并输出一致”，再做性能与稳定性强化。

### 4.6 IO 连接器与作业提交方式
- 提供 `geaflow-submit` CLI：
  - `--mode local|distributed`、`--config`、`--job`（选择内置示例：pagerank/wcc/sssp）
  - 输入：边/点文件（csv/tsv），与 Java 示例的资源文件结构兼容
  - 输出：stdout/文件

### 4.7 可观测性与运维能力
- 统一日志：结构化日志（tracing + env filter），避免在 hot path 大量格式化。
- 指标：Prometheus 指标（superstep 时长、消息量、状态读写、checkpoint 时间、失败次数）。
- 健康检查：Driver/Worker 提供 health endpoint。

### 4.8 测试与一致性回归（与 Java 保持一致）
- **算法移植**：将 Java 示例中的 PageRank 与 WCC 算法移植为 Rust `VertexCentricComputeFunction` 版本（逻辑对照 [PageRank.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/main/java/org/apache/geaflow/example/graph/statical/compute/pagerank/PageRank.java#L120-L165) 与 [WeakConnectedComponents.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/main/java/org/apache/geaflow/example/graph/statical/compute/weakconnectedcomponents/WeakConnectedComponents.java#L117-L167)）。
- **测试对齐**：新增 Rust 端 `PageRankTest`/`WCCTest` 形式的集成测试（对齐 [PageRankTest.java](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-examples/src/test/java/org/apache/geaflow/example/graph/PageRankTest.java#L63-L77) 的“本地 environment 执行 + 校验结果/成功状态”）。
- **容错测试**：故障注入（kill worker / 超时）验证 checkpoint 恢复正确性。
- **基准测试**：criterion 基准覆盖消息/状态/迭代性能，作为回归门槛。

### 4.9 文档与发布
- 完整 README（中文）：
  - 本地模式跑 PageRank/WCC/SSSP
  - 分布式模式启动 Driver/Worker
  - 配置说明、输入数据格式
- CI：增加 Rust fmt/clippy/test 工作流。

## 5. 验收标准（生产级定义的可验证项）
- `cargo test` 全通过（包含 PageRank/WCC/SSSP + checkpoint 恢复测试）。
- local 多线程模式下 `parallelism` 生效，结果与单线程一致。
- distributed 模式下（>=2 worker）可跑 PageRank/WCC，且支持 worker 失败后自动恢复并完成。
- RocksDB 状态后端可用；checkpoint 可恢复；指标与日志可观测。
- `geaflow-submit` CLI 可用，文档可复现。

如果同意该计划，我将按上述任务顺序开始实现：先做 API 对齐 + local 并行引擎 + PageRank/WCC 一致性测试，再引入 RocksDB 与 checkpoint，最后上分布式 Driver/Worker 与容错。