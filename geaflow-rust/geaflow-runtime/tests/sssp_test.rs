use geaflow_api::function::{VertexCentricComputeFuncContext, VertexCentricComputeFunction};
use geaflow_api::graph::PGraphWindow;
use geaflow_common::types::{Edge, Vertex};
use geaflow_runtime::graph::mem_graph::InMemoryGraph;

#[derive(Clone)]
struct SSSPAlgorithm {
    src_id: i32,
}

impl VertexCentricComputeFunction<i32, i32, i32, i32> for SSSPAlgorithm {
    fn compute(
        &mut self,
        vertex_id: &i32,
        message_iterator: &mut dyn Iterator<Item = i32>,
        context: &mut dyn VertexCentricComputeFuncContext<i32, i32, i32, i32>,
    ) {
        let current_val = context.vertex_value().cloned().unwrap_or(i32::MAX);
        let mut min_dist = current_val;
        let mut updated = false;

        // If source, dist is 0 initially (handled in init usually, or here check iteration)
        if context.iteration() == 1 && *vertex_id == self.src_id {
            min_dist = 0;
            updated = true;
        }

        for msg in message_iterator {
            if msg < min_dist {
                min_dist = msg;
                updated = true;
            }
        }

        if updated {
            context.set_new_vertex_value(min_dist);
            let edges: Vec<_> = context.edges().cloned().collect();
            for edge in edges {
                context.send_message(edge.target_id, min_dist + edge.value);
            }
        }
    }
}

#[test]
fn test_sssp() {
    let vertices = vec![
        Vertex::new(1, i32::MAX),
        Vertex::new(2, i32::MAX),
        Vertex::new(3, i32::MAX),
    ];

    let edges = vec![
        Edge::new(1, 2, 10),
        Edge::new(2, 3, 20),
        Edge::new(1, 3, 100), // longer path
    ];

    let graph = InMemoryGraph::new(vertices, edges);
    let result_graph = graph.compute(SSSPAlgorithm { src_id: 1 }, 1);

    let vertices = result_graph.vertices();
    let v3 = vertices.iter().find(|v| v.id == 3).unwrap();
    assert_eq!(v3.value, 30); // 1->2->3 (10+20=30) < 1->3 (100)
}
