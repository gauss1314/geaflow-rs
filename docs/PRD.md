# GeaFlow Rust Runtime — 产品需求文档（PRD）

本文档是对本仓库 Rust-only 版本的产品需求总结，描述目标用户、使用场景、核心能力边界、功能需求、非功能需求、验收标准与演进方向。

## 1. 产品背景
GeaFlow Rust Runtime 是对 GeaFlow（Java）核心能力的 Rust 重构实现，聚焦“生产级核心能力子集”：**Vertex-Centric（BSP/Superstep）图计算引擎**。

本仓库的定位不是完整产品栈替代（不包含 DSL/Console/Operator/Dashboard 等），而是提供可独立运行、可部署、可回归验证的 Rust runtime。

## 2. 产品目标与非目标
### 2.1 目标
- 提供可独立运行的 BSP 图计算引擎（Local 与 Distributed）
- 提供稳定的作业描述与提交流程（JobSpec/CLI）
- 提供可恢复的状态计算能力（RocksDB + checkpoint/恢复）
- 提供最小可用的运维与可观测面（health/list/metrics）
- 提供可回归验证的算法与数据集测试（WCC/PR/SSSP + Graph500 端到端验证）

### 2.2 非目标（明确不做）
- SQL + GQL DSL 编译与优化链路
- Console 研发平台与任务管理系统
- Kubernetes Operator / Dashboard UI
- AI/MCP 等周边服务
- Java 版多模型、多存储 State 体系（Redis/外部持久化/索引 pushdown 等）

## 3. 目标用户与使用场景
### 3.1 目标用户
- 图算法工程师：需要快速开发与验证 WCC/PageRank/SSSP 等 BSP 算法
- 数据工程师：需要在本机或小规模集群跑通图计算作业并导出结果
- 平台/基础设施工程师：需要一套可部署、可观测、可恢复的 Rust 图计算 runtime 骨架

### 3.2 使用场景
- 单机并行跑通算法与回归测试（Local）
- 多进程/多 worker 的分布式执行验证（Distributed）
- Graph500 数据集端到端验证（代码方式或 HTTP/curl 方式），用于性能/正确性回归

## 4. 产品范围（Rust-only 功能清单）
### 4.1 执行模式
- Local：单机多线程并行执行
- Distributed：TCP RPC driver/worker；master 可选

### 4.2 状态与容错
- RocksDB 状态后端
- 超步边界对齐 checkpoint/恢复（文件源一致性语义下的可恢复计算）
- 故障语义（当前阶段）：fail-fast（worker 异常时 driver 立即失败）

### 4.3 运维与可观测
- Master：HTTP `GET /healthz`、`GET /workers`
- Driver：HTTP `GET /healthz`、`GET /jobs`
- Worker：可选 Prometheus metrics（若开启端口）

### 4.4 算法与回归
- WCC / PageRank / SSSP
- 覆盖 Local/Distributed 的集成测试与容错测试（见 `geaflow-rust/geaflow-runtime/tests`）

### 4.5 Graph500 端到端验证
提供两条验证路径（均包含“加载数据 → 计算 → 与真值比对”）：
- 代码方式：`geaflow-graph500` runner 直接执行并输出验证报告
- 接口方式：`geaflow-graph500 serve` 启动 HTTP 服务，通过 curl 调用完成同样验证

## 5. 关键用户流程（User Journey）
### 5.1 Local（单机）
1) 用户准备 CSV 顶点/边文件\n2) 使用 `geaflow-submit --mode local` 选择算法并执行\n3) 输出结果（stdout 或落盘）

### 5.2 Distributed（分布式）
1) 启动 master（可选）\n2) 启动 N 个 worker（配置独立 state_dir）\n3) 启动 driver（连接 master 或直接配置 workers）\n4) 使用 `geaflow-submit --mode distributed --driver ...` 提交 JobSpec\n5) 通过 driver 的 `/jobs` 查看作业列表（只读）\n6) 作业完成后获取输出结果

