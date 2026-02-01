use geaflow_api::pipeline::{PipelineResult, PipelineTask, PipelineTaskContext};
use geaflow_common::config::Configuration;
use geaflow_common::error::{GeaFlowError, GeaFlowResult};
use geaflow_common::types::{Edge, Vertex};

use crate::graph::mem_graph::InMemoryGraph;
use crate::graph::partitioned_graph::PartitionedGraph;

#[derive(Debug, Clone)]
pub enum RuntimeMode {
    Local,
    Distributed,
}

#[derive(Debug, Clone)]
pub struct Environment {
    pub mode: RuntimeMode,
    pub config: Configuration,
}

impl Environment {
    pub fn local() -> Self {
        Self {
            mode: RuntimeMode::Local,
            config: Configuration::new(),
        }
    }

    pub fn distributed() -> Self {
        Self {
            mode: RuntimeMode::Distributed,
            config: Configuration::new(),
        }
    }
}

pub struct LocalPipeline {
    env: Environment,
    tasks: Vec<Box<dyn PipelineTask>>,
}

impl LocalPipeline {
    pub fn new(env: Environment) -> Self {
        Self {
            env,
            tasks: Vec::new(),
        }
    }

    pub fn submit<T: PipelineTask>(&mut self, task: T) -> &mut Self {
        self.tasks.push(Box::new(task));
        self
    }

    pub fn execute(&mut self) -> PipelineResult {
        let mut ctx = LocalTaskContext {
            name: "local",
            env: self.env.clone(),
        };

        for task in &self.tasks {
            if let Err(e) = task.run(&mut ctx) {
                return PipelineResult::failure(format!("task {} failed: {}", task.name(), e));
            }
        }

        PipelineResult::success()
    }
}

pub struct LocalTaskContext {
    name: &'static str,
    pub env: Environment,
}

impl LocalTaskContext {
    pub fn build_in_memory_graph<K, VV, EV>(
        &self,
        vertices: Vec<Vertex<K, VV>>,
        edges: Vec<Edge<K, EV>>,
    ) -> InMemoryGraph<K, VV, EV>
    where
        K: Clone + std::hash::Hash + Eq + Send + Sync + 'static,
        VV: Clone + Send + Sync + 'static,
        EV: Clone + Send + Sync + 'static,
    {
        InMemoryGraph::new(vertices, edges)
    }

    pub fn build_partitioned_graph<K, VV, EV>(
        &self,
        vertices: Vec<Vertex<K, VV>>,
        edges: Vec<Edge<K, EV>>,
        partitions: usize,
    ) -> PartitionedGraph<K, VV, EV>
    where
        K: Clone + std::hash::Hash + Eq + Send + Sync + 'static,
        VV: Clone + Send + Sync + 'static,
        EV: Clone + Send + Sync + 'static,
    {
        PartitionedGraph::new(vertices, edges, partitions)
    }
}

impl PipelineTaskContext for LocalTaskContext {
    fn name(&self) -> &str {
        self.name
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

pub fn ensure_local(env: &Environment) -> GeaFlowResult<()> {
    match env.mode {
        RuntimeMode::Local => Ok(()),
        RuntimeMode::Distributed => Err(GeaFlowError::InvalidArgument(
            "environment mode is distributed; local pipeline required".to_string(),
        )),
    }
}
