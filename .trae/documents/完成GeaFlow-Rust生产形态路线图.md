## 目标与范围
- 目标：把当前 Rust 版从“可运行的 BSP 图计算内核 + demo 分布式”升级为“可长期演进的生产形态骨架”，对齐 Java 版核心概念：作业描述（PipelineGraph/ExecutionGraph）、调度器（Cycle/Superstep）、运行时组件（Master/Driver/Worker/Container）、容错（全局一致 checkpoint + 恢复）、数据交换（Shuffle）、可观测与运维面。
- 范围约束：先把 Graph/BSP 跑稳、跑大、可恢复；再把 Stream/Batch 的算子与 window/cycle 语义补齐到“可用子集”；Console/REST/K8S 作为最后的运维面补齐。

## 现状基线（已具备）
- 本地分区并行 BSP：PartitionedGraph。
- 分布式 MVP：Driver 直连 Worker，driver 负责消息路由。
- RocksDB 状态 + Checkpoint：worker 本地 checkpoint 能力。
- 基础观测：tracing + Prometheus exporter（worker 超步埋点）。

## Phase 1：生产级作业模型（JobSpec / Plan）与提交链路
- 引入可序列化 JobSpec（替代 CLI 硬编码）：source/sink、graph schema、算法、并行度、迭代/窗口策略、checkpoint 策略。
- 实现 ExecutionPlan（最小 ExecutionGraph）：把 JobSpec 转换为可执行的“阶段/循环/分区任务”描述。
- 改造提交流程：geaflow-submit 生成/校验/提交 JobSpec；driver 以 JobSpec 为输入驱动执行。
- 验收：
  - 支持 `--dry-run` 输出 JobSpec/Plan。
  - 分布式执行不依赖本地 CLI 逻辑分支（统一走 plan）。

## Phase 2：运行时组件闭环（Master/Container/Driver/Worker）
- 新增 Master（控制面最小实现）：worker 注册、心跳、slot/资源视图、作业元数据、driver/worker 发现。
- Worker 改造：
  - 支持多连接、多作业隔离（JobId/ExecutionId 命名空间）。
  - 任务生命周期：Init/Run/Cancel/Shutdown。
- Driver 改造：
  - 从“直连地址列表”改为从 Master 获取 worker 分配与拓扑。
  - 超时、重试、幂等请求、协议版本。
- 验收：
  - 一个 master + N worker，可连续提交多个作业。
  - worker 异常退出能被 master 识别并触发 job failover（先 fail-fast）。

## Phase 3：调度器升级（Cycle/Superstep 状态机）
- 把 driver 的 while-loop 升级为事件驱动状态机：InitJob/LoadGraph/StartSuperstep(i)/Barrier/Commit/Finish/Fail/Recover。
- 统一执行器（executor）：并发、取消、限流（为 shuffle 与恢复做基础）。
- 验收：
  - 支持暂停/继续、取消作业。
  - 支持在指定 iteration 注入故障并进入 Fail/Recover 流程。

## Phase 4：全局一致 Checkpoint 与恢复协议（Exactly-once 的基础闭环）
- Checkpoint 协调：driver/scheduler 发起 checkpoint_id，所有分区 worker ack 后 commit；失败回滚/重试。
- 元数据：checkpoint catalog（checkpoint_id、iteration、worker/partition 映射、路径、校验信息），先落盘文件，后续可接入外部 KV。
- 恢复：
  - worker 支持从 checkpoint 打开 state；driver 从 catalog 选取最新一致点恢复并续跑。
- 验收：
  - “写入→checkpoint→再写入→故障→从 checkpoint 恢复继续”端到端测试通过。

## Phase 5：数据面演进（Shuffle：从 driver 聚合到 worker 直连）
- 抽象 Shuffle：partitioner + transport + exchange。
- 先实现最小可用：worker-to-worker 点对点发送分区消息；driver 只发控制事件与 barrier。
- 引入背压与流控（至少：每连接队列上限 + 发送窗口）。
- 验收：
  - driver 内存占用不再随消息量线性增长。
  - 大消息/高迭代数下稳定运行，无明显长尾。

## Phase 6：Stream/Batch 能力补齐（生产形态的“算子 + Window/Cycle”骨架）
- Stream API 落地：source → transform/keyBy/window → sink 的最小执行器。
- Window/Cycle：把流式 window 触发与图/迭代调度统一纳入 scheduler（与 Java 的 cycle 概念对齐的子集）。
- 验收：
  - 支持 socket/file source 触发 window 计算并输出到 sink。
  - window 与 checkpoint 联动（至少一次/精确一次可切换策略）。

## Phase 7：运维面与工程化（Console/REST/部署/CI）
- 控制面 API：REST（job submit/status/logs/metrics endpoints），基础鉴权（可选）。
- 部署：docker compose（master/worker），再扩展到 Kubernetes（state dir 与 checkpoint dir 持久化）。
- CI：Rust fmt/clippy/test、依赖锁定策略、分模块缓存。
- 验收：
  - 一键启动集群并提交作业；
  - 通过 REST 查询作业状态与指标；
  - CI 稳定复现构建与测试。

## 关键设计选择（默认方案）
- RPC：短期沿用现有 TCP+LengthDelimited+bincode，加入协议版本/心跳/超时/幂等；中期如需生态与跨语言再切换到 gRPC/tonic。
- 元数据存储：先本地文件（单机 master），后续抽象接口可替换为外部 KV。
- 状态存储：继续 RocksDB，但需要把边存储从 `src -> Vec<Edge>` 演进为 key 前缀布局（避免大 value 与读放大），并支持按分区/活跃点迭代。

## 测试与验收体系
- 单元测试：算法、序列化、状态布局、分区器。
- 集成测试：
  - 本地并行（WCC/PageRank/SSSP）
  - 分布式端到端（含故障注入 + 恢复）
  - Shuffle 压测（小规模即可，关注正确性与资源曲线）
- 可观测：每阶段补齐关键指标（吞吐、延迟、checkpoint 时长、恢复耗时、消息队列深度）。

如果确认这份路线图，我将从 Phase 1 开始实现：先落 JobSpec/ExecutionPlan 与提交协议，把当前 CLI/driver/worker 串到统一的“作业描述→计划→调度→执行”主链路上，并配套端到端测试作为回归基线。