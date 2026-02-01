# GeaFlow（Rust）To-Be 需求与验收（Rust-only 引擎子集）

本文定义“Rust-only 版本”在本仓库中的目标能力边界、系统设计要点与验收标准，并作为后续 Java→Rust 功能对照与补齐的依据。

## 1. 范围声明（非常重要）

### 1.1 本次 Rust-only 的目标

以 **Vertex-Centric（BSP/Superstep）图计算引擎** 为中心，提供可独立运行的 Rust 版本：
- 本地执行（单机多线程并行）
- 分布式执行（TCP RPC 的 driver/worker + 可选 master）
- RocksDB 状态后端
- Checkpoint/恢复（按超步边界对齐）
- 基础可观测性与运维 API（健康检查、列出作业/worker）
- 典型算法（WCC/PageRank/SSSP）的可回归集成测试

### 1.2 明确不在范围（本次会被移除或不再承诺）

以下 Java 产品能力本次 **不在 Rust-only 交付范围**（因此删除 Java 代码后将不再提供）：
- SQL+GQL DSL 编译器链路（Calcite 扩展、优化器、物理计划生成）
- Console 平台（白屏建模、发布构建、任务管理、多租户等）
- K8S Operator（CRD/Controller 调谐）
- Dashboard Web UI（如 driver/container/cycle 视图）
- AI/Memory、MCP 等周边服务
- 多存储、多模型 State 全家桶（Redis/多级索引/pushdown/动态版本化图等）

如需未来逐步补回，上述能力将以“阶段 2+”的方式另行立项，不作为“删除 Java”的阻塞条件。

## 2. 核心概念与接口（用户视角）

### 2.1 作业模型

Rust 侧作业提交以 `JobSpec` 为权威输入（序列化为 JSON）：  
- 输入：顶点/边文件（CSV，无表头）  
- 算法：WCC/PageRank/SSSP（按需扩展）  
- 执行模式：local / distributed  
- 容错：checkpoint 是否启用、间隔、目录

对应实现：  
- [job_spec.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/plan/job_spec.rs)
- [execution_plan.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/plan/execution_plan.rs)

### 2.2 运行时组件（Rust）

- **Master（可选）**：worker 注册/心跳、存活列表查询  
- **Driver（推荐）**：接收 JobSpec、分发到 worker、驱动超步调度、维护作业状态（jobs 列表）  
- **Worker（必须）**：加载分片图、执行超步计算、持有本地 RocksDB 状态、对齐 checkpoint/恢复  
- **Submit（CLI）**：生成/校验 JobSpec，支持 dry-run，提交到 driver 或直连 worker（兼容模式）

对应实现：  
- [geaflow-master.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/bin/geaflow-master.rs)  
- [geaflow-driver.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/bin/geaflow-driver.rs)  
- [geaflow-worker.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/bin/geaflow-worker.rs)  
- [geaflow-submit.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/bin/geaflow-submit.rs)

## 3. 执行语义（系统视角）

### 3.1 调度模型（Cycle/Superstep 的 Rust 等价）

Rust 采用“超步循环”作为最小调度闭环，并抽象为调度器状态机：
- Driver 每轮向 worker 下发 inbox（按 batch 分片传输）
- Worker 执行一次 superstep，产生 outbox
- Driver 收集 outbox 并 shuffle 路由成下一轮 inboxes

对应实现：  
- Driver 超步轮转：[driver.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/distributed/driver.rs)  
- 调度状态机：[cycle_scheduler.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/scheduler/cycle_scheduler.rs)  
- Shuffle 抽象：[shuffle/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/shuffle/mod.rs)

### 3.2 状态与一致性（Exactly-Once 的当前定义）

由于当前输入源为文件（无 offset 概念），Rust-only 的一致性语义定义为：
- **对计算状态**：在超步边界进行 checkpoint，对齐点为“driver 已收齐本轮 worker 结果并准备进入下一轮”  
- **对 driver 元数据**：checkpoint 持久化 inboxes + meta（包含 iteration、checkpoint_dir 等）  
- **对 worker**：RocksDB checkpoint/snapshot 到共享目录；恢复时从 snapshot 打开并回到指定迭代位置

对应实现与测试：  
- [checkpoint_meta.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/state/checkpoint_meta.rs)  
- [distributed_checkpoint_recovery_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/distributed_checkpoint_recovery_test.rs)

### 3.3 故障语义（当前阶段）

当前阶段的故障策略：
- worker 连接中断/发送失败：driver 立即失败（fail-fast）
- master 可选，仅用于 worker 存活与运维查询；driver/worker 不依赖 master 的一致性协议

对应测试：  
- [fault_injection_worker_crash_test.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/tests/fault_injection_worker_crash_test.rs)

## 4. 运维与可观测性

### 4.1 HTTP 运维面

- master：`/healthz`、`/workers`  
- driver：`/healthz`、`/jobs`

对应实现：  
- [http/mod.rs](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/geaflow-runtime/src/http/mod.rs)

### 4.2 Metrics

- worker 暴露 Prometheus metrics（如果开启端口）

## 5. 部署形态（Rust-only）

- 本地：cargo run / cargo test
- 轻量分布式：多进程 + localhost
- 开发用一键启动：docker-compose（master/driver/workers）

对应部署文件：  
- [docker-compose.yml](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-rust/deploy/docker-compose.yml)

## 6. 验收标准（删除 Java 的门槛）

### 6.1 功能验收（必须全部满足）

- local 模式：WCC/PageRank/SSSP 可跑通并输出结果
- distributed 模式：driver/worker 模式可跑通（含 batch inbox 分片）
- checkpoint/恢复：分布式恢复测试通过（从 latest meta 恢复继续执行）
- 故障注入：worker crash 时 driver fail-fast，且不会卡死
- 运维面：master/driver healthz 与列表接口可用

### 6.2 工程验收（必须全部满足）

- `cargo fmt -- --check` 通过
- `cargo clippy --all-targets -- -D warnings` 通过
- `cargo test` 全量通过

### 6.3 文档验收（必须全部满足）

- 根 README 与 Quick Start 不再要求 JDK/Maven/Java DSL 提交链路
- Rust README 明确能力边界与替代路径（不暗示已具备 DSL/Console）

