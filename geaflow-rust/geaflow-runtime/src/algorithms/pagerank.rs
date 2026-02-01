use geaflow_api::function::{
    VertexCentricComputeAlgorithm, VertexCentricComputeFuncContext, VertexCentricComputeFunction,
};

#[derive(Clone)]
pub struct PageRankAlgorithm {
    pub iterations: u64,
    pub alpha: f64,
}

impl PageRankAlgorithm {
    pub fn new(iterations: u64, alpha: f64) -> Self {
        Self { iterations, alpha }
    }
}

#[derive(Clone)]
pub struct PageRankFunction {
    alpha: f64,
    teleport: Option<f64>,
}

impl VertexCentricComputeFunction<u64, f64, u8, f64> for PageRankFunction {
    fn compute(
        &mut self,
        _vertex_id: &u64,
        message_iterator: &mut dyn Iterator<Item = f64>,
        context: &mut dyn VertexCentricComputeFuncContext<u64, f64, u8, f64>,
    ) {
        let vertex_value = context.vertex_value().cloned().unwrap_or(0.0);
        let edges: Vec<_> = context.edges().cloned().collect();
        let out_degree = edges.len() as f64;
        if self.teleport.is_none() && vertex_value > 0.0 {
            self.teleport = Some((1.0 - self.alpha) * vertex_value);
        }

        if context.iteration() == 1 {
            if out_degree > 0.0 {
                let msg = vertex_value / out_degree;
                for e in edges {
                    context.send_message(e.target_id, msg);
                }
            }
            return;
        }

        let mut sum = 0.0;
        for m in message_iterator {
            sum += m;
        }
        let pr = sum * self.alpha + self.teleport.unwrap_or(1.0 - self.alpha);
        context.set_new_vertex_value(pr);

        if out_degree > 0.0 {
            let msg = pr / out_degree;
            for e in edges {
                context.send_message(e.target_id, msg);
            }
        }
    }
}

impl VertexCentricComputeAlgorithm<u64, f64, u8, f64> for PageRankAlgorithm {
    fn name(&self) -> &str {
        "pagerank"
    }

    fn iterations(&self) -> u64 {
        self.iterations
    }

    fn create_function(&self) -> Box<dyn VertexCentricComputeFunction<u64, f64, u8, f64>> {
        Box::new(PageRankFunction {
            alpha: self.alpha,
            teleport: None,
        })
    }
}