### 5.3 Graph500（回归验证）
路径 A（runner）：解压数据集 → `run-wcc/run-pagerank/run-all` → 读取真值逐点比对\n路径 B（HTTP）：启动 `serve` → 注册数据集路径（.properties/.v）→ 提交验证任务（WCC+PR）→ 查询 job 结果与比对报告

## 6. 功能需求（按模块）
### 6.1 作业模型（JobSpec）
需求：
- 能描述图输入、算法参数、执行模式、checkpoint 策略
- 可序列化并通过 CLI 生成/校验
- Driver 以 JobSpec 为权威输入驱动执行

验收点：
- CLI 支持 dry-run（输出/校验 JobSpec）
- Local 与 Distributed 走同一套 JobSpec/Plan 主链路

### 6.2 分布式组件
需求：
- Master（可选）：提供 worker 注册/心跳、存活列表查询
- Driver：接收 JobSpec、调度超步、维护 job 状态列表
- Worker：加载图分片、执行超步、持有 RocksDB 状态

验收点：
- 多 worker 分布式可跑通 WCC/PageRank/SSSP
- Driver/Worker 协议可稳定传输 batch 分片（避免单帧过大）

### 6.3 状态与容错（Checkpoint/恢复）
需求：
- 在超步边界发起 checkpoint，对齐并落盘元数据与 RocksDB snapshot
- 支持从最新 checkpoint 恢复并继续执行（以测试为准）

验收点：
- `distributed_checkpoint_recovery_test` 通过

### 6.4 运维接口
需求：
- 健康检查：`/healthz`
- 列表查询：master `/workers`、driver `/jobs`
- 轻量实现，避免引入重型 HTTP 框架

验收点：
- curl 可直接调用并返回可解析 JSON（列表端点）

### 6.5 Graph500 验证接口
需求：
- 提供无需上传大文件的验证 API（注册本机路径）
- 支持异步提交验证任务，查询 job 状态与最终报告

验收点：
- WCC：mismatches=0/unexpected=0/missing=0
- PageRank：unexpected=0/missing=0/max_abs_diff<=1e-6

## 7. 非功能需求
- 正确性：回归测试 + Graph500 真值比对作为硬门槛
- 可复现：README/PRD 中的命令可复制粘贴复现
- 可维护：模块边界清晰（plan/scheduler/distributed/state/io/http）
- 性能（阶段性）：支持大图批量加载与增量写入；避免 driver 内存随消息量线性增长（路线图）
- 安全性：不在日志/接口中泄露敏感信息（当前默认本机开发环境）

## 8. 约束与假设
- 输入源以文件为主（无 offset 概念），一致性语义以“计算状态”对齐为主
- 当前阶段故障策略为 fail-fast，HA/自动重试与全局 failover 属于后续演进
- HTTP 运维面与 Graph500 API 主要用于开发/回归，不作为公网服务默认暴露

## 9. 验收标准
### 9.1 功能验收
- Local：WCC/PageRank/SSSP 可跑通\n- Distributed：driver/worker 可跑通\n- Checkpoint/恢复：恢复测试通过\n- 故障注入：worker crash 时 fail-fast 且不卡死\n- 运维面：healthz 与列表接口可用\n- Graph500：WCC/PR 真值比对通过

### 9.2 工程验收
- `cargo fmt -- --check`\n- `cargo clippy --all-targets -- -D warnings`\n- `cargo test`

## 10. 风险与演进方向（高层）
### 10.1 风险
- 大图下 state/edge 布局与读写放大控制\n- driver 聚合 shuffle 的内存压力\n- checkpoint 元数据一致性与恢复语义扩展（当引入更多 source 形态时）

### 10.2 演进方向
- Shuffle/流控与背压策略增强（从 driver 聚合逐步演进到 worker 直连）\n- 调度器状态机化（Init/Load/Barrier/Commit/Recover）\n- 更丰富的运维 API（作业详情、日志、指标聚合）\n- Stream/Window 能力从“骨架”扩展为可用子集

