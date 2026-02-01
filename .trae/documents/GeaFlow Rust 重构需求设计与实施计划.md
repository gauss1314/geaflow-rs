# GeaFlow Rust 重构计划

根据您的要求，我将首先提供详细的需求设计文档，然后制定 Rust 实现的重构计划。

## 1. 需求设计文档 (Requirement Design Document)

### 1.1 项目概述
GeaFlow 是一个分布式流式图计算引擎，核心能力在于统一了图处理和流处理，支持在动态图上进行实时计算。本次任务的目标是使用 Rust 语言重构其核心引擎，以利用 Rust 的内存安全和高性能特性。

### 1.2 核心概念与数据模型
- **Graph View (图视图)**: 随时间变化的图数据的抽象。支持静态图和动态图。
- **Stream (流)**: 无界的数据序列。
- **Vertex (点)**: 图的基本单元，包含 ID (`K`) 和值 (`VV`)。
- **Edge (边)**: 连接点的线，包含源 ID、目标 ID 和值 (`EV`)。
- **Window (窗口)**: 流处理的时间或大小切片，用于驱动增量图计算。

### 1.3 核心功能需求
1.  **API 层 (geaflow-api)**:
    -   提供与 Java 版本一致的编程接口（Trait 定义）。
    -   **DataStream API**: `map`, `filter`, `key_by`, `window` 等流式算子。
    -   **Graph API**: `PGraphWindow` 接口，支持 `compute` (图计算) 和 `traversal` (图遍历)。
    -   **Function 接口**: `VertexCentricComputeFunction` (核心 VC 模型)，支持 `init`, `compute`, `finish` 生命周期。
2.  **运行时层 (geaflow-runtime)**:
    -   **作业调度**: 解析逻辑计划并调度任务。
    -   **数据流转**: 实现算子间的数据传递（内存/网络）。
    -   **状态管理**: 支持图状态的存储与访问（类似 RocksDB 后端）。
3.  **计算模型**:
    -   支持 **Vertex-Centric (VC)** 迭代计算模型。
    -   支持 **Incremental Graph Computing (增量图计算)**，仅对窗口内变化的子图进行计算。

### 1.4 非功能需求
-   **一致性**: Rust 接口命名和语义需尽量与 Java 原版保持一致，降低迁移成本。
-   **安全性**: 利用 Rust 的所有权机制避免并发数据竞争。
-   **性能**: 追求零成本抽象，优化消息传递和内存布局。

---

## 2. Rust 实现计划 (Implementation Plan)

考虑到 GeaFlow 项目的庞大体量，我们将分阶段进行重构。当前阶段主要聚焦于 **核心 API 定义** 和 **基础运行时** 的原型实现。

### 阶段一：项目初始化与 API 定义
1.  **环境搭建**: 在当前目录下创建 `geaflow-rust` 子目录作为 Rust 项目根目录（避免干扰现有 Java 代码）。
2.  **Crate 划分**:
    -   `geaflow-api`: 定义核心 Trait 和数据结构。
    -   `geaflow-common`: 通用工具类、配置定义。
    -   `geaflow-runtime`: 核心执行引擎。
3.  **API 实现**:
    -   定义 `Vertex`, `Edge` 泛型结构体。
    -   定义 `VertexCentricComputeFunction` Trait。
    -   定义 `PStream` 和 `PGraphWindow` Trait。

### 阶段二：基础运行时实现 (Local Runtime)
1.  **单机执行器**: 实现一个简单的单机运行时，能够构建和执行 DAG。
2.  **消息传递**: 实现基于内存的 Channel 机制模拟分布式消息发送。
3.  **状态模拟**: 使用 `HashMap` 或 `DashMap` 模拟内存图状态存储。

### 阶段三：功能验证 (Test Cases)
1.  **单元测试**: 为每个核心数据结构和 Trait 编写测试。
2.  **集成测试**: 移植 Java 版的 `Connected Components` (连通图) 或 `PageRank` 算法作为测试用例，验证计算结果的一致性。

### 下一步行动
我将开始执行**阶段一**，创建 Rust 项目结构并编写 `geaflow-api` 的核心代码。
