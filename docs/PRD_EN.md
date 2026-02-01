# GeaFlow Rust Runtime — Product Requirements Document (PRD)

This document summarizes the product requirements for the Rust-only GeaFlow runtime in this repository: target users, use cases, scope, functional and non-functional requirements, acceptance criteria, and roadmap direction.

## 1. Background
GeaFlow Rust Runtime is a Rust re-implementation of GeaFlow (Java) core capabilities, focusing on a production-oriented subset: a **Vertex-Centric (BSP/Superstep)** graph computing engine.

This repository is not intended to replace the full Java product stack (DSL/Console/Operator/Dashboard). It focuses on a standalone, deployable, and regression-testable Rust runtime.

## 2. Goals and Non-Goals
### 2.1 Goals
- Provide a standalone BSP graph computing engine (Local and Distributed)
- Provide a stable job model and submission workflow (JobSpec/CLI)
- Provide recoverable stateful computation (RocksDB + checkpoint/recovery)
- Provide minimal operations and observability surface (health/list/metrics)
- Provide regression-friendly verification (WCC/PR/SSSP + Graph500 end-to-end verification)

### 2.2 Non-Goals (Explicitly Out of Scope)
- SQL + GQL DSL compilation and optimization pipeline
- Console platform and job management system
- Kubernetes Operator / Dashboard UI
- AI/MCP and related side services
- The full Java state ecosystem (multi-store/multi-model, Redis, external persistence, index pushdown, etc.)

## 3. Target Users and Use Cases
### 3.1 Target Users
- Graph algorithm engineers: develop and validate BSP algorithms such as WCC/PageRank/SSSP
- Data engineers: run graph jobs locally or on a small cluster and export results
- Platform engineers: build/operate a deployable, observable, recoverable Rust runtime foundation

### 3.2 Use Cases
- Run algorithms locally with parallelism for development and regression
- Validate distributed execution with multiple workers
- Run Graph500 dataset end-to-end verification (code path or HTTP/curl path)

## 4. Product Scope (Rust-only Feature Set)
### 4.1 Execution Modes
- Local: single-machine parallel execution
- Distributed: TCP RPC driver/worker; optional master

### 4.2 State and Fault Tolerance
- RocksDB state backend
- Checkpoint/recovery aligned at superstep boundaries (under file-source semantics)
- Failure semantics (current stage): fail-fast (driver fails immediately when a worker fails)

### 4.3 Operations and Observability
- Master: HTTP `GET /healthz`, `GET /workers`
- Driver: HTTP `GET /healthz`, `GET /jobs`
- Worker: optional Prometheus metrics (when enabled)

### 4.4 Algorithms and Regression
- WCC / PageRank / SSSP
- Integration tests covering local/distributed and fault-injection scenarios (`geaflow-rust/geaflow-runtime/tests`)

### 4.5 Graph500 End-to-End Verification
Two verification paths (both include load → compute → compare with truth):
- Code path: `geaflow-graph500` runner
- API path: `geaflow-graph500 serve` + curl to register dataset paths and submit verification jobs

## 5. Key User Journeys
### 5.1 Local
1) Prepare CSV vertices/edges\n2) Run `geaflow-submit --mode local`\n3) Consume outputs (stdout or files)

### 5.2 Distributed
1) Start master (optional)\n2) Start N workers (distinct state_dir)\n3) Start driver\n4) Submit job via `geaflow-submit --mode distributed --driver ...`\n5) Check `/jobs` on driver for job list (read-only)\n6) Fetch job outputs when finished

### 5.3 Graph500 Verification
Path A (runner): extract dataset → run WCC/PR → compare with truth\nPath B (HTTP): start service → register dataset paths → submit job → query job result report

## 6. Functional Requirements (By Module)
### 6.1 Job Model (JobSpec)
Requirements:
- Describe graph inputs, algorithm parameters, execution mode, checkpoint policy
- Serializable and generated/validated by CLI
- Driver executes with JobSpec as the source of truth

Acceptance:
- CLI supports dry-run (prints/validates JobSpec)
- Local and Distributed share the same job model and execution path

### 6.2 Distributed Components
Requirements:
- Master (optional): worker registration/heartbeat, list workers
- Driver: accept JobSpec, drive supersteps, maintain job list
- Worker: load partitions, execute supersteps, hold RocksDB state

Acceptance:
- Multi-worker distributed runs for WCC/PR/SSSP
- Protocol supports batching to avoid oversized frames

### 6.3 Checkpoint/Recovery
Requirements:
- Initiate checkpoint at superstep boundaries, persist driver metadata and RocksDB snapshots
- Recover from the latest checkpoint and continue execution (test-driven)

Acceptance:
- Distributed checkpoint recovery integration test passes

### 6.4 Ops Endpoints
Requirements:
- Health checks: `/healthz`
- List endpoints: master `/workers`, driver `/jobs`
- Lightweight implementation (no heavy HTTP framework required)

Acceptance:
- curl works and list endpoints return JSON

### 6.5 Graph500 Verification API
Requirements:
- No large file upload; register local file paths instead
- Asynchronous job submission and job status/query APIs

Acceptance:
- WCC: mismatches=0, unexpected=0, missing=0
- PageRank: unexpected=0, missing=0, max_abs_diff <= 1e-6

## 7. Non-Functional Requirements
- Correctness: tests + Graph500 truth comparison as hard gates
- Reproducibility: documented commands are copy-paste runnable
- Maintainability: clear module boundaries (plan/scheduler/distributed/state/io/http)
- Performance (phased): batch loading and incremental writes for large graphs; avoid driver memory scaling linearly with message volume (roadmap)
- Security: do not leak secrets via logs/APIs (default is local dev usage)

## 8. Constraints and Assumptions
- File-based sources (no offsets): consistency focuses on compute state at superstep boundaries
- Fail-fast is the default failure strategy in the current stage; HA/failover is a future enhancement
- Ops and Graph500 APIs are intended for dev/regression environments, not public internet exposure by default

## 9. Acceptance Criteria
### 9.1 Functional
- Local: WCC/PageRank/SSSP run end-to-end
- Distributed: driver/worker run end-to-end
- Checkpoint/recovery: recovery test passes
- Fault injection: worker crash triggers fail-fast without deadlock
- Ops endpoints: healthz and list endpoints available
- Graph500: truth comparison passes for WCC and PageRank

### 9.2 Engineering
- `cargo fmt -- --check`\n- `cargo clippy --all-targets -- -D warnings`\n- `cargo test`

## 10. Risks and Roadmap Direction
### 10.1 Risks
- State/edge layout and read/write amplification for large graphs\n- Driver-aggregated shuffle memory pressure\n- Checkpoint metadata consistency and recovery semantics when more source types are introduced

### 10.2 Roadmap Direction
- Stronger shuffle/backpressure strategies (evolve from driver-aggregated to worker-to-worker transport)\n- Scheduler state machine (Init/Load/Barrier/Commit/Recover)\n- Richer ops APIs (job details/logs/metrics aggregation)\n- Stream/Window capabilities evolve from skeleton to a usable subset

