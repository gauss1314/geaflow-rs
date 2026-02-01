use geaflow_api::function::{
    VertexCentricComputeAlgorithm, VertexCentricComputeFuncContext, VertexCentricComputeFunction,
};
use geaflow_api::graph::PGraphWindow;
use geaflow_common::types::{Edge, Vertex};
use std::collections::HashMap;

pub struct InMemoryGraph<K, VV, EV> {
    vertices: HashMap<K, VV>,
    edges: Vec<Edge<K, EV>>,
    adjacency: HashMap<K, Vec<Edge<K, EV>>>,
}

impl<K, VV, EV> InMemoryGraph<K, VV, EV>
where
    K: Clone + std::hash::Hash + Eq + Send + Sync + 'static,
    VV: Clone + Send + Sync + 'static,
    EV: Clone + Send + Sync + 'static,
{
    pub fn new(vertices: Vec<Vertex<K, VV>>, edges: Vec<Edge<K, EV>>) -> Self {
        let mut v_map = HashMap::new();
        for v in vertices {
            v_map.insert(v.id, v.value);
        }
        let mut adjacency: HashMap<K, Vec<Edge<K, EV>>> = HashMap::new();
        for e in &edges {
            adjacency
                .entry(e.src_id.clone())
                .or_default()
                .push(e.clone());
        }
        Self {
            vertices: v_map,
            adjacency,
            edges,
        }
    }
}

struct InMemoryContext<K, VV, EV, M> {
    vertex_value: Option<VV>,
    edges: Vec<Edge<K, EV>>,
    out_messages: Vec<(K, M)>,
    iteration: u64,
}

impl<K, VV, EV, M> VertexCentricComputeFuncContext<K, VV, EV, M> for InMemoryContext<K, VV, EV, M>
where
    K: Clone,
    VV: Clone,
    M: Clone,
    EV: Clone,
{
    fn vertex_value(&self) -> Option<&VV> {
        self.vertex_value.as_ref()
    }

    fn set_new_vertex_value(&mut self, value: VV) {
        self.vertex_value = Some(value);
    }

    fn send_message(&mut self, target_id: K, message: M) {
        self.out_messages.push((target_id, message));
    }

    fn edges(&self) -> Box<dyn Iterator<Item = &geaflow_api::graph::Edge<K, EV>> + '_> {
        Box::new(self.edges.iter())
    }

    fn iteration(&self) -> u64 {
        self.iteration
    }
}

impl<K, VV, EV> PGraphWindow<K, VV, EV> for InMemoryGraph<K, VV, EV>
where
    K: Clone + std::hash::Hash + Eq + Send + Sync + 'static,
    VV: Clone + Send + Sync + 'static,
    EV: Clone + Send + Sync + 'static,
{
    fn compute<M, F>(mut self, mut compute_function: F, _parallelism: usize) -> Self
    where
        F: VertexCentricComputeFunction<K, VV, EV, M>,
        M: Clone + Send + Sync + 'static,
    {
        self.run_with_function(&mut compute_function, None)
    }

    fn compute_algorithm<M, A>(mut self, algorithm: &A, _parallelism: usize) -> Self
    where
        A: VertexCentricComputeAlgorithm<K, VV, EV, M>,
        M: Send + Sync + 'static + Clone,
    {
        let mut func = algorithm.create_function();
        self.run_with_function(&mut *func, Some(algorithm.iterations()))
    }

    fn vertices(&self) -> Vec<Vertex<K, VV>> {
        self.vertices
            .iter()
            .map(|(k, v)| Vertex::new(k.clone(), v.clone()))
            .collect()
    }

    fn edges(&self) -> Vec<Edge<K, EV>> {
        self.edges.clone()
    }
}

impl<K, VV, EV> InMemoryGraph<K, VV, EV>
where
    K: Clone + std::hash::Hash + Eq + Send + Sync + 'static,
    VV: Clone + Send + Sync + 'static,
    EV: Clone + Send + Sync + 'static,
{
    fn run_with_function<M>(
        &mut self,
        compute_function: &mut dyn VertexCentricComputeFunction<K, VV, EV, M>,
        max_iterations: Option<u64>,
    ) -> Self
    where
        M: Clone + Send + Sync + 'static,
    {
        let mut messages: HashMap<K, Vec<M>> = HashMap::new();

        let mut init_ctx = InMemoryContext::<K, VV, EV, M> {
            vertex_value: None,
            edges: Vec::new(),
            out_messages: Vec::new(),
            iteration: 0,
        };
        compute_function.init(&mut init_ctx);

        let mut iteration: u64 = 1;
        let max_iterations = max_iterations.unwrap_or(u64::MAX);

        while iteration <= max_iterations {
            let mut next_messages: HashMap<K, Vec<M>> = HashMap::new();
            let mut msg_count = 0usize;

            let all_keys: Vec<K> = self.vertices.keys().cloned().collect();

            for v_id in all_keys {
                let v_val = self.vertices.get(&v_id).cloned();
                let edges = self.adjacency.get(&v_id).cloned().unwrap_or_default();

                let mut ctx = InMemoryContext::<K, VV, EV, M> {
                    vertex_value: v_val,
                    edges,
                    out_messages: Vec::new(),
                    iteration,
                };

                let msgs = messages.remove(&v_id).unwrap_or_default();
                let mut msg_iter = msgs.into_iter();

                compute_function.compute(&v_id, &mut msg_iter, &mut ctx);

                if let Some(new_val) = ctx.vertex_value {
                    self.vertices.insert(v_id.clone(), new_val);
                }

                for (target, msg) in ctx.out_messages {
                    next_messages.entry(target).or_default().push(msg);
                    msg_count += 1;
                }
            }

            messages = next_messages;
            iteration += 1;

            if msg_count == 0 {
                break;
            }
        }

        let mut finish_ctx = InMemoryContext::<K, VV, EV, M> {
            vertex_value: None,
            edges: Vec::new(),
            out_messages: Vec::new(),
            iteration,
        };
        compute_function.finish(&mut finish_ctx);

        InMemoryGraph {
            vertices: std::mem::take(&mut self.vertices),
            edges: std::mem::take(&mut self.edges),
            adjacency: std::mem::take(&mut self.adjacency),
        }
    }
}
