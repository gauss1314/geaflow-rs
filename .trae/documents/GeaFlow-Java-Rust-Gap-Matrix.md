# GeaFlow Java→Rust 功能对照矩阵（Gap Matrix）

说明：
- ✅ 已实现（Rust-only 范围内，且有测试/可复现路径）
- 🟡 部分实现（可跑通但能力/语义不完整，需补齐）
- ❌ 未实现（在 Rust-only 范围内但缺失）
- — 不在 Rust-only 范围（删除 Java 后不再提供）

Rust-only 范围与验收门槛见：[GeaFlow-Rust-ToBe-需求与验收.md](file:///Users/gauss/workspace/github_project/geaflow-rs/.trae/documents/GeaFlow-Rust-ToBe-需求与验收.md)

## 1. 引擎核心（Rust-only 必须交付）

| 能力点 | Java As-Is（参考） | Rust 现状 | 结论 | Rust 代码/测试落点 |
|---|---|---|---|---|
| 作业描述模型（JobSpec/Plan） | PipelineGraph/ExecutionGraph 等（概念） | 已有 JobSpec + ExecutionPlan | ✅ | [job_spec.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/plan/job_spec.rs)、[execution_plan.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/plan/execution_plan.rs) |
| 提交链路（CLI 提交） | Console / K8S Client | CLI submit 支持 dry-run、提交到 driver 或直连 worker | ✅ | [geaflow-submit.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/bin/geaflow-submit.rs)、[driver_submit_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/driver_submit_test.rs) |
| 分布式运行时组件 | Client/Master/Driver/Container/Worker | Master/Driver/Worker（TCP RPC） | ✅（子集） | [master.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/master.rs)、[driver_service.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/driver_service.rs)、[worker.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/worker.rs) |
| 超步调度（Cycle/Superstep） | CycleScheduler（事件驱动） | CycleScheduler（超步状态机） | ✅（子集） | [cycle_scheduler.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/scheduler/cycle_scheduler.rs) |
| Shuffle/路由 | Shuffle 模块（多策略） | 抽象 + 默认 driver 路由 | 🟡 | [shuffle/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/shuffle/mod.rs) |
| 流控/背压 | 运行时具备（概念） | 仅 batch 分片（避免单帧过大），无系统级背压 | 🟡 | [protocol.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/protocol.rs)、[driver.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/driver.rs) |
| 状态后端 | 多模型 + 多存储（RocksDB/Redis/…） | RocksDB Graph State（聚焦 BSP） | ✅（子集） | [state/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/state/mod.rs) |
| Checkpoint | Exactly-Once + 多介质持久化 | 超步边界对齐 checkpoint/恢复（文件源语义） | ✅（子集） | [checkpoint_meta.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/state/checkpoint_meta.rs)、[distributed_checkpoint_recovery_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/distributed_checkpoint_recovery_test.rs) |
| 故障语义 | FailOver（组件/作业恢复） | worker crash fail-fast（当前阶段） | ✅（当前定义） | [fault_injection_worker_crash_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/fault_injection_worker_crash_test.rs) |
| 运维面（健康/列表） | Dashboard/metrics/HA | master/driver 轻量 HTTP、worker metrics | ✅（子集） | [http/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/http/mod.rs) |
| 本地并行执行 | Local 模式 | PartitionedGraph + MemGraph | ✅ | [partitioned_graph.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/graph/partitioned_graph.rs)、[mem_graph.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/graph/mem_graph.rs) |
| 算法库 | WCC/PageRank 等 | WCC/PageRank/SSSP | ✅（子集） | [algorithms](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/algorithms/) |

## 2. API/Stream/Window（Rust-only 可选骨架）

| 能力点 | Java As-Is（参考） | Rust 现状 | 结论 | Rust 代码/测试落点 |
|---|---|---|---|---|
| Stream API（map/filter/…） | 统一流批图 API | LocalStream（最小实现） | 🟡 | [stream/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/stream/mod.rs)、[local_stream_window_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/local_stream_window_test.rs) |
| Window 抽象 | 无界/有界 window | Count tumbling window | 🟡 | 同上 |
| Graph API（append/snapshot/compute/traversal） | 静态/动态图 API | 仅 vertex-centric compute 接口（子集） | 🟡 | [geaflow-api/graph.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-api/src/graph.rs) |

## 3. 不在 Rust-only 范围（删除 Java 后不再提供）

| 能力域 | Java As-Is（参考） | Rust 现状 | 结论 |
|---|---|---|---|
| DSL（SQL+GQL） | Calcite 扩展/优化/执行 | 无 | — |
| Console 平台 | 发布/任务/监控/元数据 | 无 | — |
| K8S Operator | CRD/Controller | 无 | — |
| Dashboard Web | UI/Runtime 视图 | 无 | — |
| AI/Memory / MCP | Solon 服务 | 无 | — |
| 多存储 State 体系 | Redis/HDFS/OSS/S3/索引/pushdown | 无（仅 RocksDB 子集） | — |

## 4. Gap 结论（驱动后续补齐与删除动作）

在“Rust-only 引擎子集”范围内，当前主要缺口集中在：\n
- Shuffle/流控仍偏简化（仅默认路由与 batch 分片），缺少更细粒度的背压与资源治理\n
- Stream/Window/Graph API 仍是骨架，未达到 Java “统一流批图” 的功能面\n

这些缺口将以 Rust-only 范围为准逐项补齐；超出范围的 DSL/Console/Operator 等在删除 Java 后将不再提供，因此需要同步更新根文档与 Quick Start，避免功能误导。

