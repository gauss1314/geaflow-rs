## 目标与输出
- 产出一份“原有 GeaFlow（Java）能力全景/需求设计文档”，以仓库现有 docs 与关键代码为准。
- 产出一份“Java→Rust 功能对照矩阵（Gap Matrix）”，逐项标注：已实现/部分实现/未实现/不在范围。
- 按对照矩阵把 Rust 侧缺失能力补齐到“可替代”的验收标准。
- 在满足验收标准后，删除 Java 代码与其构建/CI，并把仓库重构为 Rust-only（或仅保留非 Java 的资源/文档）。

## 现状复核（只读结论）
- Java 侧是完整产品栈：引擎（Client/Master/Driver/Container/Worker + CycleScheduler + 状态/容错 + DSL + 插件/Connector + 多部署形态）+ Console（研发/发布/任务管理）+ K8S Operator + Dashboard + MCP/AI 服务等。
- Rust 侧当前明确定位为“生产级核心能力子集”：Vertex-Centric（BSP）图计算（local/distributed）、RocksDB 状态、checkpoint/恢复、WCC/PageRank 等。
- 因此：若“删除全部 Java”，必须先明确 Rust 要承接的范围（仅引擎子集 vs 全产品栈）。

## 需求设计文档补充（将新增/补齐的文档结构）
1. **仓库能力全景（As-Is）**
   - 从根 README/概念文档（framework/state/dsl/console）与关键 Java 入口（K8S runner、console submitter 等）整理：用户侧能力、运行时组件、调度模型、状态与容错、部署形态、运维面。
2. **Rust 目标能力（To-Be）**
   - 明确 Rust-only 版本的：
     - 运行时组件边界（master/driver/worker/submit）、协议与作业生命周期
     - 执行计划与调度（cycle/superstep、batch/stream/window 的支持程度）
     - 状态模型（graph/key state、版本化、pushdown/索引、持久化存储）
     - 容错语义（Exactly-Once 的定义、checkpoint 对齐点、恢复流程）
     - 部署形态（local/docker-compose/k8s/operator 是否需要）
     - 可观测性（metrics/log/health/job/workers API）
3. **验收标准**
   - 每个能力点对应：可运行样例/集成测试/故障注入测试/回归用例。

## Java→Rust 对照矩阵（Gap Matrix）制作方法
- 按能力域拆分条目，并为每条给出“对应代码位置/文档依据/验收测试”。建议能力域：
  - API 层（Pipeline/Stream/Graph/Window）
  - 执行计划（PipelineGraph/ExecutionGraph/Group/Cycle）
  - 调度与运行时（事件驱动、任务生命周期、RPC/Shuffle/流控）
  - 状态（Graph/Key State、动态版本、Checkpoint/Recover、外部持久化）
  - 部署（local/docker/k8s/operator）
  - 运维面（dashboard/metrics/REST）
  - DSL（SQL+GQL 编译器链路、优化、执行）
  - Console/平台能力（发布、任务、监控、元数据）

## Rust 查漏补缺（按优先级推进）
1. **先补齐“引擎可替代”的最小集合（推荐默认）**
   - 以 Rust 现有 BSP 图计算为核心，补齐：
     - 更完整的作业/执行计划模型（替代 Java PipelineGraph/ExecutionGraph 的最小等价）
     - 更严格的 checkpoint 一致性与恢复协议（含 source offset/driver inbox 元数据对齐）
     - 可插拔 shuffle/partition/流控与背压策略
     - 运维 API（jobs/workers/metrics）与故障注入回归集
2. **若目标是“Rust 全量替代 Java 产品栈”**（工作量巨大，需分阶段）
   - 逐步实现：DSL 编译器链路、connector 插件体系、K8S operator、console 平台能力等；在此之前不建议删 Java。

## 删除 Java 代码（在验收达标后执行）
- 删除目录：`/geaflow`、`/geaflow-console`、`/geaflow-kubernetes-operator`、`/geaflow-mcp`、`/geaflow-ai` 以及根 `pom.xml`、Java 相关 build 脚本与 CI（如 `ci-jdk11.yml`、`ci.yml` 中 Java 部分）。
- 同步更新：根 README/QuickStart/文档索引，把描述收敛到 Rust 实际支持能力；保留 docs 中与 Rust 目标一致的概念与架构说明，删除/归档纯 Java/DSL/Console/Operator 章节。
- 验证：Rust workspace 全量 `fmt/clippy/test` 通过；关键集成测试（分布式、checkpoint、故障注入）通过；docker-compose 启动与作业提交链路可复现。

## 需要你确认的关键决策（在你同意计划后我会按选择执行）
- **删除 Java 的前提范围**：
  1) 仅保留 Rust“BSP 图计算引擎子集”，接受丢弃 DSL/Console/Operator/AI 等 Java 产品能力；或
  2) Rust 要全量替代 Java 产品栈（会显著拉长补齐清单，删除动作放到最后阶段）。
