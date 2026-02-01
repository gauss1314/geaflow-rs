use geaflow_api::function::{
    VertexCentricComputeAlgorithm, VertexCentricComputeFuncContext, VertexCentricComputeFunction,
};
use geaflow_api::graph::PGraphWindow;
use geaflow_common::types::{Edge, Vertex};
use rayon::prelude::*;
use std::collections::HashMap;

pub struct PartitionedGraph<K, VV, EV> {
    partitions: Vec<GraphPartition<K, VV, EV>>,
    all_edges: Vec<Edge<K, EV>>,
}

struct GraphPartition<K, VV, EV> {
    vertices: HashMap<K, VV>,
    adjacency: HashMap<K, Vec<Edge<K, EV>>>,
}

impl<K, VV, EV> PartitionedGraph<K, VV, EV>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    VV: Clone + Send + Sync + 'static,
    EV: Clone + Send + Sync + 'static,
{
    pub fn new(vertices: Vec<Vertex<K, VV>>, edges: Vec<Edge<K, EV>>, partitions: usize) -> Self {
        let partitions = partitions.max(1);
        let mut parts: Vec<GraphPartition<K, VV, EV>> = (0..partitions)
            .map(|_| GraphPartition {
                vertices: HashMap::new(),
                adjacency: HashMap::new(),
            })
            .collect();

        for v in vertices {
            let p = partition_of(&v.id, partitions);
            parts[p].vertices.insert(v.id, v.value);
        }

        for e in &edges {
            let p = partition_of(&e.src_id, partitions);
            parts[p]
                .adjacency
                .entry(e.src_id.clone())
                .or_default()
                .push(e.clone());
        }

        Self {
            partitions: parts,
            all_edges: edges,
        }
    }

    pub fn partitions(&self) -> usize {
        self.partitions.len()
    }
}

fn partition_of<K: std::hash::Hash>(k: &K, partitions: usize) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    (h.finish() as usize) % partitions
}

struct WorkerContext<'a, K, VV, EV, M> {
    vertex_value: Option<VV>,
    edges: &'a [Edge<K, EV>],
    outbox: &'a mut Vec<(K, M)>,
    iteration: u64,
}

impl<'a, K, VV, EV, M> VertexCentricComputeFuncContext<K, VV, EV, M>
    for WorkerContext<'a, K, VV, EV, M>
where
    K: Clone,
    VV: Clone,
    EV: Clone,
    M: Clone,
{
    fn vertex_value(&self) -> Option<&VV> {
        self.vertex_value.as_ref()
    }

    fn set_new_vertex_value(&mut self, value: VV) {
        self.vertex_value = Some(value);
    }

    fn send_message(&mut self, target_id: K, message: M) {
        self.outbox.push((target_id, message));
    }

    fn edges(&self) -> Box<dyn Iterator<Item = &geaflow_api::graph::Edge<K, EV>> + '_> {
        Box::new(self.edges.iter())
    }

    fn iteration(&self) -> u64 {
        self.iteration
    }
}

struct LocalWorker<K, VV, EV, M> {
    partition: GraphPartition<K, VV, EV>,
    inbox: HashMap<K, Vec<M>>,
    outbox: Vec<(K, M)>,
    func: Box<dyn VertexCentricComputeFunction<K, VV, EV, M>>,
}

