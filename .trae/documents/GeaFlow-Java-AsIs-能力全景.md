# GeaFlow（Java）As-Is 能力全景（基于仓库现有实现）

本文面向“Rust 重构/替代”场景，对当前仓库中 **Java 体系** 的实际能力与代码落点做一次结构化复核，作为后续需求设计与差距对照的输入。

## 1. 总体定位与用户侧能力

根仓库 README 描述的用户侧能力包括：
- 分布式实时图计算
- 图表混合处理（SQL+GQL）
- 统一流/批/图计算
- 万亿级图原生存储
- 交互式图分析
- 高可用与 Exactly Once
- 高阶 API 算子开发
- UDF/图算法/Connector 插件
- 一站式图研发平台（Console）
- 云原生部署（K8S/Operator/Dashboard）

参考：[README_cn.md](file:///Users/gauss/workspace/github_project/geaflow-rs/README_cn.md)

## 2. 代码结构（按子系统）

仓库根是 Maven 聚合工程，主要子系统如下：
- **引擎主体（Java）**：`geaflow/`（核心引擎、DSL、状态、部署、示例、dashboard 等聚合）
  - 关键概念文档：Framework / State / DSL
    - [framework_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/3.framework_principle.md)
    - [state_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/4.state_principle.md)
    - [dsl_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/2.dsl_principle.md)
- **Console（Java + Web）**：`geaflow-console/`（Spring Boot 控制台 + 前端）
  - Console 原理文档：[console_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/5.console_principle.md)
- **K8S Operator（Java）**：`geaflow-kubernetes-operator/`（CRD/Controller 管理作业）
- **MCP Server（Java）**：`geaflow-mcp/`（Solon + SSE）
- **AI/Memory Server（Java）**：`geaflow-ai/`（Solon）
- **Rust 子工程（Rust）**：`geaflow-rust/`（目前为引擎核心能力子集）

## 3. 引擎架构（Java Framework）复核

### 3.1 组件与职责（Client/Master/Driver/Container/Worker）

Framework 文档描述了运行时组件与数据流转：
- Client 负责提交 Pipeline 给 Driver
- Master 负责 worker 心跳/存活、集群协调
- Driver 构建执行计划、分配资源、驱动调度
- Container/Worker 执行具体算子与图迭代，Shuffle 交换数据
- 支持 FailOver：组件 context 持久化、作业状态回滚恢复

参考：[framework_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/3.framework_principle.md#L9-L79)

### 3.2 执行计划模型（PipelineGraph / ExecutionGraph / Group / Cycle）

Framework 文档给出从高阶 API 到执行计划与调度单元的转换路径：
- PipelineGraph：逻辑 DAG（算子、并发度、shuffle 规则等）
- ExecutionGraph：物理计划与二级嵌套 group（支持流水线子图）
- Cycle：调度基本单元（事件驱动触发 head、tail 回传事件闭环）

参考：[framework_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/3.framework_principle.md#L18-L43)

### 3.3 调度模型（事件驱动状态机）

调度以 CycleScheduler 为核心，事件触发 + 状态机管理生命周期，并通过 TaskRunner / Dispatcher 将事件分发到 Worker 侧执行。

参考：[framework_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/3.framework_principle.md#L44-L65)

## 4. State（Java State System）复核

State 文档体现出 Java 侧状态系统是“可插拔、多模型、多存储”的完整体系：
- **State API**：KV 与图存储 API（点/边/VE、CRUD、Query）
- **执行层**：KeyGroup 分片/扩缩容、Accessor/StateOperator（finish/archive/compact/recover）
- **Store 层**：RocksDB/Redis 等多存储映射（KV/Graph/KMap 等）
- **持久化层**：HDFS/OSS/S3 等外部存储
- **类型**：GraphState（Static/Dynamic 版本化）+ KeyState（Value/List/Map）
- **容错**：checkpoint/恢复与 source offset 对齐

参考：[state_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/4.state_principle.md#L16-L99)

## 5. DSL（SQL+GQL）复核

DSL 文档表明 Java 侧提供完整 DSL 编译器链路：
- 语言：SQL + GQL 融合语法
- Parser/Validator：基于 Calcite 扩展
- IR：Logical RelNode（图上的逻辑表示）
- Optimizer：RBO（未来 CBO）
- Codegen：Physical RelNode → Graph/Table API 调用
- Connector 插件与 UDF 扩展

参考：[dsl_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/2.dsl_principle.md)

## 6. 部署与作业提交（K8S 为主）复核

### 6.1 Console→K8S 提交链路（文档定义）

Console 原理文档描述“提交阶段”：Console 组装运行参数并调用 K8S Client 拉起运行时组件。

参考：[console_principle.md](file:///Users/gauss/workspace/github_project/geaflow-rs/docs/docs-cn/source/4.concepts/5.console_principle.md)

### 6.2 K8S 具体实现落点（核心入口类）

引擎 K8S 部署关键代码在 `geaflow/geaflow-deploy/geaflow-on-k8s`：
- 提交作业：创建 ConfigMap + Client Pod  
  [KubernetesJobClient](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-deploy/geaflow-on-k8s/src/main/java/org/apache/geaflow/cluster/k8s/client/KubernetesJobClient.java)
- Client Pod 入口：反射调用用户 mainClass 并清理 ConfigMap  
  [KubernetesClientRunner](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-deploy/geaflow-on-k8s/src/main/java/org/apache/geaflow/cluster/k8s/entrypoint/KubernetesClientRunner.java)
- Master/Driver/Supervisor 入口：分别启动对应组件并进入等待循环  
  [KubernetesMasterRunner](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-deploy/geaflow-on-k8s/src/main/java/org/apache/geaflow/cluster/k8s/entrypoint/KubernetesMasterRunner.java)  
  [KubernetesDriverRunner](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-deploy/geaflow-on-k8s/src/main/java/org/apache/geaflow/cluster/k8s/entrypoint/KubernetesDriverRunner.java)  
  [KubernetesSupervisorRunner](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow/geaflow-deploy/geaflow-on-k8s/src/main/java/org/apache/geaflow/cluster/k8s/entrypoint/KubernetesSupervisorRunner.java)

## 7. 平台与周边系统复核

### 7.1 Console（研发平台）

Console 属于完整研发/发布/运行/监控平台，典型能力包括：作业/版本构建、任务提交与状态管理、UI 等。

关键入口/实现：
- Spring Boot 入口：[GeaflowApplication](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-console/app/bootstrap/src/main/java/org/apache/geaflow/console/bootstrap/GeaflowApplication.java)
- 构建流水线：[GeaflowBuildPipeline](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-console/app/core/service/src/main/java/org/apache/geaflow/console/core/service/release/GeaflowBuildPipeline.java)
- 任务提交器（定时扫描 WAITING → start）：[GeaflowTaskSubmitter](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-console/app/core/service/src/main/java/org/apache/geaflow/console/core/service/task/GeaflowTaskSubmitter.java)

### 7.2 K8S Operator

Operator 用 CRD/Controller 的方式接管作业生命周期与状态管理，入口：
- [GeaflowKubernetesOperatorBootstrapApplication](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-kubernetes-operator/geaflow-kubernetes-operator-bootstrap/src/main/java/org/apache/geaflow/kubernetes/operator/bootstrap/GeaflowKubernetesOperatorBootstrapApplication.java)

### 7.3 MCP Server / AI Memory Server

仓库还包含面向 MCP 与 AI/Memory 的服务实现（Solon）：  
- MCP 入口：[GeaFlowMcpServer](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-mcp/src/main/java/org/apache/geaflow/mcp/server/GeaFlowMcpServer.java)  
- AI/Memory 入口：[GeaFlowMemoryServer](file:///Users/gauss/workspace/github_project/geaflow-rs/geaflow-ai/src/main/java/org/apache/geaflow/ai/GeaFlowMemoryServer.java)

## 8. 为 Rust 替代做准备：需要被“复刻/取舍”的能力清单（概览）

从 As-Is 视角，至少需要明确以下能力点是否要由 Rust 承接：
- 引擎核心：提交→执行计划→Cycle 调度→Worker 执行→Shuffle/流控→结果输出
- Exactly Once 的定义与边界：checkpoint 对齐点、source offset、driver 元数据一致性
- State：Graph/Key State、多存储、多模型（Static/Dynamic）、外部持久化
- 部署：local/docker/k8s/operator
- 平台：console、dashboard、权限、多租户、发布与版本管理
- DSL：SQL+GQL 编译器链路与优化器、Connector/UDF 生态

后续的 Rust To-Be 文档与 Gap Matrix 将以该清单为基准逐条对照与裁剪。