impl<K, VV, EV> PGraphWindow<K, VV, EV> for PartitionedGraph<K, VV, EV>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    VV: Clone + Send + Sync + 'static,
    EV: Clone + Send + Sync + 'static,
{
    fn compute<M, F>(self, mut compute_function: F, _parallelism: usize) -> Self
    where
        F: VertexCentricComputeFunction<K, VV, EV, M>,
        M: Send + Sync + 'static + Clone,
    {
        let mut graph = self;

        let mut init_ctx = WorkerContext::<K, VV, EV, M> {
            vertex_value: None,
            edges: &[],
            outbox: &mut Vec::new(),
            iteration: 0,
        };
        compute_function.init(&mut init_ctx);

        let partitions = graph.partitions.len();
        let mut inbox: Vec<HashMap<K, Vec<M>>> = (0..partitions).map(|_| HashMap::new()).collect();

        let mut iteration: u64 = 1;
        loop {
            let mut outbox: Vec<(K, M)> = Vec::new();

            for part in &mut graph.partitions {
                let keys: Vec<K> = part.vertices.keys().cloned().collect();
                for vertex_id in keys {
                    let msgs = inbox[partition_of(&vertex_id, partitions)]
                        .remove(&vertex_id)
                        .unwrap_or_default();
                    let mut msg_iter = msgs.into_iter();
                    let edges = part
                        .adjacency
                        .get(&vertex_id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let v_val = part.vertices.get(&vertex_id).cloned();
                    let mut ctx = WorkerContext {
                        vertex_value: v_val,
                        edges,
                        outbox: &mut outbox,
                        iteration,
                    };
                    compute_function.compute(&vertex_id, &mut msg_iter, &mut ctx);
                    if let Some(new_v) = ctx.vertex_value {
                        part.vertices.insert(vertex_id.clone(), new_v);
                    }
                }
            }

            if outbox.is_empty() {
                break;
            }

            let mut next: Vec<HashMap<K, Vec<M>>> =
                (0..partitions).map(|_| HashMap::new()).collect();
            for (target, msg) in outbox {
                let p = partition_of(&target, partitions);
                next[p].entry(target).or_default().push(msg);
            }
            inbox = next;
            iteration += 1;
        }

        let mut finish_ctx = WorkerContext::<K, VV, EV, M> {
            vertex_value: None,
            edges: &[],
            outbox: &mut Vec::new(),
            iteration,
        };
        compute_function.finish(&mut finish_ctx);

        graph
    }

    fn compute_algorithm<M, A>(self, algorithm: &A, parallelism: usize) -> Self
    where
        A: VertexCentricComputeAlgorithm<K, VV, EV, M>,
        M: Send + Sync + 'static + Clone,
    {
        let partitions = parallelism.max(self.partitions());
        let mut graph = if partitions == self.partitions() {
            self
        } else {
            let vertices = self.vertices();
            let edges = self.edges();
            PartitionedGraph::new(vertices, edges, partitions)
        };

        let mut workers: Vec<LocalWorker<K, VV, EV, M>> = graph
            .partitions
            .drain(..)
            .map(|partition| LocalWorker {
                partition,
                inbox: HashMap::new(),
                outbox: Vec::new(),
                func: algorithm.create_function(),
            })
            .collect();

        workers.par_iter_mut().for_each(|w| {
            let mut init_ctx = WorkerContext::<K, VV, EV, M> {
                vertex_value: None,
                edges: &[],
                outbox: &mut Vec::new(),
                iteration: 0,
            };
            w.func.init(&mut init_ctx);
        });

        let max_iterations = algorithm.iterations();
        let partitions = workers.len();
        let mut iteration: u64 = 1;

        while iteration <= max_iterations {
            workers.par_iter_mut().for_each(|w| {
                w.outbox.clear();
                let keys: Vec<K> = w.partition.vertices.keys().cloned().collect();
                for vertex_id in keys {
                    let msgs = w.inbox.remove(&vertex_id).unwrap_or_default();
                    let mut msg_iter = msgs.into_iter();
                    let edges = w
                        .partition
                        .adjacency
                        .get(&vertex_id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let v_val = w.partition.vertices.get(&vertex_id).cloned();
                    let mut ctx = WorkerContext {
                        vertex_value: v_val,
                        edges,
                        outbox: &mut w.outbox,
                        iteration,
                    };
                    w.func.compute(&vertex_id, &mut msg_iter, &mut ctx);
                    if let Some(new_v) = ctx.vertex_value {
                        w.partition.vertices.insert(vertex_id.clone(), new_v);
                    }
                }
            });

            let mut any_msg = false;
            let mut next_inboxes: Vec<HashMap<K, Vec<M>>> =
                (0..partitions).map(|_| HashMap::new()).collect();

            for w in &mut workers {
                for (target, msg) in w.outbox.drain(..) {
                    let p = partition_of(&target, partitions);
                    next_inboxes[p].entry(target).or_default().push(msg);
                    any_msg = true;
                }
            }

            for (i, w) in workers.iter_mut().enumerate() {
                w.inbox = std::mem::take(&mut next_inboxes[i]);
            }

            iteration += 1;
            if !any_msg {
                break;
            }
        }

        workers.par_iter_mut().for_each(|w| {
            let mut finish_ctx = WorkerContext::<K, VV, EV, M> {
                vertex_value: None,
                edges: &[],
                outbox: &mut Vec::new(),
                iteration,
            };
            w.func.finish(&mut finish_ctx);
        });

        let mut merged_vertices: HashMap<K, VV> = HashMap::new();
        for w in workers {
            for (k, v) in w.partition.vertices {
                merged_vertices.insert(k, v);
            }
        }

        PartitionedGraph::new(
            merged_vertices
                .into_iter()
                .map(|(id, value)| Vertex { id, value })
                .collect(),
            graph.all_edges.clone(),
            partitions,
        )
    }

    fn vertices(&self) -> Vec<Vertex<K, VV>> {
        self.partitions
            .iter()
            .flat_map(|p| {
                p.vertices.iter().map(|(k, v)| Vertex {
                    id: k.clone(),
                    value: v.clone(),
                })
            })
            .collect()
    }

    fn edges(&self) -> Vec<Edge<K, EV>> {
        self.all_edges.clone()
    }
}
